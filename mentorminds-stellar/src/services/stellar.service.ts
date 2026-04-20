import { Horizon, Networks, Keypair } from '@stellar/stellar-sdk';
import { stellarConfig } from '../config/stellar.config';
import { StellarAccount, StellarNetwork } from '../types/stellar.types';
import { isValidPublicKey } from '../utils/stellar.utils';

export class StellarService {
  private server: Horizon.Server;
  private network: StellarNetwork;

  constructor(network: StellarNetwork = (stellarConfig.network as StellarNetwork)) {
    this.network = network;
    const horizonUrl = network === 'mainnet' 
      ? 'https://horizon.stellar.org' 
      : 'https://horizon-testnet.stellar.org';
    this.server = new Horizon.Server(horizonUrl);
  }

  /**
   * Updates the Horizon server connection based on the network
   * @param network The network to switch to
   */
  public switchNetwork(network: StellarNetwork): void {
    this.network = network;
    const horizonUrl = network === 'mainnet' 
      ? 'https://horizon.stellar.org' 
      : 'https://horizon-testnet.stellar.org';
    this.server = new Horizon.Server(horizonUrl);
  }

  /**
   * Validates a Stellar public key
   * @param publicKey The public key to validate
   * @returns boolean indicating if the public key is valid
   */
  public validatePublicKey(publicKey: string): boolean {
    return isValidPublicKey(publicKey);
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
   * Retrieves full account details
   * @param publicKey The public key of the account
   * @returns StellarAccount object
   */
  public async getAccountDetails(publicKey: string): Promise<StellarAccount> {
    if (!this.validatePublicKey(publicKey)) {
      throw new Error('Invalid public key');
    }

    try {
      const account = await this.server.loadAccount(publicKey);
      const nativeBalance = account.balances.find((b: any) => b.asset_type === 'native');
      
      return {
        publicKey,
        balance: nativeBalance ? nativeBalance.balance : '0',
        assetCode: 'XLM',
      };
    } catch (error: any) {
      if (error.response && error.response.status === 404) {
        throw new Error('Account not found');
      }
      throw new Error(`Failed to fetch account details: ${error.message}`);
    }
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
