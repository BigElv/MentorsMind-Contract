import { Networks } from '@stellar/stellar-sdk';

export interface StellarConfig {
  network: string;
  horizonUrl: string;
  sorobanRpcUrl: string;
  networkPassphrase: string;
}

type EnvMap = Record<string, string | undefined>;

const env = (import.meta as ImportMeta & { env?: EnvMap }).env ?? {};
const network = env.VITE_STELLAR_NETWORK || 'testnet';

export const stellarConfig: StellarConfig = {
  network,
  horizonUrl: env.VITE_HORIZON_URL || 'https://horizon-testnet.stellar.org',
  sorobanRpcUrl: env.VITE_SOROBAN_RPC_URL || 'https://soroban-testnet.stellar.org:443',
  networkPassphrase: network === 'mainnet' ? Networks.PUBLIC : Networks.TESTNET,
};
