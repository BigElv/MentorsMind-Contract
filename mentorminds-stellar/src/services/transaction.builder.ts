import {
  Asset,
  Horizon,
  Memo,
  Networks,
  Operation,
  Transaction,
  TransactionBuilder as StellarSdkTransactionBuilder,
} from '@stellar/stellar-sdk';

import { stellarConfig } from '../config/stellar.config';
import { FeeCalculator } from './fee.calculator';
import type {
  AssetDefinition,
  FreighterSignResult,
  PaymentTransactionRequest,
  PreparedMemo,
  PreparedTransaction,
  RetryPolicy,
  SupportedAssetCode,
  TransactionSimulationResult,
  TransactionSubmissionResult,
} from '../types/transaction.types';
import type { StellarNetwork } from '../types/stellar.types';
import {
  validateTransactionRequest,
  TransactionValidationError,
} from '../validators/transaction.validator';

const DEFAULT_TIMEOUT_SECONDS = 180;
const DEFAULT_RETRY_POLICY: RetryPolicy = {
  maxAttempts: 3,
  initialDelayMs: 1500,
  backoffMultiplier: 2,
};
const STROOPS_SCALE = 10_000_000n;

type EnvMap = Record<string, string | undefined>;
type AccountLike = {
  balances: any[];
  data_attr?: Record<string, string>;
};
declare const process:
  | {
      env?: EnvMap;
    }
  | undefined;

type FreighterApiLike = {
  signTransaction?: (
    xdr: string,
    opts?: { network?: string; networkPassphrase?: string; address?: string },
  ) => Promise<{ signedTxXdr?: string; signerAddress?: string; error?: string | { message?: string } } | string>;
  requestAccess?: () => Promise<{ address?: string; error?: string | { message?: string } }>;
  getAddress?: () => Promise<{ address?: string; error?: string | { message?: string } }>;
};

function getEnvValue(name: string): string | undefined {
  const viteEnv = (import.meta as ImportMeta & { env?: EnvMap }).env;
  const processEnv = typeof process !== 'undefined' ? (process.env as EnvMap) : undefined;
  return viteEnv?.[name] ?? processEnv?.[name];
}

function resolveNetworkPassphrase(network: StellarNetwork): string {
  return network === 'mainnet' ? Networks.PUBLIC : Networks.TESTNET;
}

function resolveHorizonUrl(network: StellarNetwork): string {
  return network === 'mainnet'
    ? 'https://horizon.stellar.org'
    : 'https://horizon-testnet.stellar.org';
}

function toScaledBigInt(amount: string): bigint {
  const [wholePart, fractionPart = ''] = amount.split('.');
  const normalizedFraction = `${fractionPart}0000000`.slice(0, 7);
  return BigInt(wholePart) * STROOPS_SCALE + BigInt(normalizedFraction);
}

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

function hashToHex(hash: Uint8Array): string {
  return Array.from(hash, byte => byte.toString(16).padStart(2, '0')).join('');
}

function normalizeErrorMessage(error: unknown): string {
  if (typeof error === 'string') {
    return error;
  }

  if (error instanceof Error) {
    return error.message;
  }

  if (error && typeof error === 'object') {
    const candidate = error as { message?: string; error?: string | { message?: string } };
    if (candidate.message) {
      return candidate.message;
    }
    if (typeof candidate.error === 'string') {
      return candidate.error;
    }
    if (candidate.error && typeof candidate.error === 'object' && candidate.error.message) {
      return candidate.error.message;
    }
  }

  return 'Unknown Stellar transaction error';
}

function getExplorerUrl(hash: string, network: StellarNetwork): string {
  const networkSegment = network === 'mainnet' ? 'public' : 'testnet';
  return `https://stellar.expert/explorer/${networkSegment}/tx/${hash}`;
}

function getFreighterApi(): FreighterApiLike | null {
  if (typeof window === 'undefined') {
    return null;
  }

  const freighterWindow = window as Window & {
    freighterApi?: FreighterApiLike;
    freighter?: FreighterApiLike;
  };

  return freighterWindow.freighterApi ?? freighterWindow.freighter ?? null;
}

