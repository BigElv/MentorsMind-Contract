import { StrKey, xdr } from "stellar-sdk";
import {
  type ContractEvent,
  EVENT_TOPICS,
  type EventMetadata,
  type HorizonEvent,
} from "./event-types";

function getMetadata(raw: HorizonEvent): EventMetadata {
  return {
    contractId: raw.contract_id ?? raw.contractId,
    ledgerSequence: raw.ledger_sequence ?? raw.ledgerSequence,
    createdAt: raw.created_at ?? raw.createdAt,
    transactionHash: raw.transaction_hash ?? raw.transactionHash,
    pagingToken: raw.paging_token ?? raw.pagingToken,
  };
}

function toBigInt(value: unknown): bigint | null {
  if (typeof value === "bigint") return value;
  if (typeof value === "number") return BigInt(value);
  if (typeof value === "string" && value.length > 0) {
    try {
      return BigInt(value);
    } catch {
      return null;
    }
  }
  if (value && typeof value === "object" && "toString" in value) {
    try {
      return BigInt(String(value));
    } catch {
      return null;
    }
  }
  return null;
}

function toStringValue(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "bigint") {
    return String(value);
  }
  if (value === null || value === undefined) return "";
  return String(value);
}

function toNumberValue(value: unknown): number {
  if (typeof value === "number") return value;
  if (typeof value === "bigint") return Number(value);
  if (typeof value === "string") return Number(value);
  if (value && typeof value === "object" && "toString" in value) {
    return Number(String(value));
  }
  return 0;
}

function readField<T = unknown>(data: unknown, key: string, index: number): T | undefined {
  if (Array.isArray(data)) {
    return data[index] as T;
  }

  if (data && typeof data === "object") {
    const candidate = (data as Record<string, unknown>)[key];
    return candidate as T;
  }

  return undefined;
}

function normalizeSymbol(value: unknown): string {
  return toStringValue(value).trim();
}

function normalizeForMatch(value: unknown): string {
  return normalizeSymbol(value).toLowerCase();
}

function parseAddress(scAddress: any): string {
  try {
    const arm = String(scAddress.switch().name).toLowerCase();

    if (arm.includes("account")) {
      const accountId = typeof scAddress.accountId === "function" ? scAddress.accountId() : scAddress.accountId;
      const ed25519 = typeof accountId.ed25519 === "function" ? accountId.ed25519() : accountId;
      return StrKey.encodeEd25519PublicKey(Buffer.from(ed25519));
    }

    if (arm.includes("contract")) {
      const contractId = typeof scAddress.contractId === "function" ? scAddress.contractId() : scAddress.contractId;
      const maybeEncodeContract = (StrKey as unknown as { encodeContract?: (bytes: Buffer) => string })
        .encodeContract;
      if (typeof maybeEncodeContract === "function") {
        return maybeEncodeContract(Buffer.from(contractId));
      }
      return toStringValue(contractId);
    }
  } catch {
    // Fall through to string conversion.
  }

  return toStringValue(scAddress);
}

function parseI128Parts(parts: any, signed: boolean): string {
  const hiRaw = typeof parts.hi === "function" ? parts.hi() : parts.hi;
  const loRaw = typeof parts.lo === "function" ? parts.lo() : parts.lo;

  const hi = toBigInt(hiRaw) ?? 0n;
  const lo = toBigInt(loRaw) ?? 0n;

  let value = (hi << 64n) + lo;

  if (signed && value >= (1n << 127n)) {
    value -= 1n << 128n;
  }

  return value.toString();
}

function scValToNative(scVal: any): unknown {
  const arm = String(scVal.switch().name).toLowerCase();

  if (arm.includes("bool")) return scVal.b();
  if (arm.includes("void")) return null;
  if (arm.includes("u32")) return scVal.u32();
  if (arm.includes("i32")) return scVal.i32();
  if (arm.includes("u64")) return toStringValue(scVal.u64());
  if (arm.includes("i64")) return toStringValue(scVal.i64());
  if (arm.includes("u128")) return parseI128Parts(scVal.u128(), false);
  if (arm.includes("i128")) return parseI128Parts(scVal.i128(), true);
  if (arm.includes("symbol")) return toStringValue(scVal.sym());
  if (arm.includes("string")) return toStringValue(scVal.str());
  if (arm.includes("bytes")) return Buffer.from(scVal.bytes()).toString("hex");
  if (arm.includes("address")) return parseAddress(scVal.address());

  if (arm.includes("vec")) {
    const values = scVal.vec() ?? [];
    return values.map((entry: any) => scValToNative(entry));
  }

  if (arm.includes("map")) {
    const result: Record<string, unknown> = {};
    const entries = scVal.map() ?? [];

    for (const entry of entries) {
      const keyVal = typeof entry.key === "function" ? entry.key() : entry[0];
      const dataVal = typeof entry.val === "function" ? entry.val() : entry[1];
      const key = normalizeSymbol(scValToNative(keyVal));
      result[key] = scValToNative(dataVal);
    }

    return result;
  }

  return toStringValue(scVal);
}

