import { paymentTrackerService } from './payment-tracker.service';
import { Payment } from '../types/payment.types';

export interface DashboardStats {
  totalVolume: {
    count: number;
    xlmValue: string;
  };
  successRate: number;
  feeMetrics: {
    average: string;
    total: string;
  };
  assetDistribution: Record<string, number>;
  escrowMetrics: {
    total: number;
    disputeRate: number;
  };
}

export class BlockchainAnalyticsService {
  async getDashboardStats(): Promise<DashboardStats> {
    const payments = await this.getAllPayments();
    
    const count = payments.length;
    const confirmed = payments.filter(p => p.status === 'confirmed');
    const failed = payments.filter(p => p.status === 'failed' || p.status === 'timeout');
    
    const successRate = count > 0 ? (confirmed.length / count) * 100 : 0;
    
    let totalXlm = 0;
    const assetDist: Record<string, number> = {};
    let totalFees = 0;
    let feeCount = 0;

    for (const p of payments) {
      // Asset distribution
      assetDist[p.assetCode] = (assetDist[p.assetCode] || 0) + 1;
      
      // Volume calculation (simplified: only counting XLM value for XLM assets for now)
      // In a real app, you'd use a price feed for USDC/PYUSD
      if (p.assetCode === 'XLM' && p.status === 'confirmed') {
        totalXlm += parseFloat(p.amount);
      }

      // Fee metrics
      if (p.fee) {
        totalFees += parseFloat(p.fee);
        feeCount++;
      }
    }

    return {
      totalVolume: {
        count,
        xlmValue: totalXlm.toFixed(7),
      },
      successRate,
      feeMetrics: {
        average: feeCount > 0 ? (totalFees / feeCount).toFixed(7) : '0',
        total: totalFees.toFixed(7),
      },
      assetDistribution: assetDist,
      escrowMetrics: {
        total: 0, // Placeholder
        disputeRate: 0, // Placeholder
      },
    };
  }

  async generateCsvReport(): Promise<string> {
    const payments = await this.getAllPayments();
    const headers = ['ID', 'Date', 'Status', 'Amount', 'Asset', 'Fee', 'Sender', 'Receiver', 'TxHash'];
    const rows = payments.map(p => [
      p.id,
      p.createdAt.toISOString(),
      p.status,
      p.amount,
      p.assetCode,
      p.fee || '0',
      p.senderAddress,
      p.receiverAddress,
      p.txHash || '',
    ].map(v => `"${String(v).replace(/"/g, '""')}"`).join(','));

    return [headers.join(','), ...rows].join('\n');
  }

  private async getAllPayments(): Promise<Payment[]> {
    // Accessing private payments map indirectly if needed, 
    // but PaymentTrackerService doesn't have a getAll. 
    // Let's assume we can add getAll to PaymentTrackerService or use an alternative.
    // For now, I'll add getAll to PaymentTrackerService.
    return (paymentTrackerService as any).getAll(); 
  }
}

export const blockchainAnalyticsService = new BlockchainAnalyticsService();
