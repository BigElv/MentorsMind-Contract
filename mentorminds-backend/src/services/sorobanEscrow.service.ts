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

// ---------------------------------------------------------------------------
// Circuit Breaker
// ---------------------------------------------------------------------------

type CircuitState = "closed" | "open" | "half-open";

interface CircuitBreakerOptions {
  /** Consecutive failures before opening the circuit. Default: 5 */
  failureThreshold: number;
  /** Milliseconds to wait in open state before probing. Default: 30_000 */
  recoveryTimeoutMs: number;
}

/**
 * Lightweight counter-based circuit breaker.
 *
 * States:
 *   closed    — calls pass through normally
 *   open      — calls fast-fail immediately (RPC node is known unhealthy)
 *   half-open — one probe call is allowed; success → closed, failure → open
 */
export class SorobanCircuitBreaker {
  private state: CircuitState = "closed";
  private consecutiveFailures = 0;
  private openedAt: number | null = null;

  constructor(private readonly options: CircuitBreakerOptions = {
    failureThreshold: 5,
    recoveryTimeoutMs: 30_000,
  }) {}

  get isOpen(): boolean {
    return this.state === "open";
  }

  /**
   * Wraps an async call with circuit-breaker logic.
   * Throws immediately when the circuit is open (and not yet ready to probe).
   */
  async call<T>(fn: () => Promise<T>): Promise<T> {
    if (this.state === "open") {
      const elapsed = Date.now() - (this.openedAt ?? 0);
      if (elapsed < this.options.recoveryTimeoutMs) {
        throw Object.assign(
          new Error("Soroban RPC circuit breaker is OPEN — fast-failing request"),
          { statusCode: 503, circuitOpen: true }
        );
      }
      // Transition to half-open to allow one probe
      this.state = "half-open";
      console.warn("[SorobanCircuitBreaker] Transitioning to HALF-OPEN — probing RPC node");
    }

    try {
      const result = await fn();
      this.onSuccess();
      return result;
    } catch (err) {
      this.onFailure();
      throw err;
    }
  }

  private onSuccess(): void {
    if (this.state !== "closed") {
      console.info("[SorobanCircuitBreaker] RPC node recovered — circuit CLOSED");
    }
    this.state = "closed";
    this.consecutiveFailures = 0;
    this.openedAt = null;
  }

  private onFailure(): void {
    this.consecutiveFailures++;
    if (
      this.state !== "open" &&
      this.consecutiveFailures >= this.options.failureThreshold
    ) {
      this.state = "open";
      this.openedAt = Date.now();
      // Emit metric/alert — replace with your observability hook (e.g. Datadog, Prometheus)
      console.error(
        `[SorobanCircuitBreaker] Circuit OPENED after ${this.consecutiveFailures} consecutive failures. ` +
        `Fast-failing all Soroban calls for ${this.options.recoveryTimeoutMs / 1000}s.`
      );
    }
  }
}

/** Shared circuit breaker instance for all Soroban RPC calls. */
export const sorobanCircuitBreaker = new SorobanCircuitBreaker({
  failureThreshold: parseInt(process.env.SOROBAN_CB_FAILURE_THRESHOLD ?? "5", 10),
  recoveryTimeoutMs: parseInt(process.env.SOROBAN_CB_RECOVERY_MS ?? "30000", 10),
});

// ---------------------------------------------------------------------------
// SorobanEscrowServiceImpl
// ---------------------------------------------------------------------------

/**
 * Concrete SorobanEscrowService implementation that validates the amount
 * before passing it to the Soroban contract, and wraps every RPC call with
 * the shared circuit breaker so a failing node fast-fails instead of
 * blocking the async queue.
 *
 * isConfigured() returns false when the circuit is open so the system can
 * degrade gracefully to off-chain-only mode.
 */
export class SorobanEscrowServiceImpl implements SorobanEscrowService {
  /**
   * Returns false when the Soroban RPC circuit is open, signalling callers
   * to fall back to off-chain-only mode.
   */
  isConfigured(): boolean {
    return !sorobanCircuitBreaker.isOpen;
  }

  async createEscrow(input: {
    escrowId: string;
    mentorId: string;
    learnerId: string;
    amount: string;
  }): Promise<{ txHash: string }> {
    validateStellarAmount(input.amount);

    return sorobanCircuitBreaker.call(async () => {
      // TODO: invoke the Soroban contract here
      // const result = await sorobanClient.invoke('create_escrow', { ... });
      // return { txHash: result.hash };

      throw new Error("SorobanEscrowServiceImpl: contract invocation not yet wired up");
    });
  }
}
