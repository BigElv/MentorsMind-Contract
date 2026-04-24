import { AdminEscrowService } from './escrow.service';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type DisputeAction = 'full_refund' | 'partial_refund' | 'release';
export type DisputeStatus = 'open' | 'resolved';

export interface Dispute {
  id: string;
  transactionId: string;
  action?: DisputeAction;
  status: DisputeStatus;
  escrowTxHash?: string;
  createdAt: Date;
  updatedAt: Date;
}

interface BookingEscrowInfo {
  escrowId: number;
  escrowContractAddress: string;
}

// ---------------------------------------------------------------------------
// In-memory stores (replace with DB in production)
// ---------------------------------------------------------------------------

const disputes = new Map<string, Dispute>();

// Keyed by transactionId — populated when a booking/escrow is created
const bookings = new Map<string, BookingEscrowInfo>();

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function getSorobanEscrowService(contractAddress: string): AdminEscrowService {
  const rpcUrl = process.env.SOROBAN_RPC_URL ?? 'https://soroban-testnet.stellar.org';
  const adminSecret = process.env.ADMIN_SECRET_KEY;
  if (!adminSecret) throw new Error('ADMIN_SECRET_KEY env var not set');
  return new AdminEscrowService(contractAddress, rpcUrl, adminSecret);
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Register a booking's escrow info so disputes can look it up by transactionId.
 */
export function registerBooking(transactionId: string, info: BookingEscrowInfo): void {
  bookings.set(transactionId, info);
}

/**
 * Resolve a dispute by executing the real Soroban escrow action.
 *
 * Atomicity: escrow call happens first. DB status is only updated on success.
 * If the escrow call throws, the dispute remains 'open' and the error propagates.
 */
export async function resolveDispute(disputeId: string, action: DisputeAction): Promise<Dispute> {
  const dispute = disputes.get(disputeId);
  if (!dispute) throw new Error(`Dispute ${disputeId} not found`);
  if (dispute.status === 'resolved') throw new Error(`Dispute ${disputeId} already resolved`);

  const booking = bookings.get(dispute.transactionId);
  if (!booking) {
    throw new Error(`No booking found for transactionId ${dispute.transactionId}`);
  }

  const escrowService = getSorobanEscrowService(booking.escrowContractAddress);

  // Execute escrow action FIRST — DB update only happens on success
  let escrowTxHash: string;
  if (action === 'full_refund' || action === 'partial_refund') {
    escrowTxHash = await escrowService.refund(booking.escrowId);
  } else {
    // release — funds go to mentor
    escrowTxHash = await escrowService.resolveDispute(booking.escrowId, true);
  }

  // Escrow succeeded — now update DB state
  dispute.action = action;
  dispute.status = 'resolved';
  dispute.escrowTxHash = escrowTxHash;
  dispute.updatedAt = new Date();
  disputes.set(disputeId, dispute);

  return dispute;
}

export function createDispute(id: string, transactionId: string): Dispute {
  const dispute: Dispute = {
    id,
    transactionId,
    status: 'open',
    createdAt: new Date(),
    updatedAt: new Date(),
  };
  disputes.set(id, dispute);
  return dispute;
}

export function getDispute(id: string): Dispute | null {
  return disputes.get(id) ?? null;
}
