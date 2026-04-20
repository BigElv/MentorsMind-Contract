import { Request, Response } from 'express';
import { randomUUID } from 'crypto';
import { paymentTrackerService } from '../services/payment-tracker.service';

type PayoutStatus = 'pending' | 'approved' | 'rejected' | 'completed';

interface PayoutRequest {
  id: string;
  mentorAddress: string;
  amount: string;
  assetCode: string;
  destination?: string;
  note?: string;
  status: PayoutStatus;
  createdAt: Date;
  updatedAt: Date;
}

const payoutStore = new Map<string, PayoutRequest>();

export async function getEarningsSummary(req: Request, res: Response): Promise<void> {
  const { address } = req.params;
  if (!address) {
    res.status(400).json({ error: 'Missing address' });
    return;
  }

  const pendingPayments = await paymentTrackerService.findPending();
  const pending = pendingPayments.filter(p => p.receiverAddress === address);

  const sum = (arr: any[]) =>
    arr.reduce<Record<string, bigint>>((acc, p) => {
      const code = p.assetCode;
      const amt = BigInt(p.amount);
      acc[code] = (acc[code] ?? 0n) + amt;
      return acc;
    }, {});

  const pendingSum = sum(pending);
  const completedForMentor = [...payoutStore.values()].filter(
    p => p.mentorAddress === address && p.status === 'completed'
  );
  const confirmedSum = completedForMentor.reduce<Record<string, bigint>>((acc, p) => {
    acc[p.assetCode] = (acc[p.assetCode] ?? 0n) + BigInt(p.amount);
    return acc;
  }, {});

  const toStringMap = (m: Record<string, bigint>) =>
    Object.fromEntries(Object.entries(m).map(([k, v]) => [k, v.toString()]));
  res.json({
    address,
    pending: toStringMap(pendingSum),
    confirmed: toStringMap(confirmedSum),
  });
}

export async function createPayout(req: Request, res: Response): Promise<void> {
  const { mentorAddress, amount, assetCode, destination, note } = req.body ?? {};
  if (!mentorAddress || !amount || !assetCode) {
    res.status(400).json({ error: 'mentorAddress, amount and assetCode are required' });
    return;
  }

  const pr: PayoutRequest = {
    id: randomUUID(),
    mentorAddress,
    amount,
    assetCode,
    destination,
    note,
    status: 'pending',
    createdAt: new Date(),
    updatedAt: new Date(),
  };
  payoutStore.set(pr.id, pr);
  res.status(201).json(pr);
}

export async function listPayouts(req: Request, res: Response): Promise<void> {
  const { address } = req.params;
  if (!address) {
    res.status(400).json({ error: 'Missing address' });
    return;
  }
  const items = [...payoutStore.values()].filter(p => p.mentorAddress === address);
  res.json(items);
}

export async function streamWalletActivity(req: Request, res: Response): Promise<void> {
  const { address } = req.params;
  if (!address) {
    res.status(400).json({ error: 'Missing address' });
    return;
  }

  res.setHeader('Content-Type', 'text/event-stream');
  res.setHeader('Cache-Control', 'no-cache');
  res.setHeader('Connection', 'keep-alive');
  res.flushHeaders();

  let closed = false;
  req.on('close', () => {
    closed = true;
  });

  let lastSnapshot = '';
  const interval = setInterval(async () => {
    if (closed) {
      clearInterval(interval);
      return;
    }
    const pending = await paymentTrackerService.findPending();
    const mine = pending.filter(p => p.receiverAddress === address);
    const snapshot = JSON.stringify(
      mine.map(p => ({ txHash: p.txHash, status: p.status, amount: p.amount, asset: p.assetCode }))
    );
    if (snapshot !== lastSnapshot) {
      lastSnapshot = snapshot;
      res.write(`event: payment_update\n`);
      res.write(`data: ${snapshot}\n\n`);
    }
  }, 5000);
}