async function createMemoField(memo?: string): Promise<PreparedMemo> {
  if (!memo) {
    return {
      memo: Memo.none(),
      type: 'none',
    };
  }

  const encoded = new TextEncoder().encode(memo);
  if (encoded.byteLength <= 28) {
    return {
      memo: Memo.text(memo),
      type: 'text',
      value: memo,
    };
  }

  if (!globalThis.crypto?.subtle) {
    throw new TransactionValidationError(
      'Memo is longer than Stellar text memo limits and hashing is unavailable in this environment',
    );
  }

  const digest = await globalThis.crypto.subtle.digest('SHA-256', encoded);
  const hashValue = hashToHex(new Uint8Array(digest));

  return {
    memo: Memo.hash(hashValue),
    type: 'hash',
    value: hashValue,
    warning: 'Memo exceeded 28 bytes, so a SHA-256 memo hash will be stored instead.',
  };
}

function resolveAssetDefinition(assetCode: SupportedAssetCode): AssetDefinition {
  if (assetCode === 'XLM') {
    return {
      code: 'XLM',
      isNative: true,
    };
  }

  const issuerEnvName = assetCode === 'USDC' ? 'VITE_USDC_ISSUER' : 'VITE_PYUSD_ISSUER';
  const nextIssuerEnvName = assetCode === 'USDC' ? 'NEXT_PUBLIC_USDC_ISSUER' : 'NEXT_PUBLIC_PYUSD_ISSUER';
  const issuer = getEnvValue(issuerEnvName) ?? getEnvValue(nextIssuerEnvName);

  if (!issuer) {
    throw new TransactionValidationError(
      `Missing issuer configuration for ${assetCode}. Set ${issuerEnvName} or ${nextIssuerEnvName}.`,
    );
  }

  return {
    code: assetCode,
    issuer,
    isNative: false,
  };
}

function findAssetBalance(account: AccountLike, asset: AssetDefinition): string | undefined {
  const balance = account.balances.find((entry: any) => {
    if (asset.isNative) {
      return entry.asset_type === 'native';
    }

    return entry.asset_code === asset.code && entry.asset_issuer === asset.issuer;
  });

  return balance?.balance;
}

function destinationHasTrustline(account: AccountLike, asset: AssetDefinition): boolean {
  if (asset.isNative) {
    return true;
  }

  return account.balances.some((entry: any) => {
    if (entry.asset_code !== asset.code || entry.asset_issuer !== asset.issuer) {
      return false;
    }

    if ('is_authorized' in entry && entry.is_authorized === false) {
      return false;
    }

    return true;
  });
}

function accountRequiresMemo(account: AccountLike): boolean {
  return account.data_attr?.['config.memo_required'] === 'MQ==';
}

function isRetriableSubmissionError(error: unknown): boolean {
  if (!error || typeof error !== 'object') {
    return false;
  }

  const candidate = error as {
    response?: { status?: number };
    status?: number;
    extras?: { result_codes?: { transaction?: string } };
  };

  const status = candidate.response?.status ?? candidate.status;
  if (status && status >= 500) {
    return true;
  }

  const txCode = candidate.extras?.result_codes?.transaction;
  return txCode === 'tx_internal_error';
}

export class TransactionBuilderService {
  private request: Partial<PaymentTransactionRequest> = {
    network: (stellarConfig.network as StellarNetwork) ?? 'testnet',
    horizonUrl: stellarConfig.horizonUrl,
    networkPassphrase: stellarConfig.networkPassphrase,
    timeoutSeconds: DEFAULT_TIMEOUT_SECONDS,
  };

  private retryPolicy: RetryPolicy = { ...DEFAULT_RETRY_POLICY };
  private server: Horizon.Server;

  constructor() {
    this.server = new Horizon.Server(this.request.horizonUrl ?? resolveHorizonUrl('testnet'));
  }

  public from(address: string): this {
    this.request.source = address;
    return this;
  }

  public to(address: string): this {
    this.request.destination = address;
    return this;
  }

  public amount(amount: string): this {
    this.request.amount = amount;
    return this;
  }

  public asset(assetCode: SupportedAssetCode): this {
    this.request.assetCode = assetCode;
    return this;
  }

  public memo(memo: string): this {
    this.request.memo = memo;
    return this;
  }

  public timeout(seconds: number): this {
    this.request.timeoutSeconds = seconds;
    return this;
  }

  public onNetwork(network: StellarNetwork): this {
    this.request.network = network;
    this.request.networkPassphrase = resolveNetworkPassphrase(network);
    this.request.horizonUrl =
      getEnvValue('VITE_HORIZON_URL') ??
      getEnvValue('NEXT_PUBLIC_HORIZON_URL') ??
      resolveHorizonUrl(network);
    this.server = new Horizon.Server(this.request.horizonUrl);
    return this;
  }

