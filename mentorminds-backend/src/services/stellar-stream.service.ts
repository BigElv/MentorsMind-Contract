import { startStellarMonitor } from "./stellar-monitor.service";

/**
 * Stellar payment confirmation path (not Horizon SSE).
 *
 * User wallet → user wallet payments are confirmed by transaction hash via
 * `POST /api/payments` plus `POST /api/payments/webhook`, with this poller as
 * a safety net for pending rows — not by opening one Horizon stream per user.
 */
export function startStellarPaymentMonitoring(): void {
  startStellarMonitor();
}
