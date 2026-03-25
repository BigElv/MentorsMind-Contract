// @ts-ignore workspace TypeScript service may not resolve nested devDependencies.
import { describe, expect, it, vi } from "vitest";
import { decodeEvent } from "./events";

const baseEvent = {
  contract_id: "CDUMMYCONTRACTID",
  ledger_sequence: 12345,
  created_at: "2026-03-25T00:00:00Z",
  transaction_hash: "abc123",
};

describe("decodeEvent", () => {
  it("decodes EscrowCreated", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["Escrow", "Created", 1],
      data: {
        mentor: "GMENTOR",
        learner: "GLEARNER",
        amount: "10000000",
        session_id: "S1",
        token_address: "CTOKEN",
        session_end_time: 1710000000,
      },
    });

    expect(decoded?.kind).toBe("EscrowCreated");
  });

  it("decodes EscrowReleased", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["Escrow", "released", 2],
      data: {
        mentor: "GMENTOR",
        amount: "1000",
        net_amount: "950",
        platform_fee: "50",
        token_address: "CTOKEN",
      },
    });

    expect(decoded?.kind).toBe("EscrowReleased");
  });

  it("decodes EscrowPartialReleased", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["Escrow", "rel_part", 2],
      data: ["GMENTOR", "100", "95", "5", "CTOKEN", "900"],
    });

    expect(decoded?.kind).toBe("EscrowPartialReleased");
  });

  it("decodes EscrowAdminRelease", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["Escrow", "adm_rel", 2],
      data: [2, 1710000100],
    });

    expect(decoded?.kind).toBe("EscrowAdminRelease");
  });

  it("decodes EscrowAutoReleased", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["Escrow", "AutoReleased", 2],
      data: { time: 1710000200 },
    });

    expect(decoded?.kind).toBe("EscrowAutoReleased");
  });

  it("decodes DisputeOpened", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["Escrow", "DisputeOpened", 3],
      data: { caller: "GCALLER", reason: "NO_SHOW", token_address: "CTOKEN" },
    });

    expect(decoded?.kind).toBe("DisputeOpened");
  });

  it("decodes DisputeResolved", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["Escrow", "DisputeResolved", 3],
      data: {
        mentor_pct: 60,
        mentor_amount: "600",
        learner_amount: "400",
        token_address: "CTOKEN",
        time: 1710000300,
      },
    });

    expect(decoded?.kind).toBe("DisputeResolved");
  });

  it("decodes EscrowRefunded", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["Escrow", "Refunded", 4],
      data: { learner: "GLEARNER", amount: "500", token_address: "CTOKEN" },
    });

    expect(decoded?.kind).toBe("EscrowRefunded");
  });

  it("decodes ReviewSubmitted", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["Escrow", "ReviewSubmitted", 4],
      data: { caller: "GLEARNER", reason: "GREAT", mentor: "GMENTOR" },
    });

    expect(decoded?.kind).toBe("ReviewSubmitted");
  });

  it("decodes StakingRewardsDistributed", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["reward", "CTOKEN"],
      data: { token: "CTOKEN", total_amount: "1000", total_staked: "50000" },
    });

    expect(decoded?.kind).toBe("StakingRewardsDistributed");
  });

  it("decodes StakingRewardsClaimed", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["claimed", "CTOKEN"],
      data: { staker: "GSTAKER", token: "CTOKEN", amount: "100" },
    });

    expect(decoded?.kind).toBe("StakingRewardsClaimed");
  });

  it("decodes MentorVerified", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["Verification", "Verified", "GMENTOR"],
      data: {
        credential_hash: "00".repeat(32),
        verified_at: 1710000000,
        expiry: 1719999999,
      },
    });

    expect(decoded?.kind).toBe("MentorVerified");
  });

  it("decodes VerificationRevoked", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["Verification", "Revoked", "GMENTOR"],
      data: {},
    });

    expect(decoded?.kind).toBe("VerificationRevoked");
  });

  it("decodes ReferralRegistered", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["Referral", "Registered", "GREFERRER"],
      data: { referee: "GREFEREE", is_mentor: true },
    });

    expect(decoded?.kind).toBe("ReferralRegistered");
  });

  it("decodes ReferralRewardClaimed", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["Referral", "RewardClaimed", "GREFERRER"],
      data: { amount: "200000000" },
    });

    expect(decoded?.kind).toBe("ReferralRewardClaimed");
  });

  it("decodes Mint", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["MNTToken", "Mint", "GTO"],
      data: { amount: "1000" },
    });

    expect(decoded?.kind).toBe("Mint");
  });

  it("decodes Burn", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["MNTToken", "Burn", "GFROM"],
      data: { amount: "1000" },
    });

    expect(decoded?.kind).toBe("Burn");
  });

  it("decodes Approve", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["MNTToken", "Approve", "GFROM"],
      data: { spender: "GSPENDER", amount: "1000" },
    });

    expect(decoded?.kind).toBe("Approve");
  });

  it("decodes Transfer", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["MNTToken", "Transfer", "GFROM"],
      data: { to: "GTO", amount: "1000" },
    });

    expect(decoded?.kind).toBe("Transfer");
  });

  it("decodes BuybackExecuted", () => {
    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["buyback", "CDEX"],
      data: { usdc_spent: "200", mnt_burned: "2", price: "100" },
    });

    expect(decoded?.kind).toBe("BuybackExecuted");
  });

  it("returns null and logs warning for unknown events", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});

    const decoded = decodeEvent({
      ...baseEvent,
      topics: ["Unknown", "Whatever"],
      data: {},
    });

    expect(decoded).toBeNull();
    expect(warn).toHaveBeenCalled();

    warn.mockRestore();
  });
});
