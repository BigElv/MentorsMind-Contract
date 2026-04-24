import { Server } from 'stellar-sdk';
import { horizonConfig } from '../config/horizon.config';

export class StellarFeesService {
  private server: Server;

  constructor(rpcUrl: string = horizonConfig.primary) {
    this.server = new Server(rpcUrl);
  }

  /**
   * Fetches the recommended fee estimate from Horizon.
   * @param operationCount Number of operations in the transaction.
   * @returns An object containing the recommended_fee as a string.
   */
  async getFeeEstimate(operationCount: number = 1): Promise<{ recommended_fee: string }> {
    try {
      // Fetch fee stats from Horizon
      const feeStats = await this.server.feeStats();
      
      // We use the 'mode' fee charged in recent ledgers as a reliable "recommended" fee.
      const baseFee = parseInt(feeStats.fee_charged.mode, 10) || 100;
      
      const totalFee = baseFee * Math.max(operationCount, 1);
      
      return {
        recommended_fee: totalFee.toString(),
      };
    } catch (error) {
      console.warn('[StellarFeesService] Failed to fetch fee stats from Horizon, using fallback:', error);
      return {
        recommended_fee: (100 * Math.max(operationCount, 1)).toString(),
      };
    }
  }
}

export const stellarFeesService = new StellarFeesService();
