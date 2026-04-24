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
  async createEscrow(input: {
    escrowId: string;
    mentorId: string;
    learnerId: string;
    amount: string;
  }): Promise<{ txHash: string }> {
    validateStellarAmount(input.amount);

    // TODO: invoke the Soroban contract here
    // const result = await sorobanClient.invoke('create_escrow', { ... });
    // return { txHash: result.hash };

    throw new Error("SorobanEscrowServiceImpl: contract invocation not yet wired up");
  }
}
