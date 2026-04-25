import {
  EscrowApiService,
  EscrowRecord,
  EscrowRepository,
  EscrowStatus,
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

  async markFunded(
    id: string,
    stellarTxHash: string,
    sorobanContractVersion: string | null
  ): Promise<EscrowRecord> {
    const record = this.store.get(id);
    if (!record) {
      throw new Error("Escrow not found");
    }

    const updated: EscrowRecord = {
      ...record,
      status: "funded",
      stellarTxHash,
      sorobanContractVersion,
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

  async findByUserId(
    userId: string,
    role: "mentor" | "learner",
    limit: number,
    offset: number,
    status?: string
  ): Promise<{ escrows: EscrowRecord[]; total: number }> {
    const userField = role === "mentor" ? "mentorId" : "learnerId";
    let filtered = [...this.store.values()].filter(
      (record) => record[userField] === userId
    );
    if (status) {
      filtered = filtered.filter((record) => record.status === status);
    }
    const total = filtered.length;
    const escrows = filtered.slice(offset, offset + limit);
    return { escrows, total };
  }

  async findById(id: string): Promise<EscrowRecord | null> {
    return this.store.get(id) ?? null;
  }

  async updateStatus(id: string, status: EscrowStatus): Promise<EscrowRecord> {
    const record = this.store.get(id);
    if (!record) {
      throw new Error("Escrow not found");
    }
    const updated: EscrowRecord = { ...record, status };
    this.store.set(id, updated);
    return updated;
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
      createEscrow: jest
        .fn()
        .mockResolvedValue({ txHash: "tx_abc", contractVersion: "v1.2.0" }),
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
    expect(record.sorobanContractVersion).toBe("v1.2.0");
  });

  it("returns pending escrows with missing tx hash after the cutoff", async () => {
    const repo = new InMemoryEscrowRepository();
    const soroban: SorobanEscrowService = {
      createEscrow: jest
        .fn()
        .mockResolvedValue({ txHash: "tx_any", contractVersion: "v1.2.0" }),
    };
    const service = new EscrowApiService(repo, soroban);

    const pending = await repo.create({
      id: "esc-3",
      mentorId: "mentor-1",
      learnerId: "learner-1",
      amount: "1000",
      status: "pending",
      stellarTxHash: null,
      sorobanContractVersion: null,
    });
    pending.createdAt = new Date("2026-01-01T00:00:00.000Z");

    const stale = await service.findUnreconciledEscrows(
      new Date("2026-01-01T00:11:00.000Z")
    );

    expect(stale).toHaveLength(1);
    expect(stale[0].id).toBe("esc-3");
  });

  it("listUserEscrows filters by user and role with status", async () => {
    const repo = new InMemoryEscrowRepository();
    const soroban: SorobanEscrowService = {
      createEscrow: jest
        .fn()
        .mockResolvedValue({ txHash: "tx_any", contractVersion: "v1.2.0" }),
    };
    const service = new EscrowApiService(repo, soroban);

    // Create some escrows
    await repo.create({
      id: "esc-1",
      mentorId: "mentor-1",
      learnerId: "learner-1",
      amount: "1000",
      status: "pending",
      stellarTxHash: null,
      sorobanContractVersion: null,
    });
    await repo.create({
      id: "esc-2",
      mentorId: "mentor-1",
      learnerId: "learner-2",
      amount: "2000",
      status: "funded",
      stellarTxHash: "tx_123",
      sorobanContractVersion: "v1.2.0",
    });
    await repo.create({
      id: "esc-3",
      mentorId: "mentor-2",
      learnerId: "learner-1",
      amount: "1500",
      status: "pending",
      stellarTxHash: null,
      sorobanContractVersion: null,
    });

    // List for mentor-1 as mentor, no status filter
    const result1 = await service.listUserEscrows("mentor-1", { role: "mentor" }, 10, 0);
    expect(result1.total).toBe(2);
    expect(result1.escrows).toHaveLength(2);
    expect(result1.escrows.map(e => e.id)).toEqual(["esc-1", "esc-2"]);

    // List for mentor-1 as mentor, status pending
    const result2 = await service.listUserEscrows("mentor-1", { role: "mentor", status: "pending" }, 10, 0);
    expect(result2.total).toBe(1);
    expect(result2.escrows).toHaveLength(1);
    expect(result2.escrows[0].id).toBe("esc-1");

    // List for learner-1 as learner
    const result3 = await service.listUserEscrows("learner-1", { role: "learner" }, 10, 0);
    expect(result3.total).toBe(2);
    expect(result3.escrows).toHaveLength(2);
    expect(result3.escrows.map(e => e.id)).toEqual(["esc-1", "esc-3"]);
  });
});
