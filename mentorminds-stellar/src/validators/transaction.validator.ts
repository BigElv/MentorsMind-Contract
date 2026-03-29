import { StrKey } from '@stellar/stellar-sdk';

import type {
  PaymentTransactionRequest,
  SupportedAssetCode,
} from '../types/transaction.types';

const DECIMAL_AMOUNT_REGEX = /^\d+(?:\.\d{1,7})?$/;

export class TransactionValidationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'TransactionValidationError';
  }
}

export function validateStellarAddress(address: string, fieldName: string): void {
  if (!address || !StrKey.isValidEd25519PublicKey(address)) {
    throw new TransactionValidationError(`${fieldName} must be a valid Stellar public key`);
  }
}

export function validateAmount(amount: string): void {
  if (!amount || !DECIMAL_AMOUNT_REGEX.test(amount)) {
    throw new TransactionValidationError('Amount must be a positive decimal with up to 7 decimal places');
  }

  if (Number(amount) <= 0) {
    throw new TransactionValidationError('Amount must be greater than zero');
  }
}

export function validateAssetCode(assetCode: SupportedAssetCode): void {
  if (!['XLM', 'USDC', 'PYUSD'].includes(assetCode)) {
    throw new TransactionValidationError(`Unsupported asset code: ${assetCode}`);
  }
}

export function validateMemoText(memo?: string): void {
  if (!memo) {
    return;
  }

  if (!memo.trim()) {
    throw new TransactionValidationError('Memo cannot be empty when provided');
  }
}

export function validateTransactionRequest(request: Partial<PaymentTransactionRequest>): asserts request is PaymentTransactionRequest {
  validateStellarAddress(request.source ?? '', 'Source address');
  validateStellarAddress(request.destination ?? '', 'Destination address');
  validateAmount(request.amount ?? '');
  validateAssetCode((request.assetCode ?? 'XLM') as SupportedAssetCode);
  validateMemoText(request.memo);

  if (!request.networkPassphrase) {
    throw new TransactionValidationError('Network passphrase is required');
  }

  if (!request.horizonUrl) {
    throw new TransactionValidationError('Horizon URL is required');
  }

  if (!request.timeoutSeconds || request.timeoutSeconds <= 0) {
    throw new TransactionValidationError('Timeout must be greater than zero');
  }
}
