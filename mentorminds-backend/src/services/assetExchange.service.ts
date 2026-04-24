/**
 * Asset Exchange Service
 * Manages periodic refresh of exchange rates for supported asset pairs.
 * Exposes start/stop controls for clean lifecycle management in tests and shutdown.
 */

import { AssetCode } from '../types/asset.types';
import { exchangeRateService } from './exchange-rate.service';

const REFRESH_INTERVAL_MS = 60_000; // 60 seconds

/** All asset pairs to keep warm in the cache. */
const TRACKED_PAIRS: Array<[AssetCode, AssetCode]> = [
  ['XLM', 'USDC'],
  ['XLM', 'PYUSD'],
  ['USDC', 'XLM'],
  ['PYUSD', 'XLM'],
];

let refreshHandle: ReturnType<typeof setInterval> | null = null;

async function refresh(): Promise<void> {
  try {
    await exchangeRateService.fetchMultipleRates(TRACKED_PAIRS);
  } catch (err) {
    console.error('[AssetExchangeService] Rate refresh failed:', err);
  }
}

/**
 * Start the periodic rate refresh loop.
 * Safe to call multiple times — subsequent calls are no-ops if already running.
 */
export async function startRateRefresh(): Promise<void> {
  if (refreshHandle !== null) return;

  // Eagerly populate cache before the first interval fires
  await refresh();

  refreshHandle = setInterval(refresh, REFRESH_INTERVAL_MS);
  console.log(`[AssetExchangeService] Rate refresh started (every ${REFRESH_INTERVAL_MS / 1000}s)`);
}

/**
 * Stop the periodic rate refresh loop and release the timer.
 * Safe to call even if the loop was never started.
 */
export function stopRateRefresh(): void {
  if (refreshHandle !== null) {
    clearInterval(refreshHandle);
    refreshHandle = null;
    console.log('[AssetExchangeService] Rate refresh stopped');
  }
}
