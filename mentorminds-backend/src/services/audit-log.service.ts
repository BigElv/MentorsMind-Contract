export interface AuditLogEntry {
  id: string;
  txHash: string;
  walletAddress: string;
  sequenceNumber: string;
  createdAt: Date;
}

// In-memory store — replace with DB in production
const logs: AuditLogEntry[] = [];

export class AuditLogService {
  create(data: Omit<AuditLogEntry, 'id' | 'createdAt'>): AuditLogEntry {
    const entry: AuditLogEntry = {
      id: crypto.randomUUID(),
      ...data,
      createdAt: new Date(),
    };
    logs.push(entry);
    return entry;
  }

  findByWallet(walletAddress: string): AuditLogEntry[] {
    return logs.filter(e => e.walletAddress === walletAddress);
  }

  findByTxHash(txHash: string): AuditLogEntry | undefined {
    return logs.find(e => e.txHash === txHash);
  }

  getAll(): AuditLogEntry[] {
    return [...logs];
  }
}

export const auditLogService = new AuditLogService();
