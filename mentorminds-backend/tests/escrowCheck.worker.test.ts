import { Pool } from 'pg';
import { EscrowCheckWorker } from '../src/jobs/escrowCheck.worker';

describe('EscrowCheckWorker', () => {
  let pool: jest.Mocked<Pool>;
  let worker: EscrowCheckWorker;

  beforeEach(() => {
    pool = {
      query: jest.fn().mockResolvedValue({ rows: [], rowCount: 0 }),
    } as any;
    worker = new EscrowCheckWorker(pool);
  });

  describe('checkAutoRelease', () => {
    it('should use a parameterized query for the release window interval', async () => {
      await worker.checkAutoRelease();

      // Verify the query contains the placeholder and the parameter list contains 48
      expect(pool.query).toHaveBeenCalledWith(
        expect.stringContaining('($1 * INTERVAL \'1 hour\')'),
        [48]
      );
    });

    it('should not contain raw string interpolation for the interval', async () => {
      await worker.checkAutoRelease();
      
      const calls = (pool.query as jest.Mock).mock.calls;
      const sql = calls[0][0] as string;
      
      // Ensure the literal '48 hours' or equivalent is not in the SQL string
      expect(sql).not.toContain("'48 hours'");
      expect(sql).not.toContain('48'); 
    });

    it('should process all rows returned by the query', async () => {
      const mockRows = [
        { id: 1, mentor_id: 'm1', learner_id: 'l1', amount: '100' },
        { id: 2, mentor_id: 'm2', learner_id: 'l2', amount: '200' },
      ];
      (pool.query as jest.Mock).mockResolvedValueOnce({ rows: mockRows, rowCount: 2 });

      const consoleSpy = jest.spyOn(console, 'log').mockImplementation();

      await worker.checkAutoRelease();

      expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('Found 2 escrows'));
      expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('Processing auto-release for escrow 1'));
      expect(consoleSpy).toHaveBeenCalledWith(expect.stringContaining('Processing auto-release for escrow 2'));

      consoleSpy.mockRestore();
    });
  });
});
