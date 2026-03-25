import {
  ContractEvent,
  ParsedEvent,
  EscrowEvent,
  CursorState,
} from "../types/event-indexer.types";

// In-memory store — replace with PostgreSQL in production
const contractEvents = new Map<string, ContractEvent>();
let cursorState: CursorState = {
  lastLedger: 0,
  lastCursor: undefined,
  updatedAt: new Date(),
};

// Event subscribers for WebSocket notifications
type EventSubscriber = (event: ContractEvent) => void;
const subscribers = new Set<EventSubscriber>();

export class EventIndexerService {
  /**
   * Save parsed contract event to database
   */
  async saveEvent(event: ContractEvent): Promise<void> {
    const eventId = `${event.transactionHash}-${event.eventType}-${event.ledgerSequence}`;
    event.id = eventId;

    // Check for duplicates
    if (!contractEvents.has(eventId)) {
      contractEvents.set(eventId, event);
      console.log(
        `[EventIndexer] Saved event: ${event.eventType} for contract ${event.contractId} in ledger ${event.ledgerSequence}`
      );

      // Notify subscribers
      this.notifySubscribers(event);
    }
  }

  /**
   * Save multiple events in batch
   */
  async saveEvents(events: ContractEvent[]): Promise<void> {
    for (const event of events) {
      await this.saveEvent(event);
    }
  }

  /**
   * Get cursor state for resuming after restart
   */
  getCursorState(): CursorState {
    return cursorState;
  }

  /**
   * Update cursor state after processing ledger
   */
  updateCursorState(ledger: number, cursor?: string): void {
    cursorState = {
      lastLedger: ledger,
      lastCursor: cursor,
      updatedAt: new Date(),
    };
  }

  /**
   * Get events by contract ID
   */
  getEventsByContract(contractId: string): ContractEvent[] {
    return Array.from(contractEvents.values()).filter(
      (e) => e.contractId === contractId
    );
  }

  /**
   * Get events by transaction hash
   */
  getEventsByTxHash(txHash: string): ContractEvent[] {
    return Array.from(contractEvents.values()).filter(
      (e) => e.transactionHash === txHash
    );
  }

  /**
   * Get recent events with pagination
   */
  getRecentEvents(limit: number = 50, offset: number = 0): ContractEvent[] {
    return Array.from(contractEvents.values())
      .sort((a, b) => b.ledgerSequence - a.ledgerSequence)
      .slice(offset, offset + limit);
  }

  /**
   * Parse escrow-specific events
   */
  parseEscrowEvent(event: ParsedEvent): EscrowEvent | null {
    try {
      const topics = event.topics;
      const data = event.data;

      if (topics.length < 2) return null;

      const eventType = this.decodeEventType(topics[0]);
      const escrowId =
        typeof topics[1] === "number"
          ? topics[1]
          : parseInt(topics[1]?.toString() || "0");

      const escrowEvent: EscrowEvent = {
        escrowId,
        eventType,
      };

      // Parse event-specific data based on type
      switch (eventType) {
        case "created":
          if (data && Array.isArray(data) && data.length >= 5) {
            escrowEvent.mentor = data[0];
            escrowEvent.learner = data[1];
            escrowEvent.amount = data[2]?.toString();
            escrowEvent.sessionId = data[3];
            escrowEvent.tokenAddress = data[4];
          }
          break;

        case "released":
        case "auto_released":
          if (data && Array.isArray(data) && data.length >= 5) {
            escrowEvent.mentor = data[0];
            escrowEvent.amount = data[1]?.toString();
            escrowEvent.netAmount = data[2]?.toString();
            escrowEvent.platformFee = data[3]?.toString();
            escrowEvent.tokenAddress = data[4];
          }
          break;

        case "dispute_opened":
          if (data && Array.isArray(data) && data.length >= 4) {
            escrowEvent.disputeReason = data[2];
          }
          break;

        case "dispute_resolved":
          if (data && Array.isArray(data) && data.length >= 6) {
            escrowEvent.mentorPct = data[1];
            escrowEvent.mentorAmount = data[2]?.toString();
            escrowEvent.learnerAmount = data[3]?.toString();
            escrowEvent.tokenAddress = data[4];
            escrowEvent.resolvedAt = new Date(data[5] * 1000); // Convert Unix timestamp
          }
          break;

        case "refunded":
          if (data && Array.isArray(data) && data.length >= 3) {
            escrowEvent.learner = data[0];
            escrowEvent.amount = data[1]?.toString();
            escrowEvent.tokenAddress = data[2];
          }
          break;
      }

      return escrowEvent;
    } catch (error) {
      console.error("[EventIndexer] Error parsing escrow event:", error);
      return null;
    }
  }

  /**
   * Subscribe to event notifications (for WebSocket)
   */
  subscribe(callback: EventSubscriber): () => void {
    subscribers.add(callback);
    // Return unsubscribe function
    return () => {
      subscribers.delete(callback);
    };
  }

  /**
   * Notify all subscribers of new event
   */
  private notifySubscribers(event: ContractEvent): void {
    subscribers.forEach((callback) => {
      try {
        callback(event);
      } catch (error) {
        console.error("[EventIndexer] Error notifying subscriber:", error);
      }
    });
  }

  /**
   * Decode event type from topic
   */
  private decodeEventType(topic: any): EscrowEvent["eventType"] {
    if (typeof topic === "string") {
      if (topic.includes("created")) return "created";
      if (topic.includes("released")) return "released";
      if (topic.includes("auto_rel")) return "auto_released";
      if (topic.includes("disp_opnd")) return "dispute_opened";
      if (topic.includes("disp_res")) return "dispute_resolved";
      if (topic.includes("refunded")) return "refunded";
    }
    return "created"; // default fallback
  }

  /**
   * Clear in-memory store (for testing)
   */
  clear(): void {
    contractEvents.clear();
    cursorState = {
      lastLedger: 0,
      lastCursor: undefined,
      updatedAt: new Date(),
    };
  }
}

export const eventIndexerService = new EventIndexerService();
