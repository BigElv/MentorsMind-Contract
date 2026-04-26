import {
  Contract,
  Keypair,
  Networks,
  nativeToScVal,
  rpc,
  TransactionBuilder,
} from '@stellar/stellar-sdk';
import { EscrowService } from './escrow.service';

export interface ReleaseFundsParams {
  escrowId: number;
  /** The caller's address — must match the escrow's learner field. */
  releasedBy: string;
}

/**
 * SorobanEscrowService wraps on-chain escrow interactions that require
 * a signed transaction (e.g. release_funds).
 *
 * Defense-in-depth: releaseFunds always fetches the escrow record from the
 * chain and verifies that `releasedBy` is the escrow's learner before
 * submitting the transaction.  This prevents bypasses when the method is
 * called from a new code path that omits the caller-identity check.
 */
export class SorobanEscrowService {
  private contract: Contract;
  private server: rpc.Server;
  private escrowService: EscrowService;
  private signerKeypair: Keypair;
  private networkPassphrase: string;

  constructor(
    contractId: string,
    rpcUrl: string,
    signerSecret: string,
    networkPassphrase: string = Networks.TESTNET,
  ) {
    this.contract = new Contract(contractId);
    this.server = new rpc.Server(rpcUrl);
    this.escrowService = new EscrowService(contractId, rpcUrl);
    this.signerKeypair = Keypair.fromSecret(signerSecret);
    this.networkPassphrase = networkPassphrase;
  }

  /**
   * Returns true when the service has been configured with a contract ID and
   * RPC URL (mirrors the pattern used by callers such as BookingsService).
   */
  static isConfigured(): boolean {
    return !!(process.env.ESCROW_CONTRACT_ID && process.env.SOROBAN_RPC_URL);
  }

  /**
   * Release escrow funds to the mentor.
   *
   * Before invoking the contract this method fetches the escrow record from
   * the chain and asserts that `releasedBy` matches `escrow.learner`.
   * This is the primary authorization guard — callers must not skip it.
   *
   * @throws {Error} if the escrow is not found, or if `releasedBy` is not
   *   the escrow's learner.
   */
  async releaseFunds({ escrowId, releasedBy }: ReleaseFundsParams): Promise<string> {
    // --- Defense-in-depth: verify caller is the escrow's learner ---
    const escrow = await this.escrowService.getEscrow(escrowId);

    if (escrow.learner !== releasedBy) {
      throw new Error(
        `Unauthorized: releasedBy (${releasedBy}) does not match escrow learner (${escrow.learner})`,
      );
    }
    // ----------------------------------------------------------------

    const sourceAccount = await this.server.getAccount(this.signerKeypair.publicKey());

    const operation = this.contract.call(
      'release_funds',
      nativeToScVal(escrowId, { type: 'u64' }),
      nativeToScVal(releasedBy, { type: 'address' }),
    );

    const transaction = new TransactionBuilder(sourceAccount, {
      fee: '1000',
      networkPassphrase: this.networkPassphrase,
    })
      .addOperation(operation)
      .setTimeout(30)
      .build();

    transaction.sign(this.signerKeypair);

    const sendResponse = await this.server.sendTransaction(transaction);
    if (sendResponse.status !== 'PENDING') {
      throw new Error(`Failed to send release_funds transaction: ${sendResponse.status}`);
    }

    return sendResponse.hash;
  }
}
