export interface EscrowRecord {
  id: string;
  mentorId: string;
  learnerId: string;
  amount: string;
  status: "pending" | "funded";
  createdAt: Date;
  stellarTxHash: string | null;
}

export interface EscrowRepository {
  create(input: Omit<EscrowRecord, "createdAt">): Promise<EscrowRecord>;
  deleteById(id: string): Promise<void>;
  markFunded(id: string, stellarTxHash: string): Promise<EscrowRecord>;
  findPendingOlderThan(cutoff: Date): Promise<EscrowRecord[]>;
  findByUserId(userId: string, role: 'mentor' | 'learner', limit: number, offset: number, status?: string): Promise<{escrows: EscrowRecord[], total: number}>;
}

export interface SorobanEscrowService {
  createEscrow(input: {
    escrowId: string;
    mentorId: string;
    learnerId: string;
    amount: string;
  }): Promise<{ txHash: string }>;
}

export class EscrowApiService {
  constructor(
    private readonly escrowRepository: EscrowRepository,
    private readonly sorobanEscrowService: SorobanEscrowService
  ) {}

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
    });

    try {
      const chainResult = await this.sorobanEscrowService.createEscrow({
        escrowId: created.id,
        mentorId: created.mentorId,
        learnerId: created.learnerId,
        amount: created.amount,
      });

      return this.escrowRepository.markFunded(created.id, chainResult.txHash);
    } catch (error) {
      await this.escrowRepository.deleteById(created.id);
      throw error;
    }
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
    options: { status?: string; role: 'mentor' | 'learner' },
    limit: number,
    offset: number
  ): Promise<{ escrows: EscrowRecord[]; total: number }> {
    return this.escrowRepository.findByUserId(userId, options.role, limit, offset, options.status);
  }
}
