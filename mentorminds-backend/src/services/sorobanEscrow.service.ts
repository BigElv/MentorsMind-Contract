import { SorobanEscrowService } from "./escrow-api.service";
import { StellarFeesService } from "./stellarFees.service";
import { Networks, rpc as SorobanRpc } from '@stellar/stellar-sdk';

// Max Stellar amount: 2^63 - 1 stroops = 922337203685.4775807 XLM
const MAX_STELLAR_AMOUNT = 922337203685.4775807;
const MAX_STELLAR_STROOPS = BigInt("9223372036854775807");

/**
 * Validates that a string is a valid Stellar amount:
 * - Parseable as a positive decimal number
 * - Greater than 0
 * - At most 7 decimal places
 * - Does not exceed the max Stellar amount (922337203685.4775807 XLM)
 *
 * Uses BigInt stroop arithmetic to avoid floating-point precision issues
 * near the max amount boundary.
 */
export function validateStellarAmount(amount: string): void {
  if (!/^\d+(\.\d+)?$/.test(amount)) {
    throw Object.assign(
      new Error(`Invalid amount "${amount}": must be a positive decimal number`),
      { statusCode: 400 }
    );
  }

  const [intPart, decimalPart = ""] = amount.split(".");

  if (decimalPart.length > 7) {
    throw Object.assign(
      new Error(`Invalid amount "${amount}": must have at most 7 decimal places`),
      { statusCode: 400 }
    );
  }

  const paddedDecimal = decimalPart.padEnd(7, "0");
  const stroops =
    BigInt(intPart) * BigInt(10_000_000) + BigInt(paddedDecimal);

  if (stroops <= 0n) {
    throw Object.assign(
      new Error(`Invalid amount "${amount}": must be greater than 0`),
      { statusCode: 400 }
    );
  }

  if (stroops > MAX_STELLAR_STROOPS) {
    throw Object.assign(
      new Error(
        `Invalid amount "${amount}": exceeds maximum Stellar amount of ${MAX_STELLAR_AMOUNT}`
      ),
      { statusCode: 400 }
    );
  }
}

export type BookingPaymentStatus =
  | "pending"
  | "paid"
  | "failed"
  | "disputed"
  | "refunded";

export interface EscrowOnChainState {
  escrowId: string;
  status: "active" | "released" | "disputed" | "refunded" | "resolved";
}

export interface BookingRecord {
  id: string;
  escrowId: string;
  status: string;
  paymentStatus: BookingPaymentStatus;
}

export interface BookingRepository {
  updatePaymentStatus(
    bookingId: string,
    status: BookingPaymentStatus
  ): Promise<void>;
  findBookingsWithActiveEscrow(statuses: string[]): Promise<BookingRecord[]>;
}

export interface EscrowStateResolver {
  getEscrowState(escrowId: string): Promise<EscrowOnChainState>;
}

export interface ContractTransactionResult {
  fee: string;
}

export class StellarSorobanClient {
  constructor(
    private readonly feesService: Pick<StellarFeesService, "getFeeEstimate">
  ) {}

  async buildContractTransaction(): Promise<ContractTransactionResult> {
    const feeMultiplier = parseInt(
      process.env.SOROBAN_FEE_MULTIPLIER || "10",
      10
    );
    const { recommended_fee } = await this.feesService.getFeeEstimate(1);
    const fee = String(parseInt(recommended_fee, 10) * feeMultiplier);
    return { fee };
  }

  async buildContractTransactionWithRetry(
    maxRetries = 2
  ): Promise<ContractTransactionResult> {
    let feeMultiplier = parseInt(
      process.env.SOROBAN_FEE_MULTIPLIER || "10",
      10
    );
    const { recommended_fee } = await this.feesService.getFeeEstimate(1);
    let baseFee = parseInt(recommended_fee, 10) * feeMultiplier;

    for (let attempt = 1; attempt <= maxRetries; attempt++) {
      try {
        return { fee: String(baseFee) };
      } catch (err: unknown) {
        const error = err as { result_codes?: { transaction?: string } };
        if (
          error?.result_codes?.transaction === "tx_insufficient_fee" &&
          attempt < maxRetries
        ) {
          baseFee = baseFee * 2;
        } else {
          throw err;
        }
      }
    }
    return { fee: String(baseFee) };
  }
}

/**
 * Concrete SorobanEscrowService implementation that validates the amount
 * before passing it to the Soroban contract.
 */
