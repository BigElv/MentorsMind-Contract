import React, { createContext, useContext, useEffect, useState } from 'react';
import { stellarService } from '../services/stellar.service';
import { StellarAccount, StellarNetwork, StellarState } from '../types/stellar.types';

interface StellarContextType {
  state: StellarState;
  switchNetwork: (network: StellarNetwork) => void;
  getAccountDetails: (publicKey: string) => Promise<StellarAccount>;
  getAccountBalance: (publicKey: string) => Promise<string>;
}

const StellarContext = createContext<StellarContextType | undefined>(undefined);

export const StellarProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [state, setState] = useState<StellarState>({
    isConnected: true,
    network: 'testnet',
    account: null,
    error: null,
    loading: false,
  });

  const switchNetwork = (network: StellarNetwork) => {
    stellarService.switchNetwork(network);
    setState(prev => ({ ...prev, network }));
  };

  const getAccountDetails = async (publicKey: string): Promise<StellarAccount> => {
    setState(prev => ({ ...prev, loading: true, error: null }));
    try {
      const account = await stellarService.getAccountDetails(publicKey);
      setState(prev => ({ ...prev, account, loading: false }));
      return account;
    } catch (error: any) {
      setState(prev => ({ ...prev, error: error.message, loading: false }));
      throw error;
    }
  };

  const getAccountBalance = async (publicKey: string): Promise<string> => {
    setState(prev => ({ ...prev, loading: true, error: null }));
    try {
      const balance = await stellarService.getAccountBalance(publicKey);
      setState(prev => ({ ...prev, loading: false }));
      return balance;
    } catch (error: any) {
      setState(prev => ({ ...prev, error: error.message, loading: false }));
      throw error;
    }
  };

  return (
    <StellarContext.Provider value={{ state, switchNetwork, getAccountDetails, getAccountBalance }}>
      {children}
    </StellarContext.Provider>
  );
};

export const useStellar = () => {
  const context = useContext(StellarContext);
  if (context === undefined) {
    throw new Error('useStellar must be used within a StellarProvider');
  }
  return context;
};
