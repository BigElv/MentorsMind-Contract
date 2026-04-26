import { 
  Server, 
  Keypair, 
  TransactionBuilder, 
  Networks, 
  Operation, 
  Asset 
} from 'stellar-sdk';
import { Pool } from 'pg';
import { stellarFeesService } from './stellarFees.service';
import { horizonConfig } from '../config/horizon.config';

const MAX_FEE_STROOPS = '10000';

/**
 * Stellar reserve model:
 *   base reserve = 1 XLM per account
 *   0.5 XLM per trustline (each non-native asset requires one)
 *   0.5 XLM safety buffer
 *
 * For 2 non-native assets (USDC, PYUSD): 1 + (0.5 * 2) + 0.5 = 2.5 XLM
 *
 * Override via STELLAR_STARTING_BALANCE env var.
 */
const NUM_TRUSTLINES = 2; // USDC + PYUSD
const MINIMUM_STARTING_BALANCE = (1 + 0.5 * NUM_TRUSTLINES + 0.5).toFixed(1); // "2.5"
const STARTING_BALANCE = process.env.STELLAR_STARTING_BALANCE ?? MINIMUM_STARTING_BALANCE;

/**
 * Wait for a Stellar account to become visible on Horizon after funding.
 *
 * Uses exponential backoff (1s → 2s → 4s … capped at 30s) for up to
 * `maxAttempts` tries (~total budget ≈ 30 s with defaults).
 *
 * Distinguishes three cases:
 *  - 404: account not yet visible — retry
 *  - account found: resolve immediately
 *  - any other error: rethrow (network failure, auth error, etc.)
 */
async function waitForAccount(
  server: Server,
  publicKey: string,
  maxAttempts = 10,
  baseDelayMs = 1000,
  maxDelayMs = 30_000
): Promise<void> {
  let lastError: unknown;
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    try {
      await server.loadAccount(publicKey);
      return; // account is live
    } catch (err: any) {
      if (err?.response?.status === 404) {
        // Account not yet included in a ledger — wait and retry
        lastError = err;
        if (attempt < maxAttempts) {
          const delay = Math.min(baseDelayMs * 2 ** (attempt - 1), maxDelayMs);
          await new Promise((r) => setTimeout(r, delay));
        }
      } else {
        // Non-404 (network failure, bad request, etc.) — don't retry
        throw err;
      }
    }
  }
  throw new Error(
    `Account ${publicKey} not visible on Horizon after ${maxAttempts} attempts. ` +
    `The funding transaction may still be pending. Original error: ${(lastError as any)?.message}`
  );
}

export class StellarAccountService {
  private server: Server;
  private adminKeypair: Keypair;

  constructor(private readonly pool: Pool) {
    this.server = new Server(horizonConfig.primary);
    
    // In a production environment, this secret should be securely managed 
    // (e.g., using AWS Secrets Manager, HashiCorp Vault, or encrypted env vars).
    const adminSecret = process.env.STELLAR_ADMIN_SECRET || 'SDAV9...'; // Placeholder
    try {
      this.adminKeypair = Keypair.fromSecret(adminSecret);
    } catch (e) {
      // Fallback for development/testing if secret is missing or invalid
      this.adminKeypair = Keypair.random();
    }
  }

  /**
   * Checks whether a Stellar account already exists on the network.
   */
  async accountExists(publicKey: string): Promise<boolean> {
    try {
      await this.server.loadAccount(publicKey);
      return true;
    } catch (e: any) {
      if (e?.response?.status === 404) return false;
      throw e;
    }
  }