  public withHorizonUrl(url: string): this {
    this.request.horizonUrl = url;
    this.server = new Horizon.Server(url);
    return this;
  }

  public withRetryPolicy(policy: Partial<RetryPolicy>): this {
    this.retryPolicy = {
      ...this.retryPolicy,
      ...policy,
    };
    return this;
  }

  public async simulateTransaction(): Promise<TransactionSimulationResult> {
    try {
      const prepared = await this.prepareTransaction();
      const sourceAccount = await this.server.loadAccount(prepared.request.source);
      const destinationAccount = await this.server.loadAccount(prepared.request.destination);
      const sourceBalance = findAssetBalance(sourceAccount, prepared.asset);
      const destinationBalance = findAssetBalance(destinationAccount, prepared.asset);
      const warnings: string[] = [];

      if (prepared.memo.warning) {
        warnings.push(prepared.memo.warning);
      }

      if (accountRequiresMemo(destinationAccount) && prepared.memo.type === 'none') {
        throw new TransactionValidationError(
          'Destination account requires a memo. Provide a session ID before submitting this payment.',
        );
      }

      if (!prepared.asset.isNative && !destinationHasTrustline(destinationAccount, prepared.asset)) {
        throw new TransactionValidationError(
          `Destination account does not trust ${prepared.asset.code} issued by ${prepared.asset.issuer}.`,
        );
      }

      if (!sourceBalance) {
        throw new TransactionValidationError(`Source account does not hold ${prepared.asset.code}.`);
      }

      const sourceScaled = toScaledBigInt(sourceBalance);
      const amountScaled = toScaledBigInt(prepared.request.amount);

      if (sourceScaled < amountScaled) {
        throw new TransactionValidationError(
          `Insufficient ${prepared.asset.code} balance. Available: ${sourceBalance}, required: ${prepared.request.amount}.`,
        );
      }

      if (prepared.asset.isNative) {
        const xlmBalance = findAssetBalance(sourceAccount, { code: 'XLM', isNative: true });
        const feeScaled = BigInt(prepared.fee.suggestedFeeStroops);

        if (!xlmBalance || toScaledBigInt(xlmBalance) < feeScaled) {
          throw new TransactionValidationError('Insufficient XLM balance to cover transaction fees.');
        }

        warnings.push('Dry-run checks XLM balance and fee coverage, but final reserve requirements are enforced by Horizon at submission time.');
      }

      return {
        success: true,
        fee: prepared.fee,
        transactionXdr: prepared.transaction.toXDR(),
        memoType: prepared.memo.type,
        memoValue: prepared.memo.value,
        sourceBalance,
        destinationBalance,
        warnings,
      };
    } catch (error) {
      return {
        success: false,
        fee: {
          baseFeeStroops: '0',
          suggestedFeeStroops: '0',
          suggestedFeeXlm: '0',
          operationCount: 1,
        },
        memoType: 'none',
        warnings: [],
        error: normalizeErrorMessage(error),
      };
    }
  }

  public async signAndSubmit(): Promise<TransactionSubmissionResult> {
    const prepared = await this.prepareTransaction();
    const unsignedXdr = prepared.transaction.toXDR();
    const signed = await this.signWithFreighter(unsignedXdr, prepared.request.source, prepared.request.networkPassphrase);
    const signedTransaction = new Transaction(signed.signedTxXdr, prepared.request.networkPassphrase);
    const hash = hashToHex(signedTransaction.hash());
    const submission = await this.submitWithRetry(signedTransaction, hash);

    return {
      hash,
      ledger: submission.ledger,
      envelopeXdr: submission.envelope_xdr,
      resultXdr: submission.result_xdr,
      resultMetaXdr: submission.result_meta_xdr,
      explorerUrl: getExplorerUrl(hash, prepared.request.network),
      submission,
    };
  }

