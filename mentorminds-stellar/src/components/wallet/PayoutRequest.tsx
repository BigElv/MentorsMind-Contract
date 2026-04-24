'use client';

import { useEffect, useMemo, useState } from 'react';
import { AssetCode, createPayoutRequest, listPayoutRequests } from '../../services/mentor-wallet.service';

interface Props {
  mentorAddress: string;
}

export default function PayoutRequest({ mentorAddress }: Props) {
  const [amount, setAmount] = useState('');
  const [asset, setAsset] = useState<AssetCode>('USDC');
  const [destination, setDestination] = useState('');
  const [note, setNote] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [requests, setRequests] = useState<any[]>([]);

  const canSubmit = useMemo(() => {
    if (!mentorAddress) return false;
    const v = Number(amount);
    return Number.isFinite(v) && v > 0;
  }, [mentorAddress, amount]);

  async function refresh() {
    if (!mentorAddress) return;
    const data = await listPayoutRequests(mentorAddress);
    setRequests(data);
  }

  useEffect(() => {
    refresh();
  }, [mentorAddress]);

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!canSubmit) return;
    setSubmitting(true);
    setError(null);
    try {
      await createPayoutRequest({
        mentorAddress,
        amount,
        assetCode: asset,
        destination: destination || undefined,
        note: note || undefined,
      });
      setAmount('');
      setDestination('');
      setNote('');
      await refresh();
    } catch (err: any) {
      setError(err.message ?? 'Failed');
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div>
      <form onSubmit={onSubmit} style={{ display: 'grid', gap: 8, maxWidth: 480 }}>
        <div>
          <label>Amount</label>
          <input value={amount} onChange={e => setAmount(e.target.value)} placeholder="0.00" />
        </div>
        <div>
          <label>Asset</label>
          <select value={asset} onChange={e => setAsset(e.target.value as AssetCode)}>
            <option value="USDC">USDC</option>
            <option value="PYUSD">PYUSD</option>
            <option value="XLM">XLM</option>
          </select>
        </div>
        <div>
          <label>Destination (optional)</label>
          <input value={destination} onChange={e => setDestination(e.target.value)} placeholder="Stellar address or notes" />
        </div>
        <div>
          <label>Note (optional)</label>
          <input value={note} onChange={e => setNote(e.target.value)} placeholder="Additional info" />
        </div>
        <button type="submit" disabled={!canSubmit || submitting}>{submitting ? 'Submitting…' : 'Request Payout'}</button>
        {error ? <div style={{ color: 'crimson' }}>{error}</div> : null}
      </form>
      <div style={{ marginTop: 16 }}>
        <h4>Payout Requests</h4>
        <table style={{ width: '100%', borderCollapse: 'collapse' }}>
          <thead>
            <tr>
              <th style={{ textAlign: 'left' }}>Date</th>
              <th style={{ textAlign: 'left' }}>Amount</th>
              <th style={{ textAlign: 'left' }}>Asset</th>
              <th style={{ textAlign: 'left' }}>Status</th>
            </tr>
          </thead>
          <tbody>
            {requests.map(r => (
              <tr key={r.id}>
                <td>{new Date(r.createdAt).toLocaleString()}</td>
                <td>{r.amount}</td>
                <td>{r.assetCode}</td>
                <td>{r.status}</td>
              </tr>
            ))}
            {!requests.length ? (
              <tr><td colSpan={4} style={{ opacity: 0.7 }}>No requests yet</td></tr>
            ) : null}
          </tbody>
        </table>
      </div>
    </div>
  );
}

