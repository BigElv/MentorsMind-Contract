import { 
  Address, 
  Contract, 
  rpc, 
  scValToNative, 
  xdr, 
  nativeToScVal 
} from '@stellar/stellar-sdk';
import { CreateEscrowParams, Escrow, EscrowStatus } from '../types/escrow.types';

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

  // Transaction building methods would be implemented here or using a transaction builder helper
  // For the purpose of this task, we define the structure
}
