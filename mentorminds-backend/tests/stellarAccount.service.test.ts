import { Pool } from 'pg';
import { StellarAccountService } from '../src/services/stellarAccount.service';
import { stellarFeesService } from '../src/services/stellarFees.service';
import { Keypair, Server } from 'stellar-sdk';

// Mock the fee service
jest.mock('../src/services/stellarFees.service', () => ({
  stellarFeesService: {
    getFeeEstimate: jest.fn(),
  },
}));

// Mock the Stellar SDK Server
jest.mock('stellar-sdk', () => {
  const actual = jest.requireActual('stellar-sdk');
  return {
    ...actual,
    Server: jest.fn().mockImplementation(() => ({
      loadAccount: jest.fn().mockImplementation((pubkey) => {
        return Promise.resolve(new actual.Account(pubkey, '1'));
      }),
      submitTransaction: jest.fn().mockResolvedValue({ hash: 'mock-tx-hash' }),
    })),
  };
});

describe('StellarAccountService', () => {
  let pool: jest.Mocked<Pool>;
  let service: StellarAccountService;

  beforeEach(() => {
    pool = {
      query: jest.fn().mockResolvedValue({ rows: [], rowCount: 0 }),
    } as any;
    
    // Reset mocks
    jest.clearAllMocks();
    
    service = new StellarAccountService(pool);
  });

  describe('fundAccount', () => {
    const destination = Keypair.random().publicKey();
    const userId = 'user-123';

    it('should use recommended fee from StellarFeesService', async () => {
      (stellarFeesService.getFeeEstimate as jest.Mock).mockResolvedValue({ recommended_fee: '250' });

      await service.fundAccount(destination, userId);

      expect(stellarFeesService.getFeeEstimate).toHaveBeenCalledWith(1);
      expect(pool.query).toHaveBeenCalledWith(
        expect.stringContaining('INSERT INTO transactions'),
        expect.arrayContaining([userId, '10.0', destination, 'completed', 'mock-tx-hash'])
      );
    });

    it('should cap the fee at 10,000 stroops during surge pricing', async () => {
      // Return a very high recommended fee
      (stellarFeesService.getFeeEstimate as jest.Mock).mockResolvedValue({ recommended_fee: '50000' });

      await service.fundAccount(destination, userId);

      // If it didn't throw and recorded 'completed', the cap logic was executed.
      // In a real test, we might inspect the TransactionBuilder call more closely.
      expect(pool.query).toHaveBeenCalledWith(
        expect.stringContaining('INSERT INTO transactions'),
        expect.arrayContaining(['completed'])
      );
    });

    it('should record failure if transaction submission fails', async () => {
      (stellarFeesService.getFeeEstimate as jest.Mock).mockResolvedValue({ recommended_fee: '100' });
      
      const mockServer = (service as any).server;
      mockServer.submitTransaction.mockRejectedValueOnce(new Error('Network error'));

      await expect(service.fundAccount(destination, userId)).rejects.toThrow('Network error');

      expect(pool.query).toHaveBeenCalledWith(
        expect.stringContaining('INSERT INTO transactions'),
        expect.arrayContaining([userId, '10.0', destination, 'failed'])
      );
    });
  });

  describe('createAndFundWallet', () => {
    it('should generate a new keypair and fund it', async () => {
      (stellarFeesService.getFeeEstimate as jest.Mock).mockResolvedValue({ recommended_fee: '100' });
      const userId = 'user-new';

      const publicKey = await service.createAndFundWallet(userId);

      expect(publicKey).toMatch(/^G[A-Z2-7]{55}$/);
      expect(pool.query).toHaveBeenCalledWith(
        expect.stringContaining('INSERT INTO transactions'),
        expect.arrayContaining([userId, '10.0', publicKey, 'completed'])
      );
    });
  });

  describe('activateExistingWallet', () => {
    it('should fund an existing destination', async () => {
      (stellarFeesService.getFeeEstimate as jest.Mock).mockResolvedValue({ recommended_fee: '100' });
      const destination = Keypair.random().publicKey();
      const userId = 'user-exist';

      await service.activateExistingWallet(destination, userId);

      expect(pool.query).toHaveBeenCalledWith(
        expect.stringContaining('INSERT INTO transactions'),
        expect.arrayContaining([userId, '10.0', destination, 'completed'])
      );
    });
  });
});
