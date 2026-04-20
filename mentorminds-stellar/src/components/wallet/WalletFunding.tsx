import React, { useState } from 'react';
import { useWallet } from '../../hooks/useWallet';

export default function WalletFunding() {
  const { fundWithFriendbot, isHealthy, publicKey } = useWallet();
  const [status, setStatus] = useState<string>('');

  const onFund = async () => {
    if (!publicKey) return;

    setStatus('Funding via Friendbot...');
    try {
      await fundWithFriendbot();
      setStatus('Funding successful!');
    } catch (error: any) {
      setStatus(`Funding failed: ${error?.message ?? error}`);
    }
  };

  return (
    <section style={{ border: '1px solid #eee', borderRadius: 8, padding: 16, marginBottom: 16 }}>
      <h3>Wallet Funding (Testnet)</h3>
      <button onClick={onFund} disabled={!isHealthy || !publicKey}>
        Fund from Friendbot
      </button>
      {status ? <p style={{ marginTop: 8 }}>{status}</p> : null}
      {!isHealthy && <p style={{ color: '#B34747' }}>Horizon is unhealthy, please retry after a moment.</p>}
      {!publicKey && <p style={{ color: '#555' }}>Connect or import an account before funding.</p>}
    </section>
  );
}
