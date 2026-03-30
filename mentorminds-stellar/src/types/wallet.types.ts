export type AssetCode = 'XLM' | 'USDC' | 'PYUSD';

export interface Balance {
  asset: AssetCode;
  amount: string;
}

export interface TransactionItem {
  id: string;
  txHash: string;
  createdAt: string;
  amount: string;
  asset: string;
  from: string;
  to: string;
  type: 'payment' | 'path_payment' | 'account_merge' | 'unknown';
}

export type WalletStatus = 'idle' | 'connecting' | 'connected' | 'disconnected' | 'error';

export interface WalletState {
  status: WalletStatus;
  publicKey: string;
  network: 'testnet' | 'mainnet' | null;
  balances: Balance[];
  transactions: TransactionItem[];
  error: string | null;
  lastHealthCheck?: number;
  isHealthy: boolean;
}
