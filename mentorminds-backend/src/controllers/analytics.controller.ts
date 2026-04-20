import { Request, Response } from 'express';
import { blockchainAnalyticsService } from '../services/blockchain-analytics.service';

export const getStats = async (_req: Request, res: Response) => {
  try {
    const stats = await blockchainAnalyticsService.getDashboardStats();
    res.json(stats);
  } catch (error: any) {
    res.status(500).json({ error: error.message });
  }
};

export const downloadReport = async (_req: Request, res: Response) => {
  try {
    const csv = await blockchainAnalyticsService.generateCsvReport();
    res.setHeader('Content-Type', 'text/csv');
    res.setHeader('Content-Disposition', 'attachment; filename=blockchain-analytics.csv');
    res.send(csv);
  } catch (error: any) {
    res.status(500).json({ error: error.message });
  }
};
