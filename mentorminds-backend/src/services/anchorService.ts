import { Keypair } from 'stellar-sdk';
import { SEP10AuthService } from './sep10';
import { SEP24Service, SEP24Transaction } from './sep24';

// Types for Anchor configuration
export interface AnchorConfig {
  homeDomain: string;
  authEndpoint: string;
  transferServerUrl: string;
  assetCode: string; // e.g., 'USDC'
}

// Map to simulate database or cache for anchor configurations
const ANCHOR_CONFIGS: Record<string, AnchorConfig> = {
  'circle': {
    homeDomain: 'circle.com',
    authEndpoint: 'https://api.circle.com/v1/auth',
    transferServerUrl: 'https://api.circle.com/v1/sep24',
    assetCode: 'USDC'
  },
  'moneygram': {
    homeDomain: 'moneygram.com',
    authEndpoint: 'https://api.moneygram.com/auth',
    transferServerUrl: 'https://api.moneygram.com/sep24',
    assetCode: 'USDC'
  }
};

export class AnchorService {
  /**
   * Orchestrates the interactive deposit flow for a user.
   */
  async startDeposit(
    anchorId: 'circle' | 'moneygram',
    userKeypair: Keypair,
    amount?: string
  ): Promise<{ interactiveUrl: string; transactionId: string }> {
    const config = ANCHOR_CONFIGS[anchorId];
    if (!config) throw new Error('Invalid anchor ID');

    // 1. Authenticate with the anchor
    const token = await SEP10AuthService.authenticate(config.authEndpoint, userKeypair);

    // 2. Initiate the interactive deposit
    const { url, id } = await SEP24Service.initiateDeposit(
      config.transferServerUrl,
      token,
      config.assetCode,
      userKeypair.publicKey(),
      amount
    );

    // 3. Start background polling (In a real app, this might be handled by a worker/queue)
    this.pollTransactionStatus(anchorId, token, id, userKeypair.publicKey());

    return {
      interactiveUrl: url,
      transactionId: id
    };
  }

  /**
   * Orchestrates the interactive withdrawal flow for a user.
   */
  async startWithdrawal(
    anchorId: 'circle' | 'moneygram',
    userKeypair: Keypair,
    amount?: string
  ): Promise<{ interactiveUrl: string; transactionId: string }> {
    const config = ANCHOR_CONFIGS[anchorId];
    if (!config) throw new Error('Invalid anchor ID');

    // 1. Authenticate
    const token = await SEP10AuthService.authenticate(config.authEndpoint, userKeypair);

    // 2. Initiate withdrawal
    const { url, id } = await SEP24Service.initiateWithdrawal(
      config.transferServerUrl,
      token,
      config.assetCode,
      userKeypair.publicKey(),
      amount
    );

    return {
      interactiveUrl: url,
      transactionId: id
    };
  }

  /**
   * Polls the transaction status until it is completed or failed.
   */
  private async pollTransactionStatus(
    anchorId: string,
    authToken: string,
    transactionId: string,
    userPublicKey: string
  ): Promise<void> {
    const config = ANCHOR_CONFIGS[anchorId];
    let attempts = 0;
    const maxAttempts = 100; // Example limit

    const interval = setInterval(async () => {
      try {
        const tx = await SEP24Service.getTransactionStatus(
          config.transferServerUrl,
          authToken,
          transactionId
        );

        if (tx.status === 'completed') {
          clearInterval(interval);
          await this.updateUserBalance(userPublicKey, tx);
        } else if (['error', 'expired', 'no_market', 'too_small', 'too_large'].includes(tx.status)) {
          clearInterval(interval);
          console.error(`Transaction ${transactionId} failed with status: ${tx.status}`);
        }

        if (++attempts >= maxAttempts) {
          clearInterval(interval);
        }
      } catch (error) {
        console.error(`Error polling transaction ${transactionId}:`, error);
      }
    }, 30000); // Poll every 30 seconds
  }

  /**
   * Credits the user's platform balance on completion.
   */
  private async updateUserBalance(userPublicKey: string, transaction: SEP24Transaction): Promise<void> {
    console.log(`Updating balance for ${userPublicKey}: Received ${transaction.amount_out} USDC`);
    // Database logic here: 
    // db.users.update({ publicKey: userPublicKey }, { $inc: { balance: transaction.amount_out } })
  }
}
