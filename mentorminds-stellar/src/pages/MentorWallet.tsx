'use client';

import WalletConnect from '../components/wallet/WalletConnect';
import WalletBalance from '../components/wallet/WalletBalance';
import WalletFunding from '../components/wallet/WalletFunding';
import WalletBackup from '../components/wallet/WalletBackup';
import TransactionHistory from '../components/wallet/TransactionHistory';
import NetworkStatus from '../components/ui/NetworkStatus';

function MentorWalletContent() {
  return (
    <div style={{ padding: 24, display: 'grid', gap: 16 }}>
      <h2>Mentor Wallet</h2>
      <NetworkStatus />
      <WalletConnect />
      <WalletBalance />
      <WalletFunding />
      <WalletBackup />
      <TransactionHistory />
    </div>
  );
}

export default function MentorWallet() {
  return <MentorWalletContent />;
}
