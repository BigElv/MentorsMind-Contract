export type EscrowStatus =
  | "pending"
  | "funded"
  | "released"
  | "disputed"
  | "refunded"
  | "resolved";

export interface EscrowRecord {
  id: string;
  mentorId: string;
  learnerId: string;
  amount: string;
  status: EscrowStatus;
  createdAt: Date;
  stellarTxHash: string | null;
  sorobanContractVersion: string | null;
}

export interface EscrowRepository {
  create(input: Omit<EscrowRecord, "createdAt">): Promise<EscrowRecord>;
  deleteById(id: string): Promise<void>;
  markFunded(
    id: string,
    stellarTxHash: string,
    sorobanContractVersion: string | null
  ): Promise<EscrowRecord>;
  findPendingOlderThan(cutoff: Date): Promise<EscrowRecord[]>;
  findByUserId(
    userId: string,
    role: "mentor" | "learner",
    limit: number,
    offset: number,
    status?: string
  ): Promise<{ escrows: EscrowRecord[]; total: number }>;
  findById(id: string): Promise<EscrowRecord | null>;
  updateStatus(id: string, status: EscrowStatus): Promise<EscrowRecord>;
}

export interface SorobanEscrowService {
  createEscrow(input: {
    escrowId: string;
    mentorId: string;
    learnerId: string;
    amount: string;
  }): Promise<{ txHash: string; contractVersion: string | null }>;
}

export class EscrowApiService {
  constructor(
    private readonly escrowRepository: EscrowRepository,
    private readonly sorobanEscrowService: SorobanEscrowService
  ) {}

  /**
   * Returns true when transitioning from currentStatus to newStatus is
   * a valid escrow state machine step.
   */
  static validateStateTransition(
    currentStatus: EscrowStatus,
    newStatus: EscrowStatus
  ): boolean {
    const validTransitions: Record<EscrowStatus, EscrowStatus[]> = {
      pending: ["funded"],
      funded: ["released", "disputed", "refunded"],
      disputed: ["resolved", "refunded"],
      released: [],
      refunded: [],
      resolved: [],
    };
    return validTransitions[currentStatus]?.includes(newStatus) ?? false;
  }

  async createEscrow(input: {
    id: string;
    mentorId: string;
    learnerId: string;
    amount: string;
  }): Promise<EscrowRecord> {
    const created = await this.escrowRepository.create({
      id: input.id,
      mentorId: input.mentorId,
      learnerId: input.learnerId,
      amount: input.amount,
      status: "pending",
      stellarTxHash: null,
      sorobanContractVersion: null,
    });

    try {
      const chainResult = await this.sorobanEscrowService.createEscrow({
        escrowId: created.id,
        mentorId: created.mentorId,
        learnerId: created.learnerId,
        amount: created.amount,
      });

      return this.escrowRepository.markFunded(
        created.id,
        chainResult.txHash,
        chainResult.contractVersion
      );
    } catch (error) {
      await this.escrowRepository.deleteById(created.id);
      throw error;
    }
  }

  async releaseEscrow(escrowId: string): Promise<EscrowRecord> {
    const escrow = await this.escrowRepository.findById(escrowId);
    if (!escrow) {
      throw new Error(`Escrow ${escrowId} not found`);
    }
    if (!EscrowApiService.validateStateTransition(escrow.status, "released")) {
      throw new Error(
        `Cannot release escrow in ${escrow.status} status`
      );
    }
    return this.escrowRepository.updateStatus(escrowId, "released");
  }

  async refundEscrow(escrowId: string): Promise<EscrowRecord> {
    const escrow = await this.escrowRepository.findById(escrowId);
    if (!escrow) {
      throw new Error(`Escrow ${escrowId} not found`);
    }
    if (!EscrowApiService.validateStateTransition(escrow.status, "refunded")) {
      throw new Error(
        `Cannot refund escrow in ${escrow.status} status`
      );
    }
    return this.escrowRepository.updateStatus(escrowId, "refunded");
  }

  async openDispute(escrowId: string): Promise<EscrowRecord> {
    const escrow = await this.escrowRepository.findById(escrowId);
    if (!escrow) {
      throw new Error(`Escrow ${escrowId} not found`);
    }
    if (!EscrowApiService.validateStateTransition(escrow.status, "disputed")) {
      throw new Error(
        `Cannot open dispute for escrow in ${escrow.status} status`
      );
    }
    return this.escrowRepository.updateStatus(escrowId, "disputed");
  }

  async resolveDispute(escrowId: string): Promise<EscrowRecord> {
    const escrow = await this.escrowRepository.findById(escrowId);
    if (!escrow) {
      throw new Error(`Escrow ${escrowId} not found`);
    }
    if (!EscrowApiService.validateStateTransition(escrow.status, "resolved")) {
      throw new Error(
        `Cannot resolve dispute for escrow in ${escrow.status} status`
      );
    }
    return this.escrowRepository.updateStatus(escrowId, "resolved");
  }

  async findUnreconciledEscrows(
    now: Date = new Date(),
    staleAfterMs: number = 10 * 60 * 1000
  ): Promise<EscrowRecord[]> {
    const cutoff = new Date(now.getTime() - staleAfterMs);
    return this.escrowRepository.findPendingOlderThan(cutoff);
  }

  async listUserEscrows(
    userId: string,
    options: { status?: string; role: "mentor" | "learner" },
    limit: number,
    offset: number
  ): Promise<{ escrows: EscrowRecord[]; total: number }> {
    return this.escrowRepository.findByUserId(
      userId,
      options.role,
      limit,
      offset,
      options.status
    );
  }
}
