import { Router } from 'express';
import {
  createPayment,
  getPayment,
  getPaymentByTxHash,
  handleWebhook,
} from '../controllers/payment.controller';

const router = Router();

router.post('/', createPayment);
router.get('/:id', getPayment);
router.get('/tx/:txHash', getPaymentByTxHash);
router.post('/webhook', handleWebhook);

export default router;
