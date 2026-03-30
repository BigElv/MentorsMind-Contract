import React, { useEffect, useState } from 'react';

type NetworkStatus = {
  status: 'ok' | 'degraded' | 'down';
  activeHorizon: string;
  primary: string;
  backup: string;
  ledger?: number;
  latencyMs?: number;
};

export default function NetworkStatus() {
  const [status, setStatus] = useState<NetworkStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadStatus = async () => {
    try {
      const res = await fetch('/api/v1/network/status');
      if (!res.ok) throw new Error('Network API failed');
      const data = await res.json();
      setStatus(data);
      setError(null);
    } catch (err: any) {
      setError(err?.message ?? 'Failed to load network status');
      setStatus(null);
    }
  };

  useEffect(() => {
    loadStatus();
    const interval = setInterval(loadStatus, 15000);
    return () => clearInterval(interval);
  }, []);

  if (error) {
    return <div style={{ padding: 8, border: '1px solid #f4b8b8', background: '#fff0f0', borderRadius: 6 }}>Network status error: {error}</div>;
  }

  if (!status) return <div style={{ padding: 8 }}>Loading network status…</div>;

  const colors: Record<string, string> = { ok: '#34a853', degraded: '#f8ae34', down: '#d93025' };

  return (
    <div style={{ padding: 8, border: `1px solid ${colors[status.status]}`, borderRadius: 6, backgroundColor: '#fff', display: 'flex', justifyContent: 'space-between', gap: 12, alignItems: 'center' }}>
      <strong>Network {status.status.toUpperCase()}</strong>
      <span>Ledger: {status.ledger ?? 'n/a'}</span>
      <span>Latency: {status.latencyMs ? `${status.latencyMs}ms` : 'n/a'}</span>
      <span>Active Horizon: {status.activeHorizon}</span>
    </div>
  );
}
