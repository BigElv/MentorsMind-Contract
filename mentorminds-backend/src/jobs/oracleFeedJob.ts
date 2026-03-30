import { OracleFeederService, oracleFeederService } from "../services/oracleFeeder";

export type OracleFeedJobOptions = {
  intervalMs?: number;
};

export class OracleFeedJob {
  private service: OracleFeederService;
  private intervalMs: number;
  private timer: NodeJS.Timeout | null = null;
  private running = false;

  constructor(service?: OracleFeederService, opts?: OracleFeedJobOptions) {
    this.service = service ?? oracleFeederService;
    this.intervalMs = opts?.intervalMs ?? 60000;
  }

  public start() {
    if (this.timer) return;
    this.timer = setInterval(() => {
      this.runOnce().catch((e) => {
        console.error("[OracleFeedJob] Run failed", e);
      });
    }, this.intervalMs);
    this.runOnce().catch((e) => {
      console.error("[OracleFeedJob] Initial run failed", e);
    });
  }

  public stop() {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }

  public async runOnce() {
    if (this.running) return;
    this.running = true;
    try {
      await this.service.submitAll();
    } finally {
      this.running = false;
    }
  }
}

export const oracleFeedJob = new OracleFeedJob();

