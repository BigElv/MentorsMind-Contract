import { Keypair, TransactionBuilder, Account, Operation, Networks } from 'stellar-sdk';
import { SEP10AuthService } from '../src/services/sep10';
import { SEP24Service } from '../src/services/sep24';
import axios from 'axios';

jest.mock('axios');
const mockedAxios = axios as jest.Mocked<typeof axios>;

describe('Stellar Anchor Integration (SEP-10 & SEP-24)', () => {
  const clientKeypair = Keypair.random();
  const anchorAuthEndpoint = 'https://auth.anchor.com';
  const transferServerUrl = 'https://api.anchor.com/sep24';

  beforeEach(() => {
    jest.clearAllMocks();
  });

  test('SEP-10: should complete the auth flow', async () => {
    // A valid minimal challenge transaction XDR (empty but valid for structure)
    // Actually, generating one with StellarSDK is better to avoid RangeError
    const sourceAccount = new Account(clientKeypair.publicKey(), '0');
    const challengeTx = new TransactionBuilder(sourceAccount, { fee: '100', networkPassphrase: Networks.TESTNET })
      .addOperation(Operation.manageData({ name: 'auth', value: 'value' }))
      .setTimeout(0)
      .build();

    mockedAxios.get.mockResolvedValueOnce({
      data: {
        transaction: challengeTx.toXDR(),
        network_passphrase: Networks.TESTNET
      }
    });

    mockedAxios.post.mockResolvedValueOnce({
      data: {
        token: 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...'
      }
    });

    const token = await SEP10AuthService.authenticate(anchorAuthEndpoint, clientKeypair);
    expect(token).toBeDefined();
  });

  test('SEP-24: should initiate a deposit', async () => {
    mockedAxios.post.mockResolvedValueOnce({
      data: {
        url: 'https://anchor.com/interactive-deposit?id=123',
        id: '123-abc'
      }
    });

    const result = await SEP24Service.initiateDeposit(
      transferServerUrl,
      'auth-token',
      'USDC',
      clientKeypair.publicKey()
    );

    expect(result.url).toBe('https://anchor.com/interactive-deposit?id=123');
    expect(result.id).toBe('123-abc');
  });

  test('SEP-24: should initiate a withdrawal', async () => {
    mockedAxios.post.mockResolvedValueOnce({
      data: {
        url: 'https://anchor.com/interactive-withdrawal?id=456',
        id: '456-def'
      }
    });

    const result = await SEP24Service.initiateWithdrawal(
      transferServerUrl,
      'auth-token',
      'USDC',
      clientKeypair.publicKey()
    );

    expect(result.url).toBe('https://anchor.com/interactive-withdrawal?id=456');
    expect(result.id).toBe('456-def');
  });

  test('SEP-24: should fetch transaction status', async () => {
    mockedAxios.get.mockResolvedValueOnce({
      data: {
        transaction: {
          id: '123-abc',
          status: 'completed',
          amount_out: '100.00'
        }
      }
    });

    const tx = await SEP24Service.getTransactionStatus(
      transferServerUrl,
      'auth-token',
      '123-abc'
    );

    expect(tx.status).toBe('completed');
    expect(tx.amount_out).toBe('100.00');
  });
});