  /**
   * Funds a new or existing Stellar account with a starting balance.
   * Uses dynamic fee estimation to ensure transactions succeed during surge pricing.
   * Idempotent: safe to call multiple times for the same destination/userId.
   *
   * @param destination The public key of the account to fund.
   * @param userId The ID of the user owning the wallet.
   */
  async fundAccount(destination: string, userId: string) {
    // 1. Idempotency check: if already activated in DB, return early
    const existing = await this.pool.query(
      'SELECT transaction_hash FROM transactions WHERE user_id = $1 AND destination = $2 AND status = $3 LIMIT 1',
      [userId, destination, 'completed']
    );
    if (existing.rowCount && existing.rowCount > 0) {
      return existing.rows[0].transaction_hash as string;
    }

    // 2. Check if the account already exists on-chain; skip funding if so
    const alreadyFunded = await this.accountExists(destination);

    if (alreadyFunded) {
      // Account exists on-chain but DB has no completed record — reconcile
      await this.pool.query(
        'INSERT INTO transactions (user_id, amount, destination, status, transaction_hash, created_at) VALUES ($1, $2, $3, $4, $5, NOW()) ON CONFLICT DO NOTHING',
        [userId, STARTING_BALANCE, destination, 'completed', 'pre-existing']
      );
      return 'pre-existing';
    }

    try {
      // 3. Fetch recommended fee estimate dynamically
      const { recommended_fee } = await stellarFeesService.getFeeEstimate(1);

      // 4. Apply a safety cap to the fee to prevent runaway costs
      const finalFee = Math.min(
        parseInt(recommended_fee, 10),
        parseInt(MAX_FEE_STROOPS, 10)
      ).toString();

      // 5. Load the source account to get the current sequence number
      const sourceAccount = await this.server.loadAccount(this.adminKeypair.publicKey());

      // 6. Build the transaction with the dynamic fee
      const transaction = new TransactionBuilder(sourceAccount, {
        fee: finalFee,
        networkPassphrase: Networks.TESTNET,
      })
        .addOperation(
          Operation.payment({
            destination,
            asset: Asset.native(),
            amount: STARTING_BALANCE,
          })
        )
        .setTimeout(30)
        .build();

      // 7. Sign and submit
      transaction.sign(this.adminKeypair);
      const submissionResult = await this.server.submitTransaction(transaction);

      // 8. Record success in the database
      await this.pool.query(
        'INSERT INTO transactions (user_id, amount, destination, status, transaction_hash, created_at) VALUES ($1, $2, $3, $4, $5, NOW())',
        [userId, STARTING_BALANCE, destination, 'completed', submissionResult.hash]
      );

      return submissionResult.hash;
    } catch (error: any) {
      // 9. Treat op_already_exists as success — account was funded by a prior attempt
      const extras = error?.response?.data?.extras;
      const resultCodes: string[] = extras?.result_codes?.operations ?? [];
      if (resultCodes.includes('op_already_exists')) {
        await this.pool.query(
          'INSERT INTO transactions (user_id, amount, destination, status, transaction_hash, created_at) VALUES ($1, $2, $3, $4, $5, NOW()) ON CONFLICT DO NOTHING',
          [userId, STARTING_BALANCE, destination, 'completed', 'op_already_exists']
        );
        return 'op_already_exists';
      }

      console.error('[StellarAccountService] Funding failed:', error);

      // Record failure in database for audit/retry purposes
      await this.pool.query(
        'INSERT INTO transactions (user_id, amount, destination, status, created_at) VALUES ($1, $2, $3, $4, NOW())',
        [userId, STARTING_BALANCE, destination, 'failed']
      );

      throw error;
    }
  }

  /**
   * Creates a new Keypair and funds it.
   * @param userId The ID of the user.
   */
  async createAndFundWallet(userId: string) {
    const keypair = Keypair.random();
    const destination = keypair.publicKey();
    
    await this.fundAccount(destination, userId);
    
    return destination;
  }

  /**
   * Activates an existing wallet by funding it.
   * @param destination The public key.
   * @param userId The ID of the user.
   */
  async activateExistingWallet(destination: string, userId: string) {
    await this.fundAccount(destination, userId);
  }

  /**
   * Verifies that an account has sufficient XLM reserve to hold all required
   * trustlines. Stellar requires 1 XLM base + 0.5 XLM per trustline entry.
   *
   * @param publicKey The account public key to check.
   * @returns An object indicating whether the reserve is sufficient and the
   *          current balance, required balance, and missing amount (if any).
   */
  async verifyActivation(publicKey: string): Promise<{
    sufficient: boolean;
    currentBalance: number;
    requiredBalance: number;
    missingXlm: number;
  }> {
    const account = await this.server.loadAccount(publicKey);

    const xlmBalance = account.balances.find(
      (b: any) => b.asset_type === 'native'
    );
    const currentBalance = xlmBalance ? parseFloat(xlmBalance.balance) : 0;

    // Count existing trustlines (all non-native balance entries)
    const existingTrustlines = account.balances.filter(
      (b: any) => b.asset_type !== 'native'
    ).length;

    // Required reserve: 1 XLM base + 0.5 per trustline + 0.5 buffer
    const requiredBalance = 1 + 0.5 * Math.max(existingTrustlines, NUM_TRUSTLINES) + 0.5;
    const missingXlm = Math.max(0, requiredBalance - currentBalance);

    if (missingXlm > 0) {
      console.warn(
        `[StellarAccountService] Account ${publicKey} has insufficient reserve. ` +
        `Current: ${currentBalance} XLM, Required: ${requiredBalance} XLM, ` +
        `Missing: ${missingXlm} XLM`
      );
    }

    return {
      sufficient: missingXlm === 0,
      currentBalance,
      requiredBalance,
      missingXlm,
    };
  }
}