function decodeScValXdr(value: string): unknown {
  const ScValCtor = (xdr as unknown as { ScVal?: { fromXDR: (v: string, enc: string) => unknown } }).ScVal;
  if (!ScValCtor?.fromXDR) {
    return value;
  }
  const scVal = ScValCtor.fromXDR(value, "base64");
  return scValToNative(scVal);
}

function decodeTopics(raw: HorizonEvent): unknown[] {
  if (Array.isArray(raw.topics)) return raw.topics;
  if (Array.isArray(raw.topic)) return raw.topic;

  if (Array.isArray(raw.topic_xdr)) {
    return raw.topic_xdr.map((topic) => decodeScValXdr(topic));
  }

  if (typeof raw.topic_xdr === "string" && raw.topic_xdr.length > 0) {
    const chunks = raw.topic_xdr
      .split(",")
      .map((chunk) => chunk.trim())
      .filter(Boolean);
    return chunks.map((topic) => decodeScValXdr(topic));
  }

  return [];
}

function decodeData(raw: HorizonEvent): unknown {
  if (raw.data !== undefined) return raw.data;
  if (raw.value !== undefined) return raw.value;
  if (typeof raw.value_xdr === "string" && raw.value_xdr.length > 0) {
    return decodeScValXdr(raw.value_xdr);
  }
  return null;
}

function warnUnknown(raw: HorizonEvent, topics: unknown[]): null {
  const contractId = raw.contract_id ?? raw.contractId ?? "unknown-contract";
  console.warn(
    `[contracts-sdk/events] Unknown event: contract=${contractId} topics=${JSON.stringify(topics)}`
  );
  return null;
}

