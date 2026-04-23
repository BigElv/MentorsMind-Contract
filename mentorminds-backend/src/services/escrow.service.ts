import {
  Keypair,
  rpc,
  TransactionBuilder,
  Networks,
  Contract,
  nativeToScVal,
} from 'stellar-sdk';

// ── Startup SDK capability check ────────────────────────────────────────────
// Fail loudly at import time rather than producing a cryptic Soroban type
// error at runtime when a contract invocation is attempted.
(function assertSorobanCapable() {
  if (typeof nativeToScVal !== 'function') {
    throw new Error(
      'stellar-sdk version does not support nativeToScVal — upgrade to v10.4+ ' +
      '(current package.json pins "stellar-sdk": "10.4.0")'
    );
  }
  if (typeof rpc?.Server !== 'function') {
    throw new Error(
      'stellar-sdk version does not expose rpc.Server — Soroban RPC support ' +
      'requires stellar-sdk v10.4+ (current package.json pins "stellar-sdk": "10.4.0")'
    );
  }
})();
// ────────────────────────────────────────────────────────────────────────────

const RPC_TIMEOUT_MS = 10_000;
const MAX_RETRIES = 3;
const RETRY_DELAY_MS = 1_500;

function withTimeout<T>(promise: Promise<T>, ms: number, label: string): Promise<T> {
  return Promise.race([
    promise,
    new Promise<never>((_, reject) =>
      setTimeout(() => reject(new Error(`${label} timed out after ${ms}ms`)), ms)
    ),
  ]);
}

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

  /**
   * Release escrowed funds to the mentor.
   *
   * Defense-in-depth: verifies `releasedBy` matches the escrow's registered
   * learnerId before invoking the contract, regardless of what the call-site
   * checked. This prevents bypasses from new code paths that forget the check.
   */
  async releaseFunds({ escrowId, releasedBy }: { escrowId: number; releasedBy: string }): Promise<string> {
    // ── Authorization guard ────────────────────────────────────────────────
    const record = await findEscrowRecord(escrowId);
    if (!record) {
      throw new Error(`Escrow ${escrowId} not found — cannot verify caller identity`);
    }
    if (record.learnerId !== releasedBy) {
      throw new Error(
        `Unauthorized: releasedBy "${releasedBy}" is not the learner for escrow ${escrowId}`
      );
    }
    // ── End authorization guard ────────────────────────────────────────────

    return withRetry(async (_signal) => {
      const sourceAccount = await withTimeout(
        this.server.getAccount(this.adminKeypair.publicKey()),
        RPC_TIMEOUT_MS,
        'getAccount'
      );

      const operation = this.contract.call(
        'release_funds',
        nativeToScVal(releasedBy, { type: 'address' }),
        nativeToScVal(escrowId, { type: 'u64' })
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

      const operation = this.contract.call(
        'refund',
        nativeToScVal(escrowId, { type: 'u64' })
      );

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
