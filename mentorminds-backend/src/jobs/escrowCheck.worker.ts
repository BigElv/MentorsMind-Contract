import { Pool } from 'pg';

const RELEASE_WINDOW_HOURS = 48;

export class EscrowCheckWorker {
  constructor(private readonly pool: Pool) {}

  /**
   * Checks for escrows that have exceeded the release window and triggers auto-release.
   * Uses a parameterized query to safely handle the interval calculation.
   */
  async checkAutoRelease(): Promise<void> {
    // FIX: Using ($1 * INTERVAL '1 hour') instead of raw string interpolation
    // to prevent potential SQL injection and follow best practices.
    const sql = `
      SELECT id, mentor_id, learner_id, amount
      FROM escrows
      WHERE status = 'active'
      AND created_at < NOW() - ($1 * INTERVAL '1 hour')
    `;

    try {
      const result = await this.pool.query(sql, [RELEASE_WINDOW_HOURS]);
      
      if (result.rows.length === 0) {
        return;
      }

      console.log(`[EscrowCheckWorker] Found ${result.rows.length} escrows for auto-release.`);

      for (const row of result.rows) {
        // NOTE: In a full implementation, this would trigger the actual 
        // Soroban contract call and update the database record status.
        await this.processAutoRelease(row);
      }
    } catch (error) {
      console.error('[EscrowCheckWorker] Error checking auto-release:', error);
      throw error;
    }
  }

  private async processAutoRelease(escrow: any): Promise<void> {
    // Placeholder for actual release logic
    console.log(`[EscrowCheckWorker] Processing auto-release for escrow ${escrow.id}`);
    
    // Example database update after successful on-chain release:
    // await this.pool.query("UPDATE escrows SET status = 'released' WHERE id = $1", [escrow.id]);
  }
}