export function decodeEvent(raw: HorizonEvent): ContractEvent | null {
  try {
    const topics = decodeTopics(raw);
    const data = decodeData(raw);
    const metadata = getMetadata(raw);

    if (topics.length === 0) {
      return warnUnknown(raw, topics);
    }

    const topic0 = normalizeForMatch(topics[0]);
    const topic1 = normalizeForMatch(topics[1]);
    const topic2 = topics[2];

    if (topic0 === normalizeForMatch(EVENT_TOPICS.ESCROW_CREATED[0])) {
      const escrowId = toNumberValue(topic2);

      if (topic1 === normalizeForMatch(EVENT_TOPICS.ESCROW_CREATED[1]) || topic1 === normalizeForMatch(EVENT_TOPICS.ESCROW_CREATED_LEGACY[1])) {
        return {
          kind: "EscrowCreated",
          escrowId,
          mentor: toStringValue(readField(data, "mentor", 0)),
          learner: toStringValue(readField(data, "learner", 1)),
          amount: toStringValue(readField(data, "amount", 2)),
          sessionId: toStringValue(readField(data, "session_id", 3)),
          tokenAddress: toStringValue(readField(data, "token_address", 4)),
          sessionEndTime: toNumberValue(readField(data, "session_end_time", 5)),
          ...metadata,
        };
      }

      if (topic1 === normalizeForMatch(EVENT_TOPICS.ESCROW_RELEASED[1]) || topic1 === normalizeForMatch(EVENT_TOPICS.ESCROW_RELEASED_LEGACY[1])) {
        return {
          kind: "EscrowReleased",
          escrowId,
          mentor: toStringValue(readField(data, "mentor", 0)),
          amount: toStringValue(readField(data, "amount", 1)),
          netAmount: toStringValue(readField(data, "net_amount", 2)),
          platformFee: toStringValue(readField(data, "platform_fee", 3)),
          tokenAddress: toStringValue(readField(data, "token_address", 4)),
          ...metadata,
        };
      }

      if (topic1 === normalizeForMatch(EVENT_TOPICS.ESCROW_PARTIAL_RELEASED[1])) {
        return {
          kind: "EscrowPartialReleased",
          escrowId,
          mentor: toStringValue(readField(data, "mentor", 0)),
          releasedAmount: toStringValue(readField(data, "released_amount", 1)),
          netAmount: toStringValue(readField(data, "net_amount", 2)),
          platformFee: toStringValue(readField(data, "platform_fee", 3)),
          tokenAddress: toStringValue(readField(data, "token_address", 4)),
          remainingAmount: toStringValue(readField(data, "remaining_amount", 5)),
          ...metadata,
        };
      }

      if (topic1 === normalizeForMatch(EVENT_TOPICS.ESCROW_ADMIN_RELEASE[1])) {
        return {
          kind: "EscrowAdminRelease",
          escrowId,
          time: toNumberValue(readField(data, "time", 1) ?? readField(data, "time", 0)),
          ...metadata,
        };
      }

      if (topic1 === normalizeForMatch(EVENT_TOPICS.ESCROW_AUTO_RELEASED[1]) || topic1 === normalizeForMatch(EVENT_TOPICS.ESCROW_AUTO_RELEASED_LEGACY[1])) {
        return {
          kind: "EscrowAutoReleased",
          escrowId,
          time: toNumberValue(readField(data, "time", 1) ?? readField(data, "time", 0)),
          ...metadata,
        };
      }

      if (topic1 === normalizeForMatch(EVENT_TOPICS.DISPUTE_OPENED[1]) || topic1 === normalizeForMatch(EVENT_TOPICS.DISPUTE_OPENED_LEGACY[1])) {
        return {
          kind: "DisputeOpened",
          escrowId,
          caller: toStringValue(readField(data, "caller", 1) ?? readField(data, "caller", 0)),
          reason: toStringValue(readField(data, "reason", 2) ?? readField(data, "reason", 1)),
          tokenAddress: toStringValue(readField(data, "token_address", 3) ?? readField(data, "token_address", 2)),
          ...metadata,
        };
      }

      if (topic1 === normalizeForMatch(EVENT_TOPICS.DISPUTE_RESOLVED[1]) || topic1 === normalizeForMatch(EVENT_TOPICS.DISPUTE_RESOLVED_LEGACY[1])) {
        return {
          kind: "DisputeResolved",
          escrowId,
          mentorPct: toNumberValue(readField(data, "mentor_pct", 1) ?? readField(data, "mentor_pct", 0)),
          mentorAmount: toStringValue(readField(data, "mentor_amount", 2) ?? readField(data, "mentor_amount", 1)),
          learnerAmount: toStringValue(readField(data, "learner_amount", 3) ?? readField(data, "learner_amount", 2)),
          tokenAddress: toStringValue(readField(data, "token_address", 4) ?? readField(data, "token_address", 3)),
          time: toNumberValue(readField(data, "time", 5) ?? readField(data, "time", 4)),
          ...metadata,
        };
      }

      if (topic1 === normalizeForMatch(EVENT_TOPICS.ESCROW_REFUNDED[1]) || topic1 === normalizeForMatch(EVENT_TOPICS.ESCROW_REFUNDED_LEGACY[1])) {
        return {
          kind: "EscrowRefunded",
          escrowId,
          learner: toStringValue(readField(data, "learner", 0)),
          amount: toStringValue(readField(data, "amount", 1)),
          tokenAddress: toStringValue(readField(data, "token_address", 2)),
          ...metadata,
        };
      }

      if (topic1 === normalizeForMatch(EVENT_TOPICS.REVIEW_SUBMITTED[1]) || topic1 === normalizeForMatch(EVENT_TOPICS.REVIEW_SUBMITTED_LEGACY[1])) {
        return {
          kind: "ReviewSubmitted",
          escrowId,
          caller: toStringValue(readField(data, "caller", 0)),
          reason: toStringValue(readField(data, "reason", 1)),
          mentor: toStringValue(readField(data, "mentor", 2)),
          ...metadata,
        };
      }

      return warnUnknown(raw, topics);
    }

    if (topic0 === normalizeForMatch(EVENT_TOPICS.STAKING_REWARDS_DISTRIBUTED[0])) {
      return {
        kind: "StakingRewardsDistributed",
        token: toStringValue(readField(data, "token", 0) ?? topics[1]),
        totalAmount: toStringValue(readField(data, "total_amount", 1)),
        totalStaked: toStringValue(readField(data, "total_staked", 2)),
        ...metadata,
      };
    }

    if (topic0 === normalizeForMatch(EVENT_TOPICS.STAKING_REWARDS_CLAIMED[0])) {
      return {
        kind: "StakingRewardsClaimed",
        token: toStringValue(readField(data, "token", 1) ?? topics[1]),
        staker: toStringValue(readField(data, "staker", 0)),
        amount: toStringValue(readField(data, "amount", 2)),
        ...metadata,
      };
    }

    if (topic0 === normalizeForMatch(EVENT_TOPICS.VERIFICATION_VERIFIED[0]) && topic1 === normalizeForMatch(EVENT_TOPICS.VERIFICATION_VERIFIED[1])) {
      return {
        kind: "MentorVerified",
        mentor: toStringValue(topic2),
        credentialHash: toStringValue(readField(data, "credential_hash", 0)),
        verifiedAt: toNumberValue(readField(data, "verified_at", 1)),
        expiry: toNumberValue(readField(data, "expiry", 2)),
        ...metadata,
      };
    }

    if (topic0 === normalizeForMatch(EVENT_TOPICS.VERIFICATION_REVOKED[0]) && topic1 === normalizeForMatch(EVENT_TOPICS.VERIFICATION_REVOKED[1])) {
      return {
        kind: "VerificationRevoked",
        mentor: toStringValue(topic2),
        ...metadata,
      };
    }

    if (topic0 === normalizeForMatch(EVENT_TOPICS.REFERRAL_REGISTERED[0]) && topic1 === normalizeForMatch(EVENT_TOPICS.REFERRAL_REGISTERED[1])) {
      return {
        kind: "ReferralRegistered",
        referrer: toStringValue(topic2),
        referee: toStringValue(readField(data, "referee", 0)),
        isMentor: Boolean(readField(data, "is_mentor", 1)),
        ...metadata,
      };
    }

    if (topic0 === normalizeForMatch(EVENT_TOPICS.REFERRAL_REWARD_CLAIMED[0]) && topic1 === normalizeForMatch(EVENT_TOPICS.REFERRAL_REWARD_CLAIMED[1])) {
      return {
        kind: "ReferralRewardClaimed",
        referrer: toStringValue(topic2),
        amount: toStringValue(readField(data, "amount", 0)),
        ...metadata,
      };
    }

    if (topic0 === normalizeForMatch(EVENT_TOPICS.MNT_MINT[0]) && topic1 === normalizeForMatch(EVENT_TOPICS.MNT_MINT[1])) {
      return {
        kind: "Mint",
        to: toStringValue(topic2),
        amount: toStringValue(readField(data, "amount", 0)),
        ...metadata,
      };
    }

    if (topic0 === normalizeForMatch(EVENT_TOPICS.MNT_BURN[0]) && topic1 === normalizeForMatch(EVENT_TOPICS.MNT_BURN[1])) {
      return {
        kind: "Burn",
        from: toStringValue(topic2),
        amount: toStringValue(readField(data, "amount", 0)),
        ...metadata,
      };
    }

    if (topic0 === normalizeForMatch(EVENT_TOPICS.MNT_APPROVE[0]) && topic1 === normalizeForMatch(EVENT_TOPICS.MNT_APPROVE[1])) {
      return {
        kind: "Approve",
        from: toStringValue(topic2),
        spender: toStringValue(readField(data, "spender", 0)),
        amount: toStringValue(readField(data, "amount", 1)),
        ...metadata,
      };
    }

    if (topic0 === normalizeForMatch(EVENT_TOPICS.MNT_TRANSFER[0]) && topic1 === normalizeForMatch(EVENT_TOPICS.MNT_TRANSFER[1])) {
      return {
        kind: "Transfer",
        from: toStringValue(topic2),
        to: toStringValue(readField(data, "to", 0)),
        amount: toStringValue(readField(data, "amount", 1)),
        ...metadata,
      };
    }

    if (topic0 === normalizeForMatch(EVENT_TOPICS.TREASURY_BUYBACK_EXECUTED[0])) {
      return {
        kind: "BuybackExecuted",
        dexContract: toStringValue(topics[1]),
        usdcSpent: toStringValue(readField(data, "usdc_spent", 0)),
        mntBurned: toStringValue(readField(data, "mnt_burned", 1)),
        price: toStringValue(readField(data, "price", 2)),
        ...metadata,
      };
    }

    return warnUnknown(raw, topics);
  } catch (error) {
    console.warn("[contracts-sdk/events] Failed to decode event", error);
    return null;
  }
}

export { EVENT_TOPICS };
