export type PaymentStatus = 'pending' | 'confirmed' | 'failed' | 'timeout';

export interface Payment {
  id: string;
  sessionId: string;
  senderAddress: string;
  receiverAddress: string;
  amount: string;
  assetCode: string;
  txHash: string | null;
  ledgerSequence: number | null;
  status: PaymentStatus;
  fee: string | null;
  errorCode: string | null;
  errorMessage: string | null;
  createdAt: Date;
  updatedAt: Date;
}

export interface CreatePaymentDto {
  sessionId: string;
  senderAddress: string;
  receiverAddress: string;
  amount: string;
  assetCode: string;
  txHash: string;
  fee?: string;
}

export interface HorizonWebhookPayload {
  type: string;
  transaction_hash: string;
  ledger: number;
  successful: boolean;
  result_code?: string;
}
