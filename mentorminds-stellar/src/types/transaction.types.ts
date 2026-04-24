import type { Horizon, Memo, Transaction } from '@stellar/stellar-sdk';

import type { StellarNetwork } from './stellar.types';

export type SupportedAssetCode = 'XLM' | 'USDC' | 'PYUSD';

export type MemoStrategy = 'none' | 'text' | 'hash';

export interface AssetDefinition {
  code: SupportedAssetCode;
  issuer?: string;
  isNative: boolean;
}

export interface PaymentTransactionRequest {
  source: string;
  destination: string;
  amount: string;
  assetCode: SupportedAssetCode;
  memo?: string;
  network: StellarNetwork;
  horizonUrl: string;
  networkPassphrase: string;
  timeoutSeconds: number;
}

export interface RetryPolicy {
  maxAttempts: number;
  initialDelayMs: number;
  backoffMultiplier: number;
}

export interface FeeDetails {
  baseFeeStroops: string;
  suggestedFeeStroops: string;
  suggestedFeeXlm: string;
  operationCount: number;
}

export interface PreparedMemo {
  memo: Memo;
  type: MemoStrategy;
  value?: string;
  warning?: string;
}

export interface PreparedTransaction {
  transaction: Transaction;
  request: PaymentTransactionRequest;
  asset: AssetDefinition;
  fee: FeeDetails;
  memo: PreparedMemo;
}

export interface TransactionSimulationResult {
  success: boolean;
  fee: FeeDetails;
  transactionXdr?: string;
  memoType: MemoStrategy;
  memoValue?: string;
  sourceBalance?: string;
  destinationBalance?: string;
  warnings: string[];
  error?: string;
}

export interface FreighterSignResult {
  signedTxXdr: string;
  signerAddress?: string;
}

export interface TransactionSubmissionResult {
  hash: string;
  ledger?: number;
  envelopeXdr?: string;
  resultXdr?: string;
  resultMetaXdr?: string;
  explorerUrl?: string;
  submission: Horizon.HorizonApi.SubmitTransactionResponse;
}
