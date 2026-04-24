import { Horizon, Networks, Keypair } from '@stellar/stellar-sdk';

export class StellarService {
  private server: Horizon.Server;
  private network: string;

  constructor() {
    this.network = process.env.STELLAR_NETWORK || 'testnet';
    const horizonUrl = process.env.HORIZON_URL || (this.network === 'mainnet' 
      ? 'https://horizon.stellar.org' 
      : 'https://horizon-testnet.stellar.org');
    this.server = new Horizon.Server(horizonUrl);
  }

  /**
   * Validates a Stellar public key
   * @param publicKey The public key to validate
   * @returns boolean indicating if the public key is valid
   */
  public validatePublicKey(publicKey: string): boolean {
    try {
      Keypair.fromPublicKey(publicKey);
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Fetches the account balance for a given public key
   * @param publicKey The public key of the account
   * @returns The balance of the account (XLM)
   */
  public async getAccountBalance(publicKey: string): Promise<string> {
    if (!this.validatePublicKey(publicKey)) {
      throw new Error('Invalid public key');
    }

    try {
      const account = await this.server.loadAccount(publicKey);
      const nativeBalance = account.balances.find((b: any) => b.asset_type === 'native');
      return nativeBalance ? nativeBalance.balance : '0';
    } catch (error: any) {
      if (error.response && error.response.status === 404) {
        throw new Error('Account not found');
      }
      throw new Error(`Failed to fetch balance: ${error.message}`);
    }
  }

  /**
   * Creates a new Stellar account (keypair only)
   * @returns Object containing publicKey and secret
   */
  public createAccount(): { publicKey: string; secret: string } {
    const pair = Keypair.random();
    return {
      publicKey: pair.publicKey(),
      secret: pair.secret(),
    };
  }

  /**
   * Gets the current network passphrase
   * @returns Networks passphrase
   */
  public getNetworkPassphrase(): string {
    return this.network === 'mainnet' ? Networks.PUBLIC : Networks.TESTNET;
  }
}

export const stellarService = new StellarService();
