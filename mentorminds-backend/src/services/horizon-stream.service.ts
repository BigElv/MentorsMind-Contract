import { eventIndexerService } from "./event-indexer.service";
import { ParsedEvent, ContractEvent } from "../types/event-indexer.types";

const HORIZON_URL =
  process.env.HORIZON_URL ?? "https://horizon-testnet.stellar.org";
const PLATFORM_STELLAR_ACCOUNT = process.env.PLATFORM_STELLAR_ACCOUNT ?? "";
const USER_WALLET_ACCOUNTS = (process.env.HORIZON_STREAM_ACCOUNTS ?? "")
  .split(",")
  .map((value) => value.trim())
  .filter(Boolean);
const STREAM_RETRY_DELAY_MS = 5000;
const MAX_RETRIES = 5;

// Known MentorMinds contract IDs to monitor (update after deployment)
const MONITORED_CONTRACTS = new Set<string>([
  // Escrow contract - add after deployment
  // Verification contract - add after deployment
  // MNT Token contract - add after deployment
  // Referral contract - add after deployment
]);

// Horizon API response types
interface HorizonEffectsResponse {
  _embedded: {
    records: HorizonEffect[];
  };
}

interface HorizonEffect {
  type: string;
  contract_id?: string;
  ledger_sequence: number;
  created_at: string;
  transaction_hash: string;
}

interface HorizonTransactionsResponse {
  _embedded: {
    records: HorizonTransaction[];
  };
}

interface HorizonTransaction {
  hash: string;
  ledger_sequence: number;
  created_at: string;
  successful: boolean;
  _links: {
    operations: {
      href: string;
    };
  };
}

interface HorizonOperationsResponse {
  _embedded: {
    records: HorizonOperation[];
  };
}

interface HorizonOperation {
  type: string;
}

export class HorizonStreamService {
  private abortController: AbortController | null = null;
  private isRunning = false;
  private retryCount = 0;

  /**
   * Start streaming contract events from Horizon
   * Uses cursor-based pagination to avoid re-processing
   */
  async startStreaming(): Promise<void> {
    if (this.isRunning) {
      console.log("[HorizonStream] Already running");
      return;
    }

    this.isRunning = true;
    this.retryCount = 0;
    this.abortController = new AbortController();

    const cursorState = eventIndexerService.getCursorState();
    const cursor = cursorState.lastCursor || cursorState.lastLedger.toString();

    console.log(`[HorizonStream] Starting stream from cursor: ${cursor}`);

    try {
      const accounts = this.getStreamAccounts();
      if (accounts.length === 0) {
        await this.streamEvents(cursor);
        return;
      }

      await Promise.all(accounts.map((accountId) => this.streamEvents(cursor, accountId)));
    } catch (error) {
      console.error("[HorizonStream] Stream error:", error);
      this.handleStreamError();
    }
  }

  /**
   * Stop streaming
   */
  stopStreaming(): void {
    this.isRunning = false;
    if (this.abortController) {
      this.abortController.abort();
      this.abortController = null;
    }
    console.log("[HorizonStream] Stopped");
  }

  /**
   * Stream events from Horizon with exponential backoff
   */
  private async streamEvents(cursor: string, accountId?: string): Promise<void> {
    const url = this.buildEventsUrl(cursor, accountId);

    try {
      const response = await fetch(url, {
        signal: this.abortController?.signal,
        headers: {
          Accept: "text/event-stream",
          "Cache-Control": "no-cache",
        },
      });

      if (!response.ok) {
        throw new Error(`Horizon HTTP error: ${response.status}`);
      }

      if (!response.body) {
        throw new Error("ReadableStream not supported");
      }

      this.retryCount = 0; // Reset retry counter on successful connection

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";

      while (this.isRunning && !this.abortController?.signal.aborted) {
        const { done, value } = await reader.read();

        if (done) {
          console.log(
            "[HorizonStream] Stream closed by server, reconnecting..."
          );
          break;
        }

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() || ""; // Keep incomplete line in buffer

        for (const line of lines) {
          if (line.startsWith("data: ")) {
            const eventData = line.slice(6);
            await this.processEventData(eventData);
          }
        }
      }
    } catch (error: any) {
      if (error.name === "AbortError") {
        console.log("[HorizonStream] Stream aborted");
      } else {
        throw error;
      }
    }
  }

  buildEventsUrl(cursor: string, accountId?: string): string {
    const params = new URLSearchParams({
      type: "contract",
      cursor,
    });

    if (accountId) {
      params.set("account", accountId);
    }

    return `${HORIZON_URL}/events?${params.toString()}`;
  }

  private getStreamAccounts(): string[] {
    const seen = new Set<string>();
    const accounts: string[] = [];

    for (const accountId of [PLATFORM_STELLAR_ACCOUNT, ...USER_WALLET_ACCOUNTS]) {
      if (!accountId || seen.has(accountId)) {
        continue;
      }
      seen.add(accountId);
      accounts.push(accountId);
    }

    return accounts;
  }

