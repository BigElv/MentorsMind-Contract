export interface EscrowReleaseRecord {
  id: string;
  learnerId: string;
  status: "active" | "released";
}

export interface EscrowReadRepository {
  findById(id: string): Promise<EscrowReleaseRecord | null>;
}

export interface EscrowWriteRepository {
  markReleased(id: string): Promise<void>;
}

export interface InternalReleasePolicy {
  isTrustedSystemCaller(callerId: string): boolean;
}

export class EscrowReleaseService {
  constructor(
    private readonly escrowReadRepository: EscrowReadRepository,
    private readonly escrowWriteRepository: EscrowWriteRepository,
    private readonly internalReleasePolicy: InternalReleasePolicy
  ) {}

  async releaseEscrow(
    escrowId: string,
    userId: string,
    options?: { bypassOwnerCheck?: boolean }
  ): Promise<void> {
    const escrow = await this.escrowReadRepository.findById(escrowId);
    if (!escrow) {
      throw new Error("Escrow not found");
    }

    const bypassRequested = options?.bypassOwnerCheck === true;
    if (bypassRequested) {
      if (!this.internalReleasePolicy.isTrustedSystemCaller(userId)) {
        throw new Error("Bypass owner check is internal-only");
      }
    } else if (escrow.learnerId !== userId) {
      throw new Error("Only the learner can release funds");
    }

    await this.escrowWriteRepository.markReleased(escrowId);
  }
}
