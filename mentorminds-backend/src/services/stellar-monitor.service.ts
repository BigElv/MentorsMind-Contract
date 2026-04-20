import { paymentTrackerService } from './payment-tracker.service';

const HORIZON_URL = process.env.HORIZON_URL ?? 'https://horizon-testnet.stellar.org';
const POLL_INTERVAL_MS = 10_000;

// Maps Horizon result codes to user-readable messages
const RESULT_CODE_MESSAGES: Record<string, string> = {
  tx_bad_auth: 'Invalid transaction signature.',
  tx_insufficient_balance: 'Insufficient balance to complete payment.',
  tx_no_account: 'Sender account not found on the network.',
  tx_failed: 'Transaction failed on the Stellar network.',
};

async function fetchTransaction(txHash: string): Promise<{ successful: boolean; ledger: number; fee_paid?: string; result_code?: string } | null> {
  const res = await fetch(`${HORIZON_URL}/transactions/${txHash}`);
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(`Horizon error: ${res.status}`);

  const data = await res.json();
  return {
    successful: data.successful,
    ledger: data.ledger,
    fee_paid: data.fee_paid,
    result_code: data.result_codes?.transaction,
  };
}

async function pollPending(): Promise<void> {
  const pending = await paymentTrackerService.findPending();

  await Promise.allSettled(
    pending.map(async (payment) => {
      if (!payment.txHash) return;

      try {
        const tx = await fetchTransaction(payment.txHash);
        if (!tx) return; // not yet on ledger

        if (tx.successful) {
          await paymentTrackerService.updateStatus(payment.id, 'confirmed', {
            ledgerSequence: tx.ledger,
            fee: tx.fee_paid,
          });
        } else {
          const errorCode = tx.result_code ?? 'tx_failed';
          await paymentTrackerService.updateStatus(payment.id, 'failed', {
            ledgerSequence: tx.ledger,
            fee: tx.fee_paid,
            errorCode,
            errorMessage: RESULT_CODE_MESSAGES[errorCode] ?? 'Transaction failed.',
          });
        }
      } catch {
        // transient error — will retry next poll cycle
      }
    })
  );

  await paymentTrackerService.timeoutStalePending();
}

export function startStellarMonitor(): void {
  setInterval(pollPending, POLL_INTERVAL_MS);
  console.log(`Stellar monitor started (poll every ${POLL_INTERVAL_MS / 1000}s)`);
}

export async function processWebhookEvent(payload: {
  transaction_hash: string;
  ledger: number;
  successful: boolean;
  fee_paid?: string;
  result_code?: string;
}): Promise<void> {
  const payment = await paymentTrackerService.findByTxHash(payload.transaction_hash);
  if (!payment || payment.status !== 'pending') return;

  if (payload.successful) {
    await paymentTrackerService.updateStatus(payment.id, 'confirmed', {
      ledgerSequence: payload.ledger,
      fee: payload.fee_paid,
    });
  } else {
    const errorCode = payload.result_code ?? 'tx_failed';
    await paymentTrackerService.updateStatus(payment.id, 'failed', {
      ledgerSequence: payload.ledger,
      fee: payload.fee_paid,
      errorCode,
      errorMessage: RESULT_CODE_MESSAGES[errorCode] ?? 'Transaction failed.',
    });
  }
}