  /**
   * Process individual event data from SSE stream
   */
  private async processEventData(data: string): Promise<void> {
    try {
      const parsed = JSON.parse(data) as Record<string, any>;

      // Extract relevant fields from Horizon event
      const eventType = parsed.type;
      const ledger = parsed.ledger_sequence;
      const timestamp = new Date(parsed.created_at);
      const txHash = parsed.transaction_hash;

      // Skip if not a contract event
      if (eventType !== "contract") {
        return;
      }

      // Parse contract event details
      const contractId = parsed.contract_id;

      // Skip if not a monitored contract (if monitoring list is populated)
      if (
        MONITORED_CONTRACTS.size > 0 &&
        !MONITORED_CONTRACTS.has(contractId)
      ) {
        return;
      }

      // Decode XDR topics and data
      const topics = this.decodeXdrTopics(parsed.topic_xdr);
      const eventData = this.decodeXdrData(parsed.value_xdr);

      const parsedEvent: ParsedEvent = {
        contractId,
        type: this.extractEventType(topics),
        topics,
        data: eventData,
        ledger,
        timestamp,
        txHash,
      };

      // Convert to database format
      const contractEvent: ContractEvent = {
        id: "", // Will be set by saveEvent
        contractId: parsedEvent.contractId,
        eventType: parsedEvent.type,
        ledgerSequence: parsedEvent.ledger,
        ledgerTimestamp: parsedEvent.timestamp,
        transactionHash: parsedEvent.txHash,
        topicJson: parsedEvent.topics,
        dataJson: parsedEvent.data,
        createdAt: new Date(),
      };

      // Save to database
      await eventIndexerService.saveEvent(contractEvent);

      // Update cursor state
      eventIndexerService.updateCursorState(ledger, parsed.paging_token);
    } catch (error) {
      console.error("[HorizonStream] Error processing event data:", error);
    }
  }

  /**
   * Decode XDR topics to JSON
   */
  private decodeXdrTopics(topicXdr: string): any[] {
    try {
      // In production, use stellar-sdk to properly decode XDR
      // For now, return placeholder - implement with actual XDR parsing
      const topics = topicXdr.split(",").map((t: string) => t.trim());
      return topics;
    } catch (error) {
      console.error("[HorizonStream] Error decoding topics XDR:", error);
      return [];
    }
  }

  /**
   * Decode XDR data to JSON
   */
  private decodeXdrData(valueXdr: string): any {
    try {
      // In production, use stellar-sdk to properly decode XDR
      // For now, return placeholder - implement with actual XDR parsing
      return { raw: valueXdr };
    } catch (error) {
      console.error("[HorizonStream] Error decoding data XDR:", error);
      return {};
    }
  }

  /**
   * Extract event type from topics
   */
  private extractEventType(topics: any[]): string {
    if (topics.length === 0) return "unknown";

    const firstTopic = topics[0];
    if (typeof firstTopic === "string") {
      return firstTopic;
    }

    return "unknown";
  }

  /**
   * Handle stream errors with exponential backoff
   */
  private handleStreamError(): void {
    this.retryCount++;

    if (this.retryCount >= MAX_RETRIES) {
      console.error("[HorizonStream] Max retries reached, stopping stream");
      this.isRunning = false;
      return;
    }

    const delay = STREAM_RETRY_DELAY_MS * Math.pow(2, this.retryCount - 1);
    console.log(
      `[HorizonStream] Retrying in ${delay}ms (attempt ${this.retryCount}/${MAX_RETRIES})`
    );

    setTimeout(() => {
      if (this.isRunning) {
        const cursor = eventIndexerService.getCursorState().lastCursor || "now";
        const accounts = this.getStreamAccounts();
        if (accounts.length === 0) {
          this.streamEvents(cursor);
          return;
        }
        for (const accountId of accounts) {
          this.streamEvents(cursor, accountId);
        }
      }
    }, delay);
  }

  /**
   * Fetch historical events for a specific account
   * Useful for catching up after downtime
   */
  async fetchAccountEvents(
    accountId: string,
    limit: number = 200
  ): Promise<ParsedEvent[]> {
    try {
      const url = `${HORIZON_URL}/accounts/${accountId}/effects?limit=${limit}&order=desc`;
      const response = await fetch(url);

      if (!response.ok) {
        throw new Error(`Horizon error: ${response.status}`);
      }

      const data = (await response.json()) as HorizonEffectsResponse; // ✅ fix line 274

      const events: ParsedEvent[] = [];

      for (const effect of data._embedded.records) {
        if (
          effect.type === "contract_created" ||
          effect.type === "contract_event"
        ) {
          events.push({
            contractId: effect.contract_id || "",
            type: effect.type,
            topics: [],
            data: effect,
            ledger: effect.ledger_sequence,
            timestamp: new Date(effect.created_at),
            txHash: effect.transaction_hash,
          });
        }
      }

      return events;
    } catch (error) {
      console.error("[HorizonStream] Error fetching account events:", error);
      return [];
    }
  }

  /**
   * Fetch transactions with contract operations
   */
  async fetchTransactionsWithContracts(limit: number = 50): Promise<any[]> {
    try {
      const url = `${HORIZON_URL}/transactions?limit=${limit}&order=desc&include_failed=false`;
      const response = await fetch(url);

      if (!response.ok) {
        throw new Error(`Horizon error: ${response.status}`);
      }

      const data = (await response.json()) as HorizonTransactionsResponse; // ✅ fix line 313

      const transactions: any[] = [];

      for (const tx of data._embedded.records) {
        // Check if transaction has invoke_host_function operation
        const operationsUrl = `${tx._links.operations.href}`;
        const opsResponse = await fetch(operationsUrl);

        if (opsResponse.ok) {
          const opsData =
            (await opsResponse.json()) as HorizonOperationsResponse; // ✅ fix line 321

          for (const op of opsData._embedded.records) {
            if (op.type === "invoke_host_function") {
              transactions.push({
                hash: tx.hash,
                ledger: tx.ledger_sequence,
                timestamp: new Date(tx.created_at),
                successful: tx.successful,
                operation: op,
              });
              break;
            }
          }
        }
      }

      return transactions;
    } catch (error) {
      console.error("[HorizonStream] Error fetching transactions:", error);
      return [];
    }
  }
}

export const horizonStreamService = new HorizonStreamService();
