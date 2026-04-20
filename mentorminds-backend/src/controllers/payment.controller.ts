import { Request, Response } from 'express';
import { randomUUID } from 'crypto';
import { paymentTrackerService } from '../services/payment-tracker.service';
import { processWebhookEvent } from '../services/stellar-monitor.service';
import { CreatePaymentDto, HorizonWebhookPayload } from '../types/payment.types';

export async function createPayment(req: Request, res: Response): Promise<void> {
  const { sessionId, senderAddress, receiverAddress, amount, assetCode, txHash } =
    req.body as CreatePaymentDto;

  if (!sessionId || !senderAddress || !receiverAddress || !amount || !assetCode || !txHash) {
    res.status(400).json({ error: 'Missing required fields' });
    return;
  }

  const existing = await paymentTrackerService.findByTxHash(txHash);
  if (existing) {
    res.status(409).json({ error: 'Transaction already tracked', payment: existing });
    return;
  }

  const payment = await paymentTrackerService.create({
    id: randomUUID(),
    sessionId,
    senderAddress,
    receiverAddress,
    amount,
    assetCode,
    txHash,
    fee: null,
  });

  res.status(201).json(payment);
}

export async function getPayment(req: Request, res: Response): Promise<void> {
  const payment = await paymentTrackerService.findById(req.params.id);
  if (!payment) {
    res.status(404).json({ error: 'Payment not found' });
    return;
  }
  res.json(payment);
}

export async function getPaymentByTxHash(req: Request, res: Response): Promise<void> {
  const payment = await paymentTrackerService.findByTxHash(req.params.txHash);
  if (!payment) {
    res.status(404).json({ error: 'Payment not found' });
    return;
  }
  res.json(payment);
}

export async function handleWebhook(req: Request, res: Response): Promise<void> {
  const payload = req.body as HorizonWebhookPayload;

  if (!payload.transaction_hash) {
    res.status(400).json({ error: 'Missing transaction_hash' });
    return;
  }

  await processWebhookEvent({
    transaction_hash: payload.transaction_hash,
    ledger: payload.ledger,
    successful: payload.successful,
    result_code: payload.result_code,
  });

  res.status(200).json({ received: true });
}
