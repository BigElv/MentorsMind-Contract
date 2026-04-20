export interface ContractEvent {
  id: string;
  contractId: string;
  eventType: string;
  ledgerSequence: number;
  ledgerTimestamp: Date;
  transactionHash: string;
  topicJson: any;
  dataJson: any;
  createdAt: Date;
}

export interface EscrowEvent {
  escrowId: number;
  eventType:
    | "created"
    | "released"
    | "auto_released"
    | "dispute_opened"
    | "dispute_resolved"
    | "refunded";
  mentor?: string;
  learner?: string;
  amount?: string;
  sessionId?: string;
  tokenAddress?: string;
  platformFee?: string;
  netAmount?: string;
  disputeReason?: string;
  mentorPct?: number;
  mentorAmount?: string;
  learnerAmount?: string;
  resolvedAt?: Date;
}

export interface ParsedEvent {
  contractId: string;
  type: string;
  topics: any[];
  data: any;
  ledger: number;
  timestamp: Date;
  txHash: string;
}

export interface CursorState {
  lastLedger: number;
  lastCursor?: string;
  updatedAt: Date;
}
