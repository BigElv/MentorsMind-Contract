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
const STARTING_BALANCE = '10.0';

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
   * Funds a new or existing Stellar account with a starting balance.
   * Uses dynamic fee estimation to ensure transactions succeed during surge pricing.
   * 
   * @param destination The public key of the account to fund.
   * @param userId The ID of the user owning the wallet.
   */
  async fundAccount(destination: string, userId: string) {
    try {
      // 1. Fetch recommended fee estimate dynamically
      const { recommended_fee } = await stellarFeesService.getFeeEstimate(1);
      
      // 2. Apply a safety cap to the fee to prevent runaway costs
      const finalFee = Math.min(
        parseInt(recommended_fee, 10), 
        parseInt(MAX_FEE_STROOPS, 10)
      ).toString();

      // 3. Load the source account to get the current sequence number
      const sourceAccount = await this.server.loadAccount(this.adminKeypair.publicKey());
      
      // 4. Build the transaction with the dynamic fee
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

      // 5. Sign the transaction
      transaction.sign(this.adminKeypair);

      // 6. Submit to the network
      const submissionResult = await this.server.submitTransaction(transaction);

      // 7. Record the transaction in the database
      await this.pool.query(
        'INSERT INTO transactions (user_id, amount, destination, status, transaction_hash, created_at) VALUES ($1, $2, $3, $4, $5, NOW())',
        [userId, STARTING_BALANCE, destination, 'completed', submissionResult.hash]
      );

      return submissionResult.hash;
    } catch (error) {
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
}
