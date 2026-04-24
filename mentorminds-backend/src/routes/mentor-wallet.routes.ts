import { Router } from 'express';
import { createPayout, getEarningsSummary, listPayouts, streamWalletActivity } from '../controllers/mentor-wallet.controller';

const router = Router();

router.get('/:address/summary', getEarningsSummary);
router.get('/:address/payout-requests', listPayouts);
router.post('/payout-requests', createPayout);
router.get('/stream/:address', streamWalletActivity);

export default router;

