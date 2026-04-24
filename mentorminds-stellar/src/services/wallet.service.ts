import { Keypair } from '@stellar/stellar-sdk';
import FreighterApi from '@stellar/freighter-api';
import { fetchAccountBalances, fetchTransactions, Balance, TransactionItem } from './mentor-wallet.service';

const HORIZON_URL = process.env.NEXT_PUBLIC_HORIZON_URL ?? 'https://horizon-testnet.stellar.org';
const FRIENDBOT_URL = process.env.NEXT_PUBLIC_FRIENDBOT_URL ?? 'https://friendbot.stellar.org';

export function detectFreighter(): boolean {
  if (typeof window === 'undefined') return false;
  if ((window as any).freighterApi) return true;
  if (typeof FreighterApi !== 'undefined') return true;
  return false;
}

export async function connectFreighter(): Promise<{ publicKey: string; network: 'testnet' | 'mainnet' }> {
  const freighter = getFreighterApi();
  if (!freighter) throw new Error('Freighter extension not found');

  const isAvailable = await freighter.isAvailable();
  if (!isAvailable) throw new Error('Freighter extension not available');

  // Some versions use connect() and/or getPublicKey() directly.
  if ('connect' in freighter) {
    await (freighter as any).connect?.();
  }

  const publicKey = await freighter.getPublicKey();
  const networkRaw = await freighter.getNetwork?.();
  const network = networkRaw === 'testnet' || networkRaw === 'mainnet' ? networkRaw : 'testnet';

  return { publicKey, network };
}

export async function getFreighterPublicKey(): Promise<string> {
  const freighter = getFreighterApi();
  if (!freighter) throw new Error('Freighter extension not found');
  return freighter.getPublicKey();
}

export function getFreighterApi(): any | null {
  if (typeof window === 'undefined') return null;
  const globalFreighter = (window as any).freighterApi;
  if (globalFreighter) return globalFreighter;
  try {
    return FreighterApi;
  } catch {
    return null;
  }
}

export function validateStellarKey(publicKey: string): boolean {
  try {
    return Keypair.fromPublicKey(publicKey) != null;
  } catch {
    return false;
  }
}

export async function importStellarAccount(secret: string): Promise<{ publicKey: string }> {
  try {
    const keypair = Keypair.fromSecret(secret);
    return { publicKey: keypair.publicKey() };
  } catch (error) {
    throw new Error('Invalid secret key');
  }
}

export async function fetchLiveBalances(publicKey: string): Promise<Balance[]> {
  return fetchAccountBalances(publicKey);
}

export async function fetchTransactionHistory(publicKey: string): Promise<TransactionItem[]> {
  return fetchTransactions(publicKey);
}

export async function createFriendbotFunding(publicKey: string): Promise<void> {
  if (!validateStellarKey(publicKey)) throw new Error('Invalid public key for funding');

  const res = await fetch(`${FRIENDBOT_URL}?addr=${encodeURIComponent(publicKey)}`);
  if (!res.ok) {
    const text = await res.text().catch(() => 'unknown error');
    throw new Error(`Friendbot funding failed: ${res.status} ${text}`);
  }
}

export async function checkHorizonHealth(): Promise<{ status: 'healthy' | 'degraded' | 'down'; ledger?: number; latencyMs?: number }> {
  const t0 = Date.now();
  try {
    const res = await fetch(`${HORIZON_URL}/ledgers?limit=1`);
    const duration = Date.now() - t0;
    if (!res.ok) {
      return { status: 'down', latencyMs: duration };
    }
    const data = await res.json();
    const latestLedger = data._embedded?.records?.[0]?.sequence;
    return { status: 'healthy', ledger: latestLedger, latencyMs: duration };
  } catch {
    return { status: 'down', latencyMs: Date.now() - t0 };
  }
}
