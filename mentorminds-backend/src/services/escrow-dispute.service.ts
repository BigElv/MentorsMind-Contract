export interface SorobanDisputeService {
  resolveDispute(input: {
    escrowId: string;
    splitPercentage: number;
    resolvedBy: string;
  }): Promise<{ txHash: string }>;
}

export interface AdminIdentityResolver {
  toStellarPublicKey(adminUserId: string): Promise<string>;
}

export class EscrowDisputeService {
  constructor(
    private readonly sorobanDisputeService: SorobanDisputeService,
    private readonly adminIdentityResolver: AdminIdentityResolver
  ) {}

  async resolveDispute(input: {
    escrowId: string;
    splitPercentage: number;
    adminUserId: string;
  }): Promise<{ txHash: string }> {
    const adminPublicKey = await this.adminIdentityResolver.toStellarPublicKey(
      input.adminUserId
    );

    return this.sorobanDisputeService.resolveDispute({
      escrowId: input.escrowId,
      splitPercentage: input.splitPercentage,
      resolvedBy: adminPublicKey,
    });
  }
}
