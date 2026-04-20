import { OracleFeederService } from "../src/services/oracleFeeder";

describe("OracleFeederService price fetching and median", () => {
  const originalFetch = global.fetch as any;

  beforeEach(() => {
    jest.useFakeTimers().setSystemTime(new Date("2024-01-01T00:00:00Z"));
  });

  afterEach(() => {
    global.fetch = originalFetch;
    jest.useRealTimers();
    jest.resetAllMocks();
  });

  test("computes median across CoinGecko and Binance", async () => {
    const mockFetch = jest.fn(async (url: any) => {
      const u = String(url);
      if (u.includes("coingecko")) {
        if (u.includes("stellar")) {
          return { ok: true, status: 200, json: async () => ({ stellar: { usd: 0.12 } }) } as any;
        }
        if (u.includes("usd-coin")) {
          return { ok: true, status: 200, json: async () => ({ "usd-coin": { usd: 1.0 } }) } as any;
        }
        if (u.includes("mantle")) {
          return { ok: true, status: 200, json: async () => ({ mantle: { usd: 0.56 } }) } as any;
        }
      }
      if (u.includes("binance")) {
        if (u.includes("XLMUSDT")) {
          return { ok: true, status: 200, json: async () => ({ symbol: "XLMUSDT", price: "0.13" }) } as any;
        }
        if (u.includes("USDCUSDT")) {
          return { ok: true, status: 200, json: async () => ({ symbol: "USDCUSDT", price: "1.0" }) } as any;
        }
        if (u.includes("MNTUSDT")) {
          return { ok: true, status: 200, json: async () => ({ symbol: "MNTUSDT", price: "0.58" }) } as any;
        }
      }
      return { ok: false, status: 404, json: async () => ({}) } as any;
    });
    // @ts-ignore
    global.fetch = mockFetch;

    const svc = new OracleFeederService({
      logger: { info: jest.fn(), warn: jest.fn(), error: jest.fn() },
      submitFn: jest.fn(),
    });
    const med = await svc.fetchMedians();
    expect(med.XLM.median).toBeCloseTo(0.125, 6);
    expect(med.USDC.median).toBeCloseTo(1.0, 6);
    expect(med.MNT.median).toBeCloseTo(0.57, 6);
  });

  test("submits computed medians using injected submitter", async () => {
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

    const submitFn = jest.fn(async () => "tx123");
    const svc = new OracleFeederService({
      sources: ["coingecko"],
      logger: { info: jest.fn(), warn: jest.fn(), error: jest.fn() },
      submitFn,
    });
    const res = await svc.submitAll();
    expect(submitFn).toHaveBeenCalledTimes(3);
    expect(res.length).toBe(3);
    expect(res[0].txId).toBe("tx123");
  });
});

