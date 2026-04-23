import dotenv from "dotenv";

dotenv.config();

type AssetSymbol = "XLM" | "USDC" | "MNT";
type PriceSource = "coingecko" | "binance";

export type PricePoint = {
  asset: AssetSymbol;
  source: PriceSource;
  price: number;
  timestamp: number;
};

export type SubmitResult = {
  asset: AssetSymbol;
  price: number;
  timestamp: number;
  txId?: string;
};

export type OracleFeederOptions = {
  sources?: PriceSource[];
  assets?: AssetSymbol[];
  backoff?: {
    retries: number;
    baseDelayMs: number;
    maxDelayMs: number;
  };
  logger?: {
    info: (...args: any[]) => void;
    warn: (...args: any[]) => void;
    error: (...args: any[]) => void;
  };
  submitFn?: (asset: AssetSymbol, price: number, timestamp: number) => Promise<string | void>;
  alertFn?: (message: string, context?: Record<string, any>) => Promise<void>;
};

const DEFAULT_SOURCES: PriceSource[] = ["coingecko", "binance"];
const DEFAULT_ASSETS: AssetSymbol[] = ["XLM", "USDC", "MNT"];

function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function median(values: number[]): number {
  if (values.length === 0) throw new Error("median requires at least one value");
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  if (sorted.length % 2 === 1) return sorted[mid];
  return (sorted[mid - 1] + sorted[mid]) / 2;
}

async function httpGetJsonWithBackoff(url: string, retries: number, baseDelayMs: number, maxDelayMs: number): Promise<any> {
  let attempt = 0;
  for (;;) {
    try {
      const res = await fetch(url, { headers: { "User-Agent": "mentorminds-oracle-feeder/1.0" } });
      if (res.status === 429) throw new Error("rate_limited");
      if (!res.ok) throw new Error(`http_${res.status}`);
      return await res.json();
    } catch (e: any) {
      attempt += 1;
      if (attempt > retries) throw e;
      const delay = Math.min(maxDelayMs, Math.floor(baseDelayMs * Math.pow(2, attempt - 1) + Math.random() * 250));
      await sleep(delay);
    }
  }
}

function nowSeconds(): number {
  return Math.floor(Date.now() / 1000);
}

function env(name: string): string | undefined {
  const v = process.env[name];
  if (!v || v.trim() === "") return undefined;
  return v.trim();
}

async function defaultAlert(message: string, context?: Record<string, any>) {
  const url = env("ALERT_WEBHOOK_URL");
  if (!url) {
    console.error("[OracleFeeder][ALERT]", message, context || {});
    return;
  }
  try {
    await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ text: message, context }),
    });
  } catch {
    console.error("[OracleFeeder][ALERT]", message, context || {});
  }
}

async function defaultSubmit(asset: AssetSymbol, price: number, timestamp: number): Promise<string | void> {
  const contractId = env("ORACLE_CONTRACT_ID");
  const feederSecret = env("ORACLE_FEEDER_SECRET");
  const rpcUrl = env("SOROBAN_RPC_URL");
  const networkPassphrase = env("SOROBAN_NETWORK_PASSPHRASE");
  if (!contractId || !feederSecret || !rpcUrl || !networkPassphrase) {
    console.log("[OracleFeeder] Submission is disabled due to missing configuration");
    return;
  }
  try {
    // Lazy import to avoid build-time coupling
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const StellarSdk = require("stellar-sdk");
    const { Keypair, xdr } = StellarSdk;

    // Verify Soroban capability — fail loudly rather than passing raw values
    if (typeof StellarSdk.nativeToScVal !== 'function') {
      throw new Error(
        'stellar-sdk version does not support nativeToScVal — upgrade to v10.4+ ' +
        '(current package.json pins "stellar-sdk": "10.4.0")'
      );
    }
    const SorobanRpc = StellarSdk.rpc;
    if (typeof SorobanRpc?.Server !== 'function') {
      throw new Error(
        'stellar-sdk version does not expose rpc.Server — Soroban RPC support ' +
        'requires stellar-sdk v10.4+ (current package.json pins "stellar-sdk": "10.4.0")'
      );
    }

    const kp = Keypair.fromSecret(feederSecret);
    const server = new SorobanRpc.Server(rpcUrl, { allowHttp: rpcUrl.startsWith("http://"), timeout: RPC_TIMEOUT_MS });
    const contract = new StellarSdk.Contract(contractId);
    const source = await server.getAccount(kp.publicKey());
    const tx = new StellarSdk.TransactionBuilder(source, {
      fee: "1000",
      networkPassphrase,
    })
      .addOperation(
        contract.call("submit_price", StellarSdk.nativeToScVal(kp.publicKey(), { type: "address" }), xdr.ScSymbol.fromString(asset), StellarSdk.nativeToScVal(BigInt(Math.round(price * 1e8))), StellarSdk.nativeToScVal(BigInt(timestamp)))
      )
      .setTimeout(60)
      .build();
    tx.sign(kp);
    const send = await server.sendTransaction(tx);
    if (send.status === "PENDING" || send.status === "SUCCESS") {
      return send.hash;
    }
    if (send.errorResultXdr) {
      throw new Error("tx_failed");
    }
    return send.hash;
  } catch (e) {
    console.error("[OracleFeeder] Failed to submit transaction", e);
    throw e;
  }
}

