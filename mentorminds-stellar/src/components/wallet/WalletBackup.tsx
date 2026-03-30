import React, { useState } from 'react';
import { useWallet } from '../../hooks/useWallet';

export default function WalletBackup() {
  const { publicKey, importAccount } = useWallet();
  const [manualKey, setManualKey] = useState('');
  const [importStatus, setImportStatus] = useState('');

  const onBackup = () => {
    if (!publicKey) return;
    alert('Please store your private key securely offline in an encrypted password manager. This UI cannot retrieve private keys from Freighter.');
  };

  const onImport = async () => {
    if (!manualKey) return;
    try {
      setImportStatus('Importing...');
      await importAccount(manualKey.trim());
      setImportStatus('Import successful');
      setManualKey('');
    } catch (e: any) {
      setImportStatus(`Import failed: ${e?.message ?? 'Unknown error'}`);
    }
  };

  return (
    <section style={{ border: '1px solid #eee', borderRadius: 8, padding: 16, marginBottom: 16 }}>
      <h3>Backup / Export Reminder</h3>
      <p>
        Always keep your secret key safe. Do not share it with anyone. 
        For Freighter-connected wallets, this client cannot access the secret key.
      </p>
      <button onClick={onBackup} disabled={!publicKey}>
        Acknowledge Backup Reminders
      </button>
      <div style={{ marginTop: 12 }}>
        <strong>Important:</strong> When you import a secret key below, store that secret safely and remove it from your clipboard or browser history.
      </div>

      <div style={{ marginTop: 16 }}>
        <input
          value={manualKey}
          onChange={e => setManualKey(e.target.value)}
          placeholder="Enter existing secret key to import"
          style={{ width: '100%', marginBottom: 8 }}
        />
        <button onClick={onImport} disabled={!manualKey}>Import Existing Account</button>
      </div>
      {importStatus ? <p style={{ marginTop: 8 }}>{importStatus}</p> : null}
    </section>
  );
}
