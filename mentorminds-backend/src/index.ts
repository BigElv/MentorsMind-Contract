import express from "express";
import { createServer } from "http";
import cors from "cors";
import dotenv from "dotenv";

// Import services
import { webSocketGateway } from "./services/websocket-gateway";
import { horizonStreamService } from "./services/horizon-stream.service";
import { eventIndexerService } from "./services/event-indexer.service";
import { eventIndexerRoutes } from "./routes/event-indexer.routes";
import paymentRoutes from "./routes/payment.routes";
import mentorWalletRoutes from "./routes/mentor-wallet.routes";
import auditLogRoutes from "./routes/audit-log.routes";

// Load environment variables
dotenv.config();

const app = express();
const httpServer = createServer(app);
const PORT = process.env.PORT || 3001;

// Middleware
app.use(
  cors({
    origin: process.env.FRONTEND_URL || "*",
    methods: ["GET", "POST", "PUT", "DELETE", "OPTIONS"],
    allowedHeaders: ["Content-Type", "Authorization"],
  })
);
app.use(express.json());
app.use(express.urlencoded({ extended: true }));

// Routes
app.use("/api/events", eventIndexerRoutes);
app.use("/api/payments", paymentRoutes);
app.use("/api/mentor-wallet", mentorWalletRoutes);
app.use("/api/audit-logs", auditLogRoutes);

// Health check endpoint
app.get("/health", (req, res) => {
  const cursor = eventIndexerService.getCursorState();
  const timeSinceLastLedger = Date.now() - cursor.updatedAt.getTime();

  res.json({
    status: "ok",
    uptime: process.uptime(),
    timestamp: new Date().toISOString(),
    indexer: {
      status: timeSinceLastLedger < 60000 ? "healthy" : "degraded",
      lastLedger: cursor.lastLedger,
      lastUpdate: cursor.updatedAt,
    },
  });
});

// Root endpoint
app.get("/", (req, res) => {
  res.json({
    name: "MentorMinds Backend API",
    version: "1.0.0",
    endpoints: {
      events: "/api/events",
      payments: "/api/payments",
      mentorWallet: "/api/mentor-wallet",
      auditLogs: "/api/audit-logs",
      health: "/health",
      websocket: "ws://localhost:" + PORT + "/ws/events",
    },
  });
});

// Initialize WebSocket gateway
webSocketGateway.init(httpServer);

// Subscribe to event notifications and broadcast via WebSocket
eventIndexerService.subscribe((event) => {
  // Broadcast to all WebSocket subscribers
  webSocketGateway.broadcastEvent(event);

  // Additional processing can be added here
  console.log(
    `[Event] Processed: ${event.eventType} for contract ${event.contractId}`
  );
});

// Error handling middleware
app.use(
  (
    err: any,
    req: express.Request,
    res: express.Response,
    next: express.NextFunction
  ) => {
    console.error("[Error]", err.stack);
    res.status(500).json({
      success: false,
      error:
        process.env.NODE_ENV === "production"
          ? "Internal server error"
          : err.message,
    });
  }
);

// 404 handler
app.use((req, res) => {
  res.status(404).json({
    success: false,
    error: "Not found",
  });
});

// Start server
httpServer.listen(PORT, () => {
  console.log("=".repeat(60));
  console.log("MentorMinds Backend Server Started");
  console.log("=".repeat(60));
  console.log(`Environment: ${process.env.NODE_ENV || "development"}`);
  console.log(`Port: ${PORT}`);
  console.log(`REST API: http://localhost:${PORT}`);
  console.log(`WebSocket: ws://localhost:${PORT}/ws/events`);
  console.log(`Health Check: http://localhost:${PORT}/health`);
  console.log("=".repeat(60));

  // Start Horizon streaming after server is ready
  setTimeout(() => {
    console.log("Starting Horizon event streaming...");
    horizonStreamService.startStreaming().catch((err) => {
      console.error("[Startup] Failed to start streaming:", err);

      // Notify via WebSocket if available
      webSocketGateway.broadcastError({
        code: "STREAM_START_FAILED",
        message: "Failed to connect to Horizon",
        details: err.message,
      });
    });
  }, 2000);
});

// Graceful shutdown
process.on("SIGTERM", () => {
  console.log("SIGTERM received, shutting down gracefully...");

  horizonStreamService.stopStreaming();
  webSocketGateway.close();

  httpServer.close(() => {
    console.log("Server closed");
    process.exit(0);
  });
});

process.on("SIGINT", () => {
  console.log("SIGINT received, shutting down gracefully...");

  horizonStreamService.stopStreaming();
  webSocketGateway.close();

  httpServer.close(() => {
    console.log("Server closed");
    process.exit(0);
  });
});

export default app;
