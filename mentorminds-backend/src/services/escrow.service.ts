import {
  Keypair,
  rpc,
  TransactionBuilder,
  Networks,
  Contract,
  nativeToScVal,
} from 'stellar-sdk';

const RPC_TIMEOUT_MS = 10_000;
const MAX_RETRIES = 3;
const RETRY_DELAY_MS = 1_500;

/** Rejects after `ms` milliseconds with a timeout error. */
function withTimeout<T>(promise: Promise<T>, ms: number, label: string): Promise<T> {
  return Promise.race([
    promise,
    new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error(`${label} timed out after ${ms}ms`)), ms)
    ),
  ]);
}

/**
 * Retry wrapper with per-attempt AbortController so in-flight requests
 * from a previous attempt don't bleed into the next one.
 */
async function withRetry<T>(
  fn: (signal: AbortSignal) => Promise<T>,
  retries: number = MAX_RETRIES,
  delayMs: number = RETRY_DELAY_MS
): Promise<T> {
  let lastError: unknown;
  for (let attempt = 1; attempt <= retries; attempt++) {
    const controller = new AbortController();
    try {
      return await fn(controller.signal);
    } catch (err) {
      controller.abort();
      lastError = err;
      if (attempt < retries) {
        await new Promise((r) => setTimeout(r, delayMs));
      }
    }
  }
  throw lastError;
}

export class AdminEscrowService {
  private contract: Contract;
  private server: rpc.Server;
  private adminKeypair: Keypair;

  constructor(contractId: string, rpcUrl: string, adminSecret: string) {
    this.contract = new Contract(contractId);
    this.server = new rpc.Server(rpcUrl, {
      allowHttp: rpcUrl.startsWith('http://'),
      timeout: RPC_TIMEOUT_MS,
    });
    this.adminKeypair = Keypair.fromSecret(adminSecret);
  }

  async resolveDispute(escrowId: number, releaseToMentor: boolean): Promise<string> {
    return withRetry(async (_signal) => {
      const sourceAccount = await withTimeout(
        this.server.getAccount(this.adminKeypair.publicKey()),
        RPC_TIMEOUT_MS,
        'getAccount'
      );

      const operation = this.contract.call(
        'resolve_dispute',
        nativeToScVal(escrowId, { type: 'u64' }),
        nativeToScVal(releaseToMentor, { type: 'bool' })
      );

      const transaction = new TransactionBuilder(sourceAccount, {
        fee: '1000',
        networkPassphrase: Networks.TESTNET,
      })
        .addOperation(operation)
        .setTimeout(60)
        .build();

      transaction.sign(this.adminKeypair);

      const sendResponse = await withTimeout(
        this.server.sendTransaction(transaction),
        RPC_TIMEOUT_MS,
        'sendTransaction'
      ) as Awaited<ReturnType<typeof this.server.sendTransaction>>;

      if (sendResponse.status !== 'PENDING') {
        throw new Error(`Failed to send transaction: ${sendResponse.status}`);
      }

      return sendResponse.hash;
    });
  }

  async refund(escrowId: number): Promise<string> {
    return withRetry(async (_signal) => {
      const sourceAccount = await withTimeout(
        this.server.getAccount(this.adminKeypair.publicKey()),
        RPC_TIMEOUT_MS,
        'getAccount'
      );

      const operation = this.contract.call('refund', nativeToScVal(escrowId, { type: 'u64' }));

      const transaction = new TransactionBuilder(sourceAccount, {
        fee: '1000',
        networkPassphrase: Networks.TESTNET,
      })
        .addOperation(operation)
        .setTimeout(60)
        .build();

      transaction.sign(this.adminKeypair);

      const res = await withTimeout(
        this.server.sendTransaction(transaction),
        RPC_TIMEOUT_MS,
        'sendTransaction'
      ) as Awaited<ReturnType<typeof this.server.sendTransaction>>;

      return res.hash;
    });
  }
}
