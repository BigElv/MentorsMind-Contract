import {
  AdminIdentityResolver,
  EscrowDisputeService,
  SorobanDisputeService,
} from "../src/services/escrow-dispute.service";

describe("EscrowDisputeService", () => {
  it("threads adminUserId through resolver and sends admin public key to Soroban", async () => {
    const sorobanDisputeService: SorobanDisputeService = {
      resolveDispute: jest.fn().mockResolvedValue({ txHash: "tx_resolve" }),
    };
    const adminIdentityResolver: AdminIdentityResolver = {
      toStellarPublicKey: jest
        .fn()
        .mockResolvedValue("GADMINPUBLICKEY123456789"),
    };

    const service = new EscrowDisputeService(
      sorobanDisputeService,
      adminIdentityResolver
    );

    const result = await service.resolveDispute({
      escrowId: "escrow-1",
      splitPercentage: 60,
      adminUserId: "admin-user-42",
    });

    expect(adminIdentityResolver.toStellarPublicKey).toHaveBeenCalledWith(
      "admin-user-42"
    );
    expect(sorobanDisputeService.resolveDispute).toHaveBeenCalledWith({
      escrowId: "escrow-1",
      splitPercentage: 60,
      resolvedBy: "GADMINPUBLICKEY123456789",
    });
    expect(result.txHash).toBe("tx_resolve");
  });
});
