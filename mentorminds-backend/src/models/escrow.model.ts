import { Pool } from 'pg';

export type EscrowStatus = 'active' | 'released' | 'disputed' | 'refunded' | 'resolved';

export interface UpdateStatusOptions {
  status: EscrowStatus;
  additionalFields?: {
    stellar_tx_hash?: string;
    dispute_reason?: string;
    resolved_at?: Date;
    released_at?: Date;
  };
}

export class EscrowModel {
  constructor(private readonly pool: Pool) {}

  async updateStatus(escrowId: number, options: UpdateStatusOptions): Promise<void> {
    const { status, additionalFields = {} } = options;

    let paramIndex = 1;
    const fields: string[] = [];
    const values: unknown[] = [];

    fields.push(`status = $${paramIndex++}`);
    values.push(status);

    if (additionalFields.stellar_tx_hash !== undefined) {
      fields.push(`stellar_tx_hash = $${paramIndex++}`);
      values.push(additionalFields.stellar_tx_hash);
    }
    if (additionalFields.dispute_reason !== undefined) {
      fields.push(`dispute_reason = $${paramIndex++}`);
      values.push(additionalFields.dispute_reason);
    }
    if (additionalFields.resolved_at !== undefined) {
      fields.push(`resolved_at = $${paramIndex++}`);
      values.push(additionalFields.resolved_at);
    }
    if (additionalFields.released_at !== undefined) {
      fields.push(`released_at = $${paramIndex++}`);
      values.push(additionalFields.released_at);
    }

    values.push(escrowId);
    const sql = `UPDATE escrows SET ${fields.join(', ')} WHERE id = $${paramIndex}`;

    await this.pool.query(sql, values);
  }
}