export class OracleFeederService {
  private options: Required<OracleFeederOptions>;
  private failures: Record<AssetSymbol, number> = { XLM: 0, USDC: 0, MNT: 0 };

  constructor(opts?: OracleFeederOptions) {
    this.options = {
      sources: opts?.sources ?? DEFAULT_SOURCES,
      assets: opts?.assets ?? DEFAULT_ASSETS,
      backoff: opts?.backoff ?? { retries: 5, baseDelayMs: 500, maxDelayMs: 8000 },
      logger: opts?.logger ?? console,
      submitFn: opts?.submitFn ?? defaultSubmit,
      alertFn: opts?.alertFn ?? defaultAlert,
    };
  }

  public async fetchPrices(): Promise<PricePoint[]> {
    const tasks: Promise<PricePoint | null>[] = [];
    for (const asset of this.options.assets) {
      for (const source of this.options.sources) {
        tasks.push(this.fetchFromSource(asset, source).catch(() => null));
      }
    }
    const results = await Promise.all(tasks);
    return results.filter((r): r is PricePoint => r !== null);
  }

  private async fetchFromSource(asset: AssetSymbol, source: PriceSource): Promise<PricePoint> {
    if (source === "coingecko") {
      const ids: Record<AssetSymbol, string> = {
        XLM: env("COINGECKO_ID_XLM") || "stellar",
        USDC: env("COINGECKO_ID_USDC") || "usd-coin",
        MNT: env("COINGECKO_ID_MNT") || "mantle",
      };
      const id = ids[asset];
      const url = `https://api.coingecko.com/api/v3/simple/price?ids=${encodeURIComponent(id)}&vs_currencies=usd`;
      const data = await httpGetJsonWithBackoff(url, this.options.backoff.retries, this.options.backoff.baseDelayMs, this.options.backoff.maxDelayMs);
      const price = data?.[id]?.usd;
      if (typeof price !== "number") throw new Error("invalid_response");
      return { asset, source, price: price, timestamp: nowSeconds() };
    }
    if (source === "binance") {
      const symbols: Record<AssetSymbol, string> = {
        XLM: env("BINANCE_SYMBOL_XLM") || "XLMUSDT",
        USDC: env("BINANCE_SYMBOL_USDC") || "USDCUSDT",
        MNT: env("BINANCE_SYMBOL_MNT") || "MNTUSDT",
      };
      const s = symbols[asset];
      const url = `https://api.binance.com/api/v3/ticker/price?symbol=${encodeURIComponent(s)}`;
      const data = await httpGetJsonWithBackoff(url, this.options.backoff.retries, this.options.backoff.baseDelayMs, this.options.backoff.maxDelayMs);
      const p = parseFloat(data?.price ?? data?.priceFloat ?? data?.lastPrice);
      if (!isFinite(p)) throw new Error("invalid_response");
      return { asset, source, price: p, timestamp: nowSeconds() };
    }
    throw new Error("unsupported_source");
  }

  public async fetchMedians(): Promise<Record<AssetSymbol, { median: number; points: PricePoint[] }>> {
    const points = await this.fetchPrices();
    const grouped: Record<AssetSymbol, PricePoint[]> = { XLM: [], USDC: [], MNT: [] };
    for (const p of points) grouped[p.asset].push(p);
    const out: Record<AssetSymbol, { median: number; points: PricePoint[] }> = { XLM: { median: NaN, points: [] }, USDC: { median: NaN, points: [] }, MNT: { median: NaN, points: [] } };
    for (const asset of this.options.assets) {
      const ps = grouped[asset].map((p) => p.price);
      if (ps.length === 0) continue;
      const m = median(ps);
      out[asset] = { median: m, points: grouped[asset] };
    }
    return out;
  }

  public async submitAll(): Promise<SubmitResult[]> {
    const medians = await this.fetchMedians();
    const results: SubmitResult[] = [];
    for (const asset of this.options.assets) {
      const entry = medians[asset];
      if (!entry || !isFinite(entry.median)) {
        this.options.logger.warn("[OracleFeeder] No median price for", asset);
        continue;
      }
      const ts = nowSeconds();
      try {
        const txId = await this.options.submitFn(asset, entry.median, ts);
        this.failures[asset] = 0;
        this.options.logger.info("[OracleFeeder] Submitted", { asset, price: entry.median, timestamp: ts, txId, sources: entry.points.map((p) => p.source) });
        results.push({ asset, price: entry.median, timestamp: ts, txId: typeof txId === "string" ? txId : undefined });
      } catch (e: any) {
        this.failures[asset] = (this.failures[asset] || 0) + 1;
        this.options.logger.error("[OracleFeeder] Submission failed", { asset, error: e?.message || String(e), failures: this.failures[asset] });
        if (this.failures[asset] >= 3) {
          await this.options.alertFn(`Price submission failed ${this.failures[asset]} times for ${asset}`, { asset, failures: this.failures[asset] });
        }
      }
    }
    return results;
  }
}

export const oracleFeederService = new OracleFeederService();

