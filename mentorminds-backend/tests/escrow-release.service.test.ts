import {
  EscrowReadRepository,
  EscrowReleaseRecord,
  EscrowReleaseService,
  EscrowWriteRepository,
  InternalReleasePolicy,
} from "../src/services/escrow-release.service";

class InMemoryEscrowRepo implements EscrowReadRepository, EscrowWriteRepository {
  private readonly store = new Map<string, EscrowReleaseRecord>();

  constructor(records: EscrowReleaseRecord[]) {
    for (const record of records) {
      this.store.set(record.id, record);
    }
  }

  async findById(id: string): Promise<EscrowReleaseRecord | null> {
    return this.store.get(id) ?? null;
  }

  async markReleased(id: string): Promise<void> {
    const record = this.store.get(id);
    if (!record) {
      throw new Error("Escrow not found");
    }
    this.store.set(id, { ...record, status: "released" });
  }

  getById(id: string): EscrowReleaseRecord | undefined {
    return this.store.get(id);
  }
}

describe("EscrowReleaseService", () => {
  it("keeps learner-only guard for user-facing releases", async () => {
    const repo = new InMemoryEscrowRepo([
      { id: "esc-1", learnerId: "learner-1", status: "active" },
    ]);
    const policy: InternalReleasePolicy = {
      isTrustedSystemCaller: () => false,
    };

    const service = new EscrowReleaseService(repo, repo, policy);

    await expect(service.releaseEscrow("esc-1", "not-learner")).rejects.toThrow(
      "Only the learner can release funds"
    );
  });

  it("allows trusted system caller to bypass learner check", async () => {
    const repo = new InMemoryEscrowRepo([
      { id: "esc-2", learnerId: "learner-1", status: "active" },
    ]);
    const policy: InternalReleasePolicy = {
      isTrustedSystemCaller: (callerId: string) => callerId === "system",
    };

    const service = new EscrowReleaseService(repo, repo, policy);

    await service.releaseEscrow("esc-2", "system", { bypassOwnerCheck: true });

    expect(repo.getById("esc-2")?.status).toBe("released");
  });

  it("blocks bypass when caller is not trusted", async () => {
    const repo = new InMemoryEscrowRepo([
      { id: "esc-3", learnerId: "learner-1", status: "active" },
    ]);
    const policy: InternalReleasePolicy = {
      isTrustedSystemCaller: () => false,
    };

    const service = new EscrowReleaseService(repo, repo, policy);

    await expect(
      service.releaseEscrow("esc-3", "system", { bypassOwnerCheck: true })
    ).rejects.toThrow("Bypass owner check is internal-only");
  });
});