  private async prepareTransaction(): Promise<PreparedTransaction> {
    this.request.network = this.request.network ?? ((stellarConfig.network as StellarNetwork) || 'testnet');
    this.request.horizonUrl = this.request.horizonUrl ?? stellarConfig.horizonUrl ?? resolveHorizonUrl(this.request.network);
    this.request.networkPassphrase =
      this.request.networkPassphrase ?? resolveNetworkPassphrase(this.request.network);
    this.request.timeoutSeconds = this.request.timeoutSeconds ?? DEFAULT_TIMEOUT_SECONDS;

    validateTransactionRequest(this.request);

    this.server = new Horizon.Server(this.request.horizonUrl);

    const sourceAccount = await this.server.loadAccount(this.request.source);
    const fee = await FeeCalculator.calculateSuggestedFee(this.server, 1);
    const memo = await createMemoField(this.request.memo);
    const assetDefinition = resolveAssetDefinition(this.request.assetCode);
    const stellarAsset = assetDefinition.isNative
      ? Asset.native()
      : new Asset(assetDefinition.code, assetDefinition.issuer as string);

    const transaction = new StellarSdkTransactionBuilder(sourceAccount, {
      fee: fee.suggestedFeeStroops,
      networkPassphrase: this.request.networkPassphrase,
    })
      .addOperation(
        Operation.payment({
          destination: this.request.destination,
          amount: this.request.amount,
          asset: stellarAsset,
        }),
      )
      .addMemo(memo.memo)
      .setTimeout(this.request.timeoutSeconds)
      .build();

    return {
      transaction,
      request: this.request,
      asset: assetDefinition,
      fee,
      memo,
    };
  }

  private async signWithFreighter(
    xdr: string,
    sourceAddress: string,
    networkPassphrase: string,
  ): Promise<FreighterSignResult> {
    const freighter = getFreighterApi();

    if (!freighter?.signTransaction) {
      throw new Error('Freighter wallet was not found in this browser.');
    }

    if (freighter.requestAccess) {
      const accessResponse = await freighter.requestAccess();
      const accessError = normalizeErrorMessage(accessResponse?.error);
      if (accessResponse?.error) {
        throw new Error(accessError);
      }
      if (accessResponse?.address && accessResponse.address !== sourceAddress) {
        throw new Error(`Freighter is connected to ${accessResponse.address}, but ${sourceAddress} is required for this payment.`);
      }
    } else if (freighter.getAddress) {
      const addressResponse = await freighter.getAddress();
      const addressError = normalizeErrorMessage(addressResponse?.error);
      if (addressResponse?.error) {
        throw new Error(addressError);
      }
      if (addressResponse?.address && addressResponse.address !== sourceAddress) {
        throw new Error(`Freighter is connected to ${addressResponse.address}, but ${sourceAddress} is required for this payment.`);
      }
    }

    const signResponse = await freighter.signTransaction(xdr, {
      networkPassphrase,
      address: sourceAddress,
    });

    if (typeof signResponse === 'string') {
      return { signedTxXdr: signResponse };
    }

    if (signResponse.error) {
      throw new Error(normalizeErrorMessage(signResponse.error));
    }

    if (!signResponse.signedTxXdr) {
      throw new Error('Freighter did not return a signed transaction XDR.');
    }

    if (signResponse.signerAddress && signResponse.signerAddress !== sourceAddress) {
      throw new Error(`Freighter signed with ${signResponse.signerAddress}, but ${sourceAddress} is required.`);
    }

    return {
      signedTxXdr: signResponse.signedTxXdr,
      signerAddress: signResponse.signerAddress,
    };
  }

  private async submitWithRetry(
    signedTransaction: Transaction,
    hash: string,
  ): Promise<Horizon.HorizonApi.SubmitTransactionResponse> {
    let attempt = 0;
    let delayMs = this.retryPolicy.initialDelayMs;
    let lastError: unknown;

    while (attempt < this.retryPolicy.maxAttempts) {
      attempt += 1;

      try {
        return await this.server.submitTransaction(signedTransaction);
      } catch (error) {
        lastError = error;

        try {
          const existingTx = await this.server.transactions().transaction(hash).call();
          return {
            hash: existingTx.hash,
            paging_token: existingTx.paging_token,
            ledger: undefined,
            envelope_xdr: existingTx.envelope_xdr,
            result_xdr: existingTx.result_xdr,
            result_meta_xdr: existingTx.result_meta_xdr,
            extras: {},
            successful: true,
          } as unknown as Horizon.HorizonApi.SubmitTransactionResponse;
        } catch {
          // If Horizon cannot find the transaction yet, continue with retry handling.
        }

        if (!isRetriableSubmissionError(error) || attempt >= this.retryPolicy.maxAttempts) {
          break;
        }

        await sleep(delayMs);
        delayMs = Math.round(delayMs * this.retryPolicy.backoffMultiplier);
      }
    }

    throw new Error(normalizeErrorMessage(lastError));
  }
}
