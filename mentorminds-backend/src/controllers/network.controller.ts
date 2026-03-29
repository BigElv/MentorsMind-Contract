import { Request, Response } from 'express';
import { networkMonitorService } from '../services/network-monitor.service';

/**
 * Controller to expose current network status and health metrics.
 */
export const getNetworkStatus = (req: Request, res: Response) => {
  const status = networkMonitorService.getStatus();
  res.status(200).json({
    success: true,
    data: status,
  });
};
