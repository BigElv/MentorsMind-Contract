import React, { useState } from 'react';
import { MultiSigService } from '../../services/multisig.service';

interface MultiSigWalletProps {
  contractId: string;
  adminAddress: string;
}

export const MultiSigWallet: React.FC<MultiSigWalletProps> = ({ contractId, adminAddress }) => {
  const [signerAddress, setSignerAddress] = useState('');
  const [threshold, setThreshold] = useState<number>(2);
  const multisigService = new MultiSigService(contractId);

  const handleAddSigner = async () => {
    try {
      await multisigService.addSigner(adminAddress, signerAddress);
      alert('Signer added successfully!');
    } catch (error) {
      console.error(error);
      alert('Failed to add signer');
    }
  };

  const handleUpdateThreshold = async () => {
    try {
      await multisigService.updateThreshold(adminAddress, threshold);
      alert('Threshold updated successfully!');
    } catch (error) {
      console.error(error);
      alert('Failed to update threshold');
    }
  };

  return (
    <div className="p-4 bg-white rounded shadow-md">
      <h2 className="text-2xl font-bold mb-4">Multi-Signature Wallet Admin</h2>
      
      <div className="mb-6">
        <h3 className="text-xl font-semibold mb-2">Manage Signers</h3>
        <div className="flex gap-2">
          <input 
            type="text" 
            placeholder="Signer Address" 
            className="border p-2 rounded w-full"
            value={signerAddress}
            onChange={(e) => setSignerAddress(e.target.value)}
          />
          <button 
            className="bg-blue-500 text-white px-4 py-2 rounded hover:bg-blue-600"
            onClick={handleAddSigner}
          >
            Add Signer
          </button>
        </div>
      </div>

      <div>
        <h3 className="text-xl font-semibold mb-2">Update Threshold</h3>
        <div className="flex gap-2">
          <input 
            type="number" 
            min="1"
            className="border p-2 rounded w-32"
            value={threshold}
            onChange={(e) => setThreshold(Number(e.target.value))}
          />
          <button 
            className="bg-green-500 text-white px-4 py-2 rounded hover:bg-green-600"
            onClick={handleUpdateThreshold}
          >
            Update Threshold
          </button>
        </div>
      </div>
    </div>
  );
};
