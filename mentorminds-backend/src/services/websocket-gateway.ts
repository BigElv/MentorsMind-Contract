import { Server as HttpServer } from "http";
import { Server as SocketIOServer, Socket } from "socket.io";
import { eventIndexerService } from "../services/event-indexer.service";
import { ContractEvent } from "../types/event-indexer.types";

export class WebSocketGateway {
  private io: SocketIOServer | null = null;
  private httpServer: HttpServer | null = null;

  /**
   * Initialize WebSocket server
   */
  init(httpServer: HttpServer): void {
    this.httpServer = httpServer;
    this.io = new SocketIOServer(httpServer, {
      cors: {
        origin: process.env.FRONTEND_URL || "*",
        methods: ["GET", "POST"],
      },
      path: "/ws/events",
    });

    this.io.on("connection", (socket: Socket) => {
      console.log(`[WebSocket] Client connected: ${socket.id}`);
      this.handleConnection(socket);
    });

    console.log("[WebSocket] Gateway initialized on /ws/events");
  }

  /**
   * Handle client connection
   */
  private handleConnection(socket: Socket): void {
    // Join specific rooms (e.g., monitor specific contracts)
    socket.on("join", (data: { contractId?: string; txHash?: string }) => {
      if (data.contractId) {
        socket.join(`contract:${data.contractId}`);
        console.log(
          `[WebSocket] Client ${socket.id} joined contract:${data.contractId}`
        );
      }
      if (data.txHash) {
        socket.join(`tx:${data.txHash}`);
        console.log(`[WebSocket] Client ${socket.id} joined tx:${data.txHash}`);
      }
    });

    // Leave rooms
    socket.on("leave", (data: { contractId?: string; txHash?: string }) => {
      if (data.contractId) {
        socket.leave(`contract:${data.contractId}`);
      }
      if (data.txHash) {
        socket.leave(`tx:${data.txHash}`);
      }
    });

    // Subscribe to all events
    socket.on("subscribe:all", () => {
      socket.join("all-events");
      console.log(`[WebSocket] Client ${socket.id} subscribed to all events`);
    });

    // Unsubscribe from all events
    socket.on("unsubscribe:all", () => {
      socket.leave("all-events");
    });

    // Get recent events on demand
    socket.on(
      "get:recent",
      (data: { limit?: number; offset?: number }, callback?: Function) => {
        const limit = data.limit || 50;
        const offset = data.offset || 0;
        const events = eventIndexerService.getRecentEvents(limit, offset);

        if (callback) {
          callback({ success: true, data: events });
        }
      }
    );

    // Disconnect
    socket.on("disconnect", () => {
      console.log(`[WebSocket] Client disconnected: ${socket.id}`);
    });
  }

  /**
   * Broadcast event to subscribers
   */
  broadcastEvent(event: ContractEvent): void {
    if (!this.io) {
      console.warn("[WebSocket] Gateway not initialized, cannot broadcast");
      return;
    }

    const payload = {
      type: "contract_event",
      timestamp: new Date().toISOString(),
      data: event,
    };

    // Broadcast to all subscribers
    this.io.to("all-events").emit("event", payload);

    // Broadcast to contract-specific room
    this.io.to(`contract:${event.contractId}`).emit("event", {
      ...payload,
      type: "contract_event_specific",
    });

    // Broadcast to transaction-specific room
    this.io.to(`tx:${event.transactionHash}`).emit("event", {
      ...payload,
      type: "transaction_event",
    });

    console.log(
      `[WebSocket] Broadcasted event: ${event.eventType} (ledger ${event.ledgerSequence})`
    );
  }

  /**
   * Broadcast escrow-specific event (parsed)
   */
  broadcastEscrowEvent(escrowEvent: any): void {
    if (!this.io) return;

    const payload = {
      type: "escrow_event",
      timestamp: new Date().toISOString(),
      data: escrowEvent,
    };

    // Broadcast to escrow room
    this.io.to(`escrow:${escrowEvent.escrowId}`).emit("event", payload);

    // Also broadcast to mentor and learner rooms if available
    if (escrowEvent.mentor) {
      this.io.to(`user:${escrowEvent.mentor}`).emit("event", payload);
    }
    if (escrowEvent.learner) {
      this.io.to(`user:${escrowEvent.learner}`).emit("event", payload);
    }

    console.log(
      `[WebSocket] Broadcasted escrow event: ${escrowEvent.eventType} (escrow #${escrowEvent.escrowId})`
    );
  }

  /**
   * Send error notification
   */
  broadcastError(error: {
    code: string;
    message: string;
    details?: any;
  }): void {
    if (!this.io) return;

    this.io.emit("error", {
      type: "system_error",
      timestamp: new Date().toISOString(),
      data: error,
    });
  }

  /**
   * Close WebSocket server
   */
  close(): void {
    if (this.io) {
      this.io.close();
      this.io = null;
      console.log("[WebSocket] Gateway closed");
    }
  }
}

export const webSocketGateway = new WebSocketGateway();
