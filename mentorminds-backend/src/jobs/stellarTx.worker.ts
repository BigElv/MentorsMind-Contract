import { Pool } from 'pg';

export interface StellarTxSubmitter {
  submit(signedXdr: string): Promise<{ hash: string }>;
}

export class StellarTxWorker {
  constructor(
    private readonly pool: Pool,
    private readonly submitter: StellarTxSubmitter
  ) {}

  async process(paymentId: string, signedXdr: string): Promise<void> {
    try {
      const result = await this.submitter.submit(signedXdr);
      await this.pool.query(
        "UPDATE transactions SET status = 'completed', transaction_hash = $1, updated_at = NOW() WHERE id = $2",
        [result.hash, paymentId]
      );
    } catch (err) {
      await this.pool.query(
        "UPDATE transactions SET status = 'failed', updated_at = NOW() WHERE id = $1",
        [paymentId]
      );
      throw err;
    }
  }
}
