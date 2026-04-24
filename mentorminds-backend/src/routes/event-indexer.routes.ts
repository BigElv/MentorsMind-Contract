import { Router } from "express";
import { eventIndexerController } from "../controllers/event-indexer.controller";

const router = Router();

/**
 * Event Indexer Routes
 *
 * Base path: /api/events
 */

// Get recent events with pagination
router.get("/", (req, res) => eventIndexerController.getRecentEvents(req, res));

// Get cursor state (for debugging/monitoring)
router.get("/cursor", (req, res) =>
  eventIndexerController.getCursorState(req, res)
);

// Get events by contract ID
router.get("/contract/:contractId", (req, res) =>
  eventIndexerController.getEventsByContract(req, res)
);

// Get events by transaction hash
router.get("/tx/:txHash", (req, res) =>
  eventIndexerController.getEventsByTxHash(req, res)
);

// Get escrow-specific events
router.get("/escrow/:escrowId", (req, res) =>
  eventIndexerController.getEscrowEvents(req, res)
);

// Fetch historical events for an account
router.get("/account/:accountId", (req, res) =>
  eventIndexerController.getAccountEvents(req, res)
);

// Control streaming
router.post("/stream/start", (req, res) =>
  eventIndexerController.startStreaming(req, res)
);
router.post("/stream/stop", (req, res) =>
  eventIndexerController.stopStreaming(req, res)
);

export const eventIndexerRoutes = router;
