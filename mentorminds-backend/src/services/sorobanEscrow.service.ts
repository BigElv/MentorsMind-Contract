import { SorobanEscrowService } from "./escrow-api.service";

// Max Stellar amount: 2^63 - 1 stroops = 922337203685.4775807 XLM
const MAX_STELLAR_AMOUNT = 922337203685.4775807;

/**
 * Validates that a string is a valid Stellar amount:
 * - Parseable as a positive decimal number
 * - Greater than 0
 * - At most 7 decimal places
 * - Does not exceed the max Stellar amount (922337203685.4775807 XLM)
 *
 * Throws a 400-style error with a descriptive message if invalid.
 */
export function validateStellarAmount(amount: string): void {
  if (!/^\d+(\.\d+)?$/.test(amount)) {
    throw Object.assign(
      new Error(`Invalid amount "${amount}": must be a positive decimal number`),
      { statusCode: 400 }
    );
  }

  const value = parseFloat(amount);

  if (value <= 0) {
    throw Object.assign(
      new Error(`Invalid amount "${amount}": must be greater than 0`),
      { statusCode: 400 }
    );
  }

  const decimalPart = amount.split(".")[1];
  if (decimalPart && decimalPart.length > 7) {
    throw Object.assign(
      new Error(`Invalid amount "${amount}": must have at most 7 decimal places`),
      { statusCode: 400 }
    );
  }

  if (value > MAX_STELLAR_AMOUNT) {
    throw Object.assign(
      new Error(
        `Invalid amount "${amount}": exceeds maximum Stellar amount of ${MAX_STELLAR_AMOUNT}`
      ),
      { statusCode: 400 }
    );
  }
}

/**
 * Concrete SorobanEscrowService implementation that validates the amount
 * before passing it to the Soroban contract.
 *
 * Extend this class (or inject a contract client) to wire up the actual
 * Soroban RPC call.
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

    throw new Error("SorobanEscrowServiceImpl: contract invocation not yet wired up");
  }
}
