import {
  Transaction,
  Networks,
  Keypair,
  xdr,
} from '@stellar/stellar-sdk';

const SIGN_TIMEOUT_MS = 2 * 60 * 1000; // 2 minutes
const RATE_LIMIT_MAX = 5;
const RATE_LIMIT_WINDOW_MS = 60 * 1000;
const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? '/api/v1';

// wallet -> [timestamp, ...]
const rateLimitStore = new Map<string, number[]>();

// track used sequence numbers to prevent replay
const usedSequences = new Set<string>();

export interface SigningResult {
  signedXdr: string;
  txHash: string;
  walletAddress: string;
  sequenceNumber: string;
}

export class SigningError extends Error {
  constructor(
    message: string,
    public readonly code: 'TIMEOUT' | 'RATE_LIMITED' | 'REPLAY' | 'INVALID_SIGNATURE' | 'USER_REJECTED'
  ) {
    super(message);
    this.name = 'SigningError';
  }
}

function checkRateLimit(walletAddress: string): void {
  const now = Date.now();
  const timestamps = (rateLimitStore.get(walletAddress) ?? []).filter(
    t => now - t < RATE_LIMIT_WINDOW_MS
  );
  if (timestamps.length >= RATE_LIMIT_MAX) {
    throw new SigningError(
      `Rate limit exceeded: max ${RATE_LIMIT_MAX} transactions per minute`,
      'RATE_LIMITED'
    );
  }
  timestamps.push(now);
  rateLimitStore.set(walletAddress, timestamps);
}

function checkReplay(networkPassphrase: string, sequenceNumber: string): void {
  const key = `${networkPassphrase}:${sequenceNumber}`;
  if (usedSequences.has(key)) {
    throw new SigningError('Replay attack detected: sequence number already used', 'REPLAY');
  }
  usedSequences.add(key);
}

function verifySignature(signedXdr: string, walletAddress: string, networkPassphrase: string): void {
  const tx = new Transaction(signedXdr, networkPassphrase);
  const keypair = Keypair.fromPublicKey(walletAddress);
  const txHash = tx.hash();

  const valid = tx.signatures.some(sig => {
    try {
      return keypair.verify(txHash, sig.signature());
    } catch {
      return false;
    }
  });

  if (!valid) {
    throw new SigningError('Signature verification failed', 'INVALID_SIGNATURE');
  }
}

async function requestFreighterSignature(xdrEnvelope: string): Promise<string> {
  // Freighter injects window.freighter
  const freighter = (window as any).freighter;
  if (!freighter) throw new Error('Freighter wallet not found');

  const result = await freighter.signTransaction(xdrEnvelope, {
    networkPassphrase: Networks.TESTNET,
  });

  if (result.error) {
    throw new SigningError(result.error, 'USER_REJECTED');
  }
  return result.signedTxXdr ?? result;
}

export async function signTransaction(
  txXdr: string,
  walletAddress: string,
  networkPassphrase: string = Networks.TESTNET
): Promise<SigningResult> {
  checkRateLimit(walletAddress);

  const tx = new Transaction(txXdr, networkPassphrase);
  const sequenceNumber = tx.sequence;
  checkReplay(networkPassphrase, sequenceNumber);

  // Race signing against timeout
  const signedXdr = await Promise.race([
    requestFreighterSignature(txXdr),
    new Promise<never>((_, reject) =>
      setTimeout(
        () => reject(new SigningError('Signing timed out after 2 minutes', 'TIMEOUT')),
        SIGN_TIMEOUT_MS
      )
    ),
  ]);

  verifySignature(signedXdr, walletAddress, networkPassphrase);

  const signedTx = new Transaction(signedXdr, networkPassphrase);
  const txHash = signedTx.hash().toString('hex');

  await fetch(`${API_BASE}/audit-logs`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ txHash, walletAddress, sequenceNumber }),
  }).catch(() => {/* non-blocking */});

  return { signedXdr, txHash, walletAddress, sequenceNumber };
}
