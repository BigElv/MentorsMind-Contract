import { Pool, PoolClient, QueryResult } from 'pg';
import { EscrowModel } from '../src/models/escrow.model';

function makePool(spy: jest.Mock): Pool {
  return { query: spy } as unknown as Pool;
}

describe('EscrowModel.updateStatus', () => {
  let querySpy: jest.Mock;
  let model: EscrowModel;

  beforeEach(() => {
    querySpy = jest.fn().mockResolvedValue({ rowCount: 1 } as QueryResult);
    model = new EscrowModel(makePool(querySpy));
  });

  it('status only — produces valid $1 param for status and $2 for WHERE', async () => {
    await model.updateStatus(42, { status: 'released' });

    const [sql, values] = querySpy.mock.calls[0];
    expect(sql).toBe('UPDATE escrows SET status = $1 WHERE id = $2');
    expect(values).toEqual(['released', 42]);
  });

  it('stellar_tx_hash — correct $1/$2 for fields, $3 for WHERE', async () => {
    await model.updateStatus(1, { status: 'released', additionalFields: { stellar_tx_hash: 'abc123' } });

    const [sql, values] = querySpy.mock.calls[0];
    expect(sql).toBe('UPDATE escrows SET status = $1, stellar_tx_hash = $2 WHERE id = $3');
    expect(values).toEqual(['released', 'abc123', 1]);
  });

  it('dispute_reason — correct parameterization', async () => {
    await model.updateStatus(2, { status: 'disputed', additionalFields: { dispute_reason: 'no show' } });

    const [sql, values] = querySpy.mock.calls[0];
    expect(sql).toBe('UPDATE escrows SET status = $1, dispute_reason = $2 WHERE id = $3');
    expect(values).toEqual(['disputed', 'no show', 2]);
  });

  it('resolved_at — correct parameterization', async () => {
    const resolvedAt = new Date('2026-01-01T00:00:00Z');
    await model.updateStatus(3, { status: 'resolved', additionalFields: { resolved_at: resolvedAt } });

    const [sql, values] = querySpy.mock.calls[0];
    expect(sql).toBe('UPDATE escrows SET status = $1, resolved_at = $2 WHERE id = $3');
    expect(values).toEqual(['resolved', resolvedAt, 3]);
  });

  it('released_at — correct parameterization', async () => {
    const releasedAt = new Date('2026-02-01T00:00:00Z');
    await model.updateStatus(4, { status: 'released', additionalFields: { released_at: releasedAt } });

    const [sql, values] = querySpy.mock.calls[0];
    expect(sql).toBe('UPDATE escrows SET status = $1, released_at = $2 WHERE id = $3');
    expect(values).toEqual(['released', releasedAt, 4]);
  });

  it('all additionalFields — sequential params with no gaps', async () => {
    const resolvedAt = new Date('2026-03-01T00:00:00Z');
    const releasedAt = new Date('2026-03-02T00:00:00Z');

    await model.updateStatus(99, {
      status: 'resolved',
      additionalFields: {
        stellar_tx_hash: 'txhash',
        dispute_reason: 'fraud',
        resolved_at: resolvedAt,
        released_at: releasedAt,
      },
    });

    const [sql, values] = querySpy.mock.calls[0];
    expect(sql).toBe(
      'UPDATE escrows SET status = $1, stellar_tx_hash = $2, dispute_reason = $3, resolved_at = $4, released_at = $5 WHERE id = $6'
    );
    expect(values).toEqual(['resolved', 'txhash', 'fraud', resolvedAt, releasedAt, 99]);
  });

  it('no $ missing — no param placeholder is ever a bare number', async () => {
    await model.updateStatus(5, {
      status: 'disputed',
      additionalFields: { stellar_tx_hash: 'h', dispute_reason: 'r' },
    });

    const [sql] = querySpy.mock.calls[0];
    // Every assignment must use $N, never a bare digit
    const bareNumberAssignment = /= \d/;
    expect(sql).not.toMatch(bareNumberAssignment);
  });
});
