export interface StellarAccount {
  publicKey: string;
  balance: string;
  assetCode: string;
}

export interface StellarTransaction {
  id: string;
  from: string;
  to: string;
  amount: string;
  asset: string;
  createdAt: string;
}

export type StellarNetwork = 'testnet' | 'mainnet';

export interface StellarState {
  isConnected: boolean;
  network: StellarNetwork;
  account: StellarAccount | null;
  error: string | null;
  loading: boolean;
}
