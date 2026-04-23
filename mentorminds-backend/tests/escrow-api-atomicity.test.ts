import {
  EscrowApiService,
  EscrowRecord,
  EscrowRepository,
  SorobanEscrowService,
} from "../src/services/escrow-api.service";

class InMemoryEscrowRepository implements EscrowRepository {
  private readonly store = new Map<string, EscrowRecord>();

  async create(input: Omit<EscrowRecord, "createdAt">): Promise<EscrowRecord> {
    const record: EscrowRecord = { ...input, createdAt: new Date() };
    this.store.set(record.id, record);
    return record;
  }

  async deleteById(id: string): Promise<void> {
    this.store.delete(id);
  }

  async markFunded(id: string, stellarTxHash: string): Promise<EscrowRecord> {
    const record = this.store.get(id);
    if (!record) {
      throw new Error("Escrow not found");
    }

    const updated: EscrowRecord = {
      ...record,
      status: "funded",
      stellarTxHash,
    };
    this.store.set(id, updated);
    return updated;
  }

  async findPendingOlderThan(cutoff: Date): Promise<EscrowRecord[]> {
    return [...this.store.values()].filter(
      (record) =>
        record.status === "pending" &&
        record.stellarTxHash === null &&
        record.createdAt < cutoff
    );
  }

  getById(id: string): EscrowRecord | undefined {
    return this.store.get(id);
  }
}

describe("EscrowApiService.createEscrow atomicity", () => {
  it("rolls back the off-chain escrow if Soroban create fails", async () => {
    const repo = new InMemoryEscrowRepository();
    const soroban: SorobanEscrowService = {
      createEscrow: jest.fn().mockRejectedValue(new Error("soroban unavailable")),
    };

    const service = new EscrowApiService(repo, soroban);

    await expect(
      service.createEscrow({
        id: "esc-1",
        mentorId: "mentor-1",
        learnerId: "learner-1",
        amount: "1000",
      })
    ).rejects.toThrow("soroban unavailable");

    expect(repo.getById("esc-1")).toBeUndefined();
  });

  it("marks escrow funded when Soroban create succeeds", async () => {
    const repo = new InMemoryEscrowRepository();
    const soroban: SorobanEscrowService = {
      createEscrow: jest.fn().mockResolvedValue({ txHash: "tx_abc" }),
    };

    const service = new EscrowApiService(repo, soroban);

    const record = await service.createEscrow({
      id: "esc-2",
      mentorId: "mentor-1",
      learnerId: "learner-1",
      amount: "1000",
    });

    expect(record.status).toBe("funded");
    expect(record.stellarTxHash).toBe("tx_abc");
  });

  it("returns pending escrows with missing tx hash after the cutoff", async () => {
    const repo = new InMemoryEscrowRepository();
    const soroban: SorobanEscrowService = {
      createEscrow: jest.fn().mockResolvedValue({ txHash: "tx_any" }),
    };
    const service = new EscrowApiService(repo, soroban);

    const pending = await repo.create({
      id: "esc-3",
      mentorId: "mentor-1",
      learnerId: "learner-1",
      amount: "1000",
      status: "pending",
      stellarTxHash: null,
    });
    pending.createdAt = new Date("2026-01-01T00:00:00.000Z");

    const stale = await service.findUnreconciledEscrows(
      new Date("2026-01-01T00:11:00.000Z")
    );

    expect(stale).toHaveLength(1);
    expect(stale[0].id).toBe("esc-3");
  });
});
