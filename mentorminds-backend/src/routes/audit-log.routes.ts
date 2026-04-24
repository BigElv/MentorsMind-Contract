import { Router, Request, Response } from 'express';
import { auditLogService } from '../services/audit-log.service';

const router = Router();

router.post('/', (req: Request, res: Response) => {
  const { txHash, walletAddress, sequenceNumber } = req.body;
  if (!txHash || !walletAddress || !sequenceNumber) {
    res.status(400).json({ error: 'Missing required fields' });
    return;
  }
  const entry = auditLogService.create({ txHash, walletAddress, sequenceNumber });
  res.status(201).json(entry);
});

router.get('/', (_req: Request, res: Response) => {
  res.json(auditLogService.getAll());
});

router.get('/wallet/:address', (req: Request, res: Response) => {
  res.json(auditLogService.findByWallet(req.params.address));
});

export default router;
