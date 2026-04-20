import { horizonConfig } from '../config/horizon.config';

export type HorizonEndpoint = 'primary' | 'backup';

export interface NetworkStatus {
  status: 'ok' | 'degraded' | 'down';
  activeHorizon: HorizonEndpoint;
  primary: string;
  backup: string;
  ledger?: number;
  latencyMs?: number;
  lastUpdate: string;
  errors: string[];
}

let statusState: NetworkStatus = {
  status: 'down',
  activeHorizon: 'primary',
  primary: horizonConfig.primary,
  backup: horizonConfig.backup,
  lastUpdate: new Date().toISOString(),
  errors: ['Monitor starting'],
};

async function fetchLatestLedger(url: string): Promise<{ ledger?: number; latencyMs: number }> {
  const start = Date.now();
  const res = await fetch(`${url}/ledgers?limit=1`, { timeout: horizonConfig.healthyResponseTimeoutMs } as any);
  const latencyMs = Date.now() - start;
  if (!res.ok) throw new Error(`Horizon ${url} returned ${res.status}`);

  const body = await res.json();
  const latestLedger = body._embedded?.records?.[0]?.sequence;
  return { ledger: latestLedger, latencyMs };
}

async function evaluateNetwork() {
  const errors: string[] = [];
  let active: HorizonEndpoint = 'primary';
  let ledger: number | undefined;
  let latencyMs: number | undefined;

  try {
    const p = await fetchLatestLedger(horizonConfig.primary);
    ledger = p.ledger;
    latencyMs = p.latencyMs;
    active = 'primary';
    statusState = { ...statusState, status: 'ok', errors: [], activeHorizon: active, ledger, latencyMs, lastUpdate: new Date().toISOString() };
    return;
  } catch (err: any) {
    errors.push(`Primary failed: ${err?.message}`);
  }

  try {
    const b = await fetchLatestLedger(horizonConfig.backup);
    ledger = b.ledger;
    latencyMs = b.latencyMs;
    active = 'backup';
    statusState = { ...statusState, status: 'degraded', errors, activeHorizon: active, ledger, latencyMs, lastUpdate: new Date().toISOString() };
    return;
  } catch (err: any) {
    errors.push(`Backup failed: ${err?.message}`);
  }

  statusState = { ...statusState, status: 'down', errors, activeHorizon: active, ledger, latencyMs, lastUpdate: new Date().toISOString() };
}

export function getNetworkStatus(): NetworkStatus {
  return statusState;
}

export async function startNetworkMonitor(): Promise<void> {
  await evaluateNetwork();
  setInterval(evaluateNetwork, 15_000);
  console.log('Network monitor started with primary', horizonConfig.primary, 'backup', horizonConfig.backup);
}
