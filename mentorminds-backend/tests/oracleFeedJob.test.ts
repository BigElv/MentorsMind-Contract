import { OracleFeederService } from "../src/services/oracleFeeder";
import { OracleFeedJob } from "../src/jobs/oracleFeedJob";

describe("OracleFeedJob and failure alerts", () => {
  const originalFetch = global.fetch as any;

  beforeEach(() => {
    jest.useFakeTimers().setSystemTime(new Date("2024-01-01T00:00:00Z"));
  });

  afterEach(() => {
    global.fetch = originalFetch;
    jest.useRealTimers();
    jest.resetAllMocks();
  });

  test("alerts after 3 consecutive failures per asset", async () => {
    const mockFetch = jest.fn(async (url: any) => {
      const u = String(url);
      if (u.includes("coingecko")) {
        if (u.includes("stellar")) return { ok: true, status: 200, json: async () => ({ stellar: { usd: 0.12 } }) } as any;
        if (u.includes("usd-coin")) return { ok: true, status: 200, json: async () => ({ "usd-coin": { usd: 1.0 } }) } as any;
        if (u.includes("mantle")) return { ok: true, status: 200, json: async () => ({ mantle: { usd: 0.56 } }) } as any;
      }
      return { ok: false, status: 404, json: async () => ({}) } as any;
    });
    // @ts-ignore
    global.fetch = mockFetch;

    const alerts: string[] = [];
    const alertFn = jest.fn(async (m: string) => {
      alerts.push(m);
    });
    const svc = new OracleFeederService({
      sources: ["coingecko"],
      logger: { info: jest.fn(), warn: jest.fn(), error: jest.fn() },
      submitFn: jest.fn(async () => {
        throw new Error("submission_failed");
      }),
      alertFn,
    });
    const job = new OracleFeedJob(svc, { intervalMs: 1000 });
    await svc.submitAll();
    await svc.submitAll();
    await svc.submitAll();
    expect(alertFn).toHaveBeenCalled();
    expect(alerts.some((m) => m.includes("XLM"))).toBeTruthy();
    expect(alerts.some((m) => m.includes("USDC"))).toBeTruthy();
    expect(alerts.some((m) => m.includes("MNT"))).toBeTruthy();
    job.stop();
  });
});

