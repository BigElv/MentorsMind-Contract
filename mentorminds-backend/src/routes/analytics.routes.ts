import { Router } from 'express';
import { getStats, downloadReport } from '../controllers/analytics.controller';

const router = Router();

router.get('/stats', getStats);
router.get('/report', downloadReport);

export default router;
