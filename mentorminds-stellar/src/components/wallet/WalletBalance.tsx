import React from 'react';
import { useWallet } from '../../hooks/useWallet';

export default function WalletBalance() {
  const { balances, refresh, status, isHealthy, lastHealthCheck } = useWallet();

  const xlm = balances.find(b => b.asset === 'XLM')?.amount ?? '0';
  const usdc = balances.find(b => b.asset === 'USDC')?.amount ?? '0';
  const pyusd = balances.find(b => b.asset === 'PYUSD')?.amount ?? '0';

  return (
    <section style={{ border: '1px solid #eee', borderRadius: 8, padding: 16, marginBottom: 16 }}>
      <h3>Balances</h3>
      <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
        <div>XLM: {xlm}</div>
        <div>USDC: {usdc}</div>
        <div>PYUSD: {pyusd}</div>
        <button onClick={refresh} disabled={status !== 'connected'}>Refresh</button>
      </div>
      <div style={{ marginTop: 8, fontSize: 12 }}>
        Horizon Health: {isHealthy ? 'Healthy' : 'Unhealthy'}{lastHealthCheck ? ` (checked ${new Date(lastHealthCheck).toLocaleTimeString()})` : ''}
      </div>
    </section>
  );
}
