'use client';

import { useEffect, useState } from 'react';
import { Transaction, Networks } from '@stellar/stellar-sdk';
import { signTransaction, SigningError, SigningResult } from '../../services/signing.service';

interface Props {
  txXdr: string;
  walletAddress: string;
  onSuccess: (result: SigningResult) => void;
  onCancel: () => void;
}

const TIMEOUT_SECONDS = 120;

function parsePreview(txXdr: string) {
  try {
    const tx = new Transaction(txXdr, Networks.TESTNET);
    return {
      fee: `${parseInt(tx.fee) / 1e7} XLM`,
      sequence: tx.sequence,
      operations: tx.operations.map(op => op.type),
    };
  } catch {
    return null;
  }
}

export default function TransactionSigning({ txXdr, walletAddress, onSuccess, onCancel }: Props) {
  const [secondsLeft, setSecondsLeft] = useState(TIMEOUT_SECONDS);
  const [status, setStatus] = useState<'preview' | 'signing' | 'error'>('preview');
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const preview = parsePreview(txXdr);

  // Countdown timer
  useEffect(() => {
    if (status !== 'preview') return;
    if (secondsLeft <= 0) {
      onCancel();
      return;
    }
    const t = setTimeout(() => setSecondsLeft(s => s - 1), 1000);
    return () => clearTimeout(t);
  }, [secondsLeft, status, onCancel]);

  async function handleSign() {
    setStatus('signing');
    try {
      const result = await signTransaction(txXdr, walletAddress);
      onSuccess(result);
    } catch (err) {
      const msg = err instanceof SigningError ? err.message : 'Signing failed';
      setErrorMsg(msg);
      setStatus('error');
    }
  }

  if (status === 'error') {
    return (
      <div role="alert" style={{ padding: 16, border: '1px solid red', borderRadius: 8 }}>
        <p>❌ {errorMsg}</p>
        <button onClick={onCancel}>Close</button>
      </div>
    );
  }

  return (
    <div role="dialog" aria-modal="true" aria-label="Review Transaction" style={{ padding: 16, border: '1px solid #ccc', borderRadius: 8 }}>
      <h2>Review Transaction</h2>

      {preview ? (
        <dl>
          <dt>Fee</dt><dd>{preview.fee}</dd>
          <dt>Sequence</dt><dd>{preview.sequence}</dd>
          <dt>Operations</dt><dd>{preview.operations.join(', ')}</dd>
        </dl>
      ) : (
        <p>Unable to parse transaction details.</p>
      )}

      <p>Wallet: <code>{walletAddress}</code></p>

      {status === 'preview' && (
        <p aria-live="polite">
          Auto-cancels in <strong>{secondsLeft}s</strong>
        </p>
      )}

      <div style={{ display: 'flex', gap: 8, marginTop: 12 }}>
        <button onClick={onCancel} disabled={status === 'signing'}>
          Cancel
        </button>
        <button onClick={handleSign} disabled={status === 'signing'}>
          {status === 'signing' ? 'Waiting for Freighter…' : 'Sign & Submit'}
        </button>
      </div>
    </div>
  );
}
