import { Router } from 'express';
import { getNetworkStatus } from '../controllers/network.controller';

const router = Router();

/**
 * Route for network health monitoring at /api/v1/network/status
 */
router.get('/status', getNetworkStatus);

export default router;
