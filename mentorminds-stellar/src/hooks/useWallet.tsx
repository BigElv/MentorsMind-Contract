import React, { createContext, useCallback, useContext, useEffect, useState } from 'react';
import { Balance, TransactionItem } from '../types/wallet.types';
import {
  detectFreighter,
  connectFreighter,
  importStellarAccount,
  createFriendbotFunding,
  fetchLiveBalances,
  fetchTransactionHistory,
  validateStellarKey,
  checkHorizonHealth,
} from '../services/wallet.service';

export interface WalletState {
  status: 'idle' | 'connecting' | 'connected' | 'disconnected' | 'error';
  publicKey: string;
  network: 'testnet' | 'mainnet' | null;
  balances: Balance[];
  transactions: TransactionItem[];
  error: string | null;
  isFreighterAvailable: boolean;
  isHealthy: boolean;
  lastHealthCheck?: number;
}

export interface WalletHook extends WalletState {
  connect: () => Promise<void>;
  disconnect: () => void;
  refresh: () => Promise<void>;
  importAccount: (secret: string) => Promise<void>;
  fundWithFriendbot: () => Promise<void>;
  validateKey: (key: string) => boolean;
  runHealthCheck: () => Promise<void>;
}

const initialState: WalletState = {
  status: 'idle',
  publicKey: '',
  network: null,
  balances: [],
  transactions: [],
  error: null,
  isFreighterAvailable: false,
  isHealthy: true,
  lastHealthCheck: undefined,
};

const WalletContext = createContext<WalletHook | undefined>(undefined);

export const WalletProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [state, setState] = useState<WalletState>(initialState);

  const runHealthCheck = useCallback(async () => {
    try {
      const data = await checkHorizonHealth();
      setState(prev => ({ ...prev, isHealthy: data.status === 'healthy', lastHealthCheck: Date.now() }));
    } catch (error: any) {
      setState(prev => ({ ...prev, isHealthy: false, lastHealthCheck: Date.now(), error: error?.message ?? 'Health check failed' }));
    }
  }, []);

  useEffect(() => {
    setState(prev => ({ ...prev, isFreighterAvailable: detectFreighter() }));
    runHealthCheck();
    const interval = setInterval(() => runHealthCheck(), 30_000);
    return () => clearInterval(interval);
  }, [runHealthCheck]);

  const loadWalletData = useCallback(async (publicKey: string) => {
    const balances = await fetchLiveBalances(publicKey);
    const transactions = await fetchTransactionHistory(publicKey);
    setState(prev => ({ ...prev, balances, transactions }));
  }, []);

  const connect = useCallback(async () => {
    setState(prev => ({ ...prev, status: 'connecting', error: null }));
    try {
      const { publicKey, network } = await connectFreighter();
      if (!validateStellarKey(publicKey)) throw new Error('Received invalid public key from Freighter');
      setState(prev => ({ ...prev, publicKey, network, status: 'connected', error: null }));
      await loadWalletData(publicKey);
    } catch (error: any) {
      setState(prev => ({ ...prev, status: 'error', error: error?.message ?? 'Freighter connection failed' }));
      throw error;
    }
  }, [loadWalletData]);

  const disconnect = useCallback(() => setState(initialState), []);

  const refresh = useCallback(async () => {
    if (!state.publicKey) return;
    setState(prev => ({ ...prev, status: 'connected', error: null }));
    await loadWalletData(state.publicKey);
    await runHealthCheck();
  }, [state.publicKey, loadWalletData, runHealthCheck]);

  const importAccount = useCallback(async (secret: string) => {
    setState(prev => ({ ...prev, status: 'connecting', error: null }));
    try {
      const { publicKey } = await importStellarAccount(secret);
      setState(prev => ({ ...prev, publicKey, network: 'testnet', status: 'connected', error: null }));
      await loadWalletData(publicKey);
    } catch (error: any) {
      setState(prev => ({ ...prev, status: 'error', error: error?.message ?? 'Import account failed' }));
      throw error;
    }
  }, [loadWalletData]);

  const fundWithFriendbot = useCallback(async () => {
    if (!state.publicKey) throw new Error('No wallet connected/imported');
    try {
      await createFriendbotFunding(state.publicKey);
      await refresh();
    } catch (error: any) {
      setState(prev => ({ ...prev, error: error?.message ?? 'Friendbot failed' }));
      throw error;
    }
  }, [state.publicKey, refresh]);

  const value: WalletHook = {
    ...state,
    connect,
    disconnect,
    refresh,
    importAccount,
    fundWithFriendbot,
    validateKey: validateStellarKey,
    runHealthCheck,
  };

  return <WalletContext.Provider value={value}>{children}</WalletContext.Provider>;
};

export const useWallet = (): WalletHook => {
  const context = useContext(WalletContext);
  if (!context) {
    throw new Error('useWallet must be used within WalletProvider');
  }
  return context;
};
