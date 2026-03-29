import { HORIZON_CONFIG } from '../config/horizon.config';

export interface NetworkStatus {
  activeEndpoint: string;
  isHealthy: boolean;
  lastLedgerSequence: number;
  lastLedgerTime: string;
  isStalled: boolean;
  isCongested: boolean;
  latencyMs: number;
  hasFailureSpike: boolean;
}

class NetworkMonitor {
  private status: NetworkStatus = {
    activeEndpoint: HORIZON_CONFIG.endpoints[0],
    isHealthy: true,
    lastLedgerSequence: 0,
    lastLedgerTime: new Date().toISOString(),
    isStalled: false,
    isCongested: false,
    latencyMs: 0,
    hasFailureSpike: false,
  };

  private errorCount = 0;
  private txFailureCount = 0;
  private readonly FAILOVER_THRESHOLD = 3;
  private readonly SPIKE_THRESHOLD = 50; // Alarms after 50 failures in interval

  constructor() {
    // Start monitoring only if not in a testing environment
    if (process.env.NODE_ENV !== 'test') {
      this.startMonitoring();
    }
  }

  private async checkHealth(endpoint: string): Promise<boolean> {
    const startTime = Date.now();
    try {
      const resp = await fetch(`${endpoint}/ledgers?limit=1&order=desc`);
      if (!resp.ok) return false;
      
      const data: any = await resp.json();
      if (!data._embedded?.records?.length) return false;
      
      const latestLedger = data._embedded.records[0];
      
      this.status.latencyMs = Date.now() - startTime;
      const ledgerTime = new Date(latestLedger.closed_at).getTime();
      const now = Date.now();

      // Detect stall: No new ledger in X seconds
      this.status.isStalled = (now - ledgerTime) > HORIZON_CONFIG.maxStallDurationMs;
      
      // Detect congestion: transaction_count vs max_tx_set_size
      const congestionRatio = latestLedger.transaction_count / latestLedger.max_tx_set_size;
      this.status.isCongested = congestionRatio > HORIZON_CONFIG.congestionCapacityThreshold;

      this.status.lastLedgerSequence = latestLedger.sequence;
      this.status.lastLedgerTime = latestLedger.closed_at;
      
      return true;
    } catch (err) {
      console.error(`Health check failed for ${endpoint}:`, err);
      return false;
    }
  }

  private async performCheck() {
    const healthy = await this.checkHealth(this.status.activeEndpoint);
    
    // Check for failure spikes (e.g. if 50+ transactions failed since last check)
    this.status.hasFailureSpike = this.txFailureCount > this.SPIKE_THRESHOLD;
    if (this.status.hasFailureSpike) {
      console.error(`ALERT: Transaction failure spike detected (${this.txFailureCount} failures)`);
    }
    // Reset failure count each interval
    this.txFailureCount = 0;

    if (!healthy) {
      this.errorCount++;
      if (this.errorCount >= this.FAILOVER_THRESHOLD) {
        this.attemptFailover();
      }
      this.status.isHealthy = false;
    } else {
      this.errorCount = 0;
      this.status.isHealthy = true;
    }
  }

  private attemptFailover() {
    const currentIndex = HORIZON_CONFIG.endpoints.indexOf(this.status.activeEndpoint);
    const nextIndex = (currentIndex + 1) % HORIZON_CONFIG.endpoints.length;
    this.status.activeEndpoint = HORIZON_CONFIG.endpoints[nextIndex];
    this.errorCount = 0;
    console.warn(`Failing over to Horizon node: ${this.status.activeEndpoint}`);
  }

  /**
   * Called by other services to record a transaction submission failure.
   */
  public recordTransactionFailure() {
    this.txFailureCount++;
  }

  public startMonitoring() {
    setInterval(() => this.performCheck(), HORIZON_CONFIG.checkIntervalMs);
    this.performCheck(); // initial check
  }

  public getStatus(): NetworkStatus {
    return { ...this.status };
  }
}

export const networkMonitorService = new NetworkMonitor();
