import { Pool } from 'pg';
import { StellarTxWorker, StellarTxSubmitter } from '../src/jobs/stellarTx.worker';

function makePool(spy: jest.Mock): Pool {
  return { query: spy } as unknown as Pool;
}

describe('StellarTxWorker', () => {
  let querySpy: jest.Mock;
  let submitter: StellarTxSubmitter;
  let worker: StellarTxWorker;

  beforeEach(() => {
    querySpy = jest.fn().mockResolvedValue({ rowCount: 1 });
    submitter = { submit: jest.fn() };
    worker = new StellarTxWorker(makePool(querySpy), submitter);
  });

  it('success path — updates transactions table with completed status and hash', async () => {
    (submitter.submit as jest.Mock).mockResolvedValue({ hash: 'tx-abc' });

    await worker.process('pay-1', 'signed-xdr');

    const [sql, values] = querySpy.mock.calls[0];
    expect(sql).toContain('UPDATE transactions');
    expect(sql).toContain("status = 'completed'");
    expect(sql).toContain('transaction_hash = $1');
    expect(values).toEqual(['tx-abc', 'pay-1']);
  });

  it('failure path — updates transactions table with failed status', async () => {
    (submitter.submit as jest.Mock).mockRejectedValue(new Error('network error'));

    await expect(worker.process('pay-2', 'signed-xdr')).rejects.toThrow('network error');

    const [sql, values] = querySpy.mock.calls[0];
    expect(sql).toContain('UPDATE transactions');
    expect(sql).toContain("status = 'failed'");
    expect(values).toEqual(['pay-2']);
  });

  it('neither path ever references the payments table', async () => {
    (submitter.submit as jest.Mock).mockResolvedValue({ hash: 'tx-xyz' });
    await worker.process('pay-3', 'signed-xdr');
    const [successSql] = querySpy.mock.calls[0];
    expect(successSql).not.toMatch(/payments/);

    querySpy.mockClear();
    (submitter.submit as jest.Mock).mockRejectedValue(new Error('fail'));
    await expect(worker.process('pay-4', 'signed-xdr')).rejects.toThrow();
    const [failSql] = querySpy.mock.calls[0];
    expect(failSql).not.toMatch(/payments/);
  });
});