export class SorobanEscrowServiceImpl implements SorobanEscrowService {
  private readonly expectedContractVersion =
    process.env.SOROBAN_CONTRACT_VERSION?.trim() || null;
  private resolvedContractVersion: string | null = null;
  private configured = true;

  constructor(
    private readonly resolveVersion: (() => Promise<string | null>) | null = null
  ) {}

  async verifyContractVersion(): Promise<boolean> {
    // Check network configuration
    const network = process.env.STELLAR_NETWORK || 'testnet';
    const networkPassphrase = network === 'mainnet' ? Networks.PUBLIC : Networks.TESTNET;
    const rpcUrl = process.env.SOROBAN_RPC_URL || 'https://soroban-testnet.stellar.org';
    const rpcServer = new SorobanRpc.Server(rpcUrl, { allowHttp: rpcUrl.startsWith("http://") });
    const networkInfo = await rpcServer.getNetwork();
    if (networkInfo.passphrase !== networkPassphrase) {
      throw new Error("SOROBAN_RPC_URL network passphrase does not match STELLAR_NETWORK configuration");
    }

    if (!this.expectedContractVersion) {
      return true;
    }

    const fetchVersion =
      this.resolveVersion ??
      (async (): Promise<string | null> => {
        return null;
      });

    let detectedVersion: string | null;
    try {
      detectedVersion = await fetchVersion();
    } catch (error) {
      this.configured = false;
      throw Object.assign(
        new Error(
          `Soroban contract version check failed: ${(error as Error).message}`
        ),
        { statusCode: 503 }
      );
    }

    this.resolvedContractVersion = detectedVersion;
    if (!detectedVersion || detectedVersion !== this.expectedContractVersion) {
      this.configured = false;
      return false;
    }

    this.configured = true;
    return true;
  }

  isConfigured(): boolean {
    return this.configured;
  }

  getExpectedContractVersion(): string | null {
    return this.expectedContractVersion;
  }

  getResolvedContractVersion(): string | null {
    return this.resolvedContractVersion;
  }

  async createEscrow(input: {
    escrowId: string;
    mentorId: string;
    learnerId: string;
    amount: string;
  }): Promise<{ txHash: string; contractVersion: string | null }> {
    validateStellarAmount(input.amount);

    if (this.expectedContractVersion && !this.configured) {
      throw Object.assign(
        new Error(
          "Soroban escrow integration disabled due to contract version mismatch"
        ),
        { statusCode: 503 }
      );
    }

    // TODO: invoke the Soroban contract here
    // const result = await sorobanClient.invoke('create_escrow', { ... });
    // return { txHash: result.hash, contractVersion: this.resolvedContractVersion };

    throw new Error(
      "SorobanEscrowServiceImpl: contract invocation not yet wired up"
    );
  }

  /**
   * Applies the on-chain escrow state to a booking record.
   *
   * Disputed escrows must set payment_status = 'disputed' — never 'failed'.
   * A dispute means funds are held in escrow pending resolution, not that
   * payment failed.
   */
  async applyEscrowStateToBookings(
    state: EscrowOnChainState,
    bookingId: string,
    repo: BookingRepository
  ): Promise<void> {
    switch (state.status) {
      case "disputed":
        await repo.updatePaymentStatus(bookingId, "disputed");
        break;
      case "released":
        await repo.updatePaymentStatus(bookingId, "paid");
        break;
      case "refunded":
        await repo.updatePaymentStatus(bookingId, "refunded");
        break;
      // 'active' and 'resolved' require no payment status change
    }
  }

  /**
   * Syncs on-chain escrow state to bookings.
   *
   * Includes 'pending' bookings because escrow is created when payment is
   * confirmed, which can happen before the mentor confirms the booking.
   * Omitting 'pending' means timeout refunds on pending bookings are never
   * reflected in the DB.
   */
  async syncPendingEscrows(
    bookingRepo: BookingRepository,
    escrowStateResolver: EscrowStateResolver
  ): Promise<void> {
    const bookings = await bookingRepo.findBookingsWithActiveEscrow([
      "pending",
      "confirmed",
      "completed",
      "cancelled",
    ]);

    for (const booking of bookings) {
      const state = await escrowStateResolver.getEscrowState(booking.escrowId);
      await this.applyEscrowStateToBookings(state, booking.id, bookingRepo);
    }
  }
}
