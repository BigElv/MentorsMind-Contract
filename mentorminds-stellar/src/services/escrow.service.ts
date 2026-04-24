import { 
  Address, 
  Contract, 
  rpc, 
  scValToNative, 
  xdr, 
  nativeToScVal,
  TransactionBuilder,
  Networks,
  BASE_FEE,
} from '@stellar/stellar-sdk';
import { CreateEscrowParams, Escrow, EscrowStatus } from '../types/escrow.types';

export interface ReleaseFundsParams {
  escrowId: number;
  /** Stellar public key of the caller claiming to be the learner. */
  releasedBy: string;
  /** Keypair secret for signing the transaction. */
  signerSecret: string;
}

export class EscrowService {
  private contract: Contract;
  private server: rpc.Server;

  constructor(contractId: string, rpcUrl: string) {
    this.contract = new Contract(contractId);
    this.server = new rpc.Server(rpcUrl);
  }

  async getEscrow(escrowId: number): Promise<Escrow> {
    const response = await this.server.getContractData(
      this.contract.address(),
      xdr.ScVal.scvVec([
        nativeToScVal('ESCROW', { type: 'symbol' }),
        nativeToScVal(escrowId, { type: 'u64' })
      ]),
      rpc.Durability.Persistent,
    );

    if (!response || !response.val) {
      throw new Error('Escrow not found');
    }

    const contractDataVal = response.val.contractData().val();
    return this.parseEscrow(scValToNative(contractDataVal));
  }

  async getEscrowCount(): Promise<number> {
    const response = await this.server.getContractData(
      this.contract.address(),
      nativeToScVal('ESC_CNT', { type: 'symbol' }),
      rpc.Durability.Persistent,
    );

    return response && response.val
      ? Number(scValToNative(response.val.contractData().val()))
      : 0;
  }

  private parseEscrow(val: any): Escrow {
    return {
      id: Number(val.id),
      mentor: val.mentor,
      learner: val.learner,
      amount: BigInt(val.amount),
      sessionId: val.session_id,
      status: val.status as EscrowStatus,
      createdAt: Number(val.created_at),
      tokenAddress: val.token_address,
      platformFee: BigInt(val.platform_fee),
      netAmount: BigInt(val.net_amount),
      sessionEndTime: Number(val.session_end_time),
      autoReleaseDelay: Number(val.auto_release_delay),
      disputeReason: val.dispute_reason,
      resolvedAt: Number(val.resolved_at),
    };
  }

  /**
   * Release escrow funds to the mentor.
   *
   * Defense-in-depth: fetches the escrow record from the contract and verifies
   * that `releasedBy` matches the stored `learner` address before submitting
   * the on-chain transaction. This prevents bypasses when the method is called
   * from a new code path that omits the caller check.
   *
   * The Soroban contract enforces the same rule on-chain, so this is a
   * belt-and-suspenders guard at the service layer.
   */
  async releaseFunds({ escrowId, releasedBy, signerSecret }: ReleaseFundsParams): Promise<string> {
    // --- Defense-in-depth: verify caller is the escrow learner ---
    const escrow = await this.getEscrow(escrowId);

    if (escrow.learner !== releasedBy) {
      throw new Error(
        `Unauthorized: releasedBy (${releasedBy}) does not match escrow learner (${escrow.learner})`
      );
    }
    // --- End guard ---

    const { Keypair } = await import('@stellar/stellar-sdk');
    const signer = Keypair.fromSecret(signerSecret);
    const sourceAccount = await this.server.getAccount(signer.publicKey());

    const operation = this.contract.call(
      'release_funds',
      new Address(releasedBy).toScVal(),
      nativeToScVal(escrowId, { type: 'u64' }),
    );

    const tx = new TransactionBuilder(sourceAccount, {
      fee: BASE_FEE,
      networkPassphrase: Networks.TESTNET,
    })
      .addOperation(operation)
      .setTimeout(30)
      .build();

    const prepared = await this.server.prepareTransaction(tx);
    prepared.sign(signer);

    const response = await this.server.sendTransaction(prepared);
    if (response.status !== 'PENDING') {
      throw new Error(`Transaction failed with status: ${response.status}`);
    }

    return response.hash;
  }

  // Transaction building methods would be implemented here or using a transaction builder helper
  // For the purpose of this task, we define the structure
}
