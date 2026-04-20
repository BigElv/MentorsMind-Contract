import React from 'react';
import { useWallet } from '../../hooks/useWallet';

export default function TransactionHistory() {
  const { transactions, status } = useWallet();

  return (
    <section style={{ border: '1px solid #eee', borderRadius: 8, padding: 16, marginBottom: 16 }}>
      <h3>Transaction History</h3>
      {status !== 'connected' ? <p>Connect wallet to view history.</p> : null}
      <div style={{ overflowX: 'auto', maxHeight: 400 }}>
        <table width="100%" style={{ borderCollapse: 'collapse', fontSize: 12, minWidth: 640 }}>
          <thead>
            <tr>
              <th align="left">Time</th>
              <th align="left">Type</th>
              <th align="left">Asset</th>
              <th align="left">Amount</th>
              <th align="left">From</th>
              <th align="left">To</th>
              <th align="left">Tx Hash</th>
            </tr>
          </thead>
          <tbody>
            {transactions.map(tx => (
              <tr key={tx.id} style={{ borderTop: '1px solid #eee' }}>
                <td>{new Date(tx.createdAt).toLocaleString()}</td>
                <td>{tx.type}</td>
                <td>{tx.asset || 'XLM'}</td>
                <td>{tx.amount}</td>
                <td>{tx.from}</td>
                <td>{tx.to}</td>
                <td style={{ wordBreak: 'break-word' }}>{tx.txHash}</td>
              </tr>
            ))}
            {!transactions.length && (
              <tr>
                <td colSpan={7} style={{ opacity: 0.7, padding: 12 }}>
                  No transaction history available.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </section>
  );
}
