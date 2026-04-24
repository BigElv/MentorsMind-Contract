import React from 'react';
import { useWallet } from '../../hooks/useWallet';

export default function WalletConnect() {
  const { status, publicKey, network, isFreighterAvailable, error, connect, disconnect } = useWallet();

  const renderConnect = () => {
    if (!isFreighterAvailable) {
      return <p style={{ color: '#B34747' }}>Freighter browser extension is not installed. Please install it to continue.</p>;
    }

    if (status === 'connected' && publicKey) {
      return (
        <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
          <div style={{ fontSize: 14 }}>
            Connected as <strong>{publicKey}</strong> ({network})
          </div>
          <button onClick={disconnect}>Disconnect</button>
        </div>
      );
    }

    return (
      <button onClick={connect} disabled={status === 'connecting'}>
        {status === 'connecting' ? 'Connecting...' : 'Connect Freighter Wallet'}
      </button>
    );
  };

  return (
    <section style={{ border: '1px solid #eee', borderRadius: 8, padding: 16, marginBottom: 16 }}>
      <h3>Wallet Connection</h3>
      {renderConnect()}
      {error ? <p style={{ color: '#B34747' }}>{error}</p> : null}
    </section>
  );
}
