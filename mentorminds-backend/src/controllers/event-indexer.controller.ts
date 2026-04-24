import { Request, Response } from "express";
import { eventIndexerService } from "../services/event-indexer.service";
import { horizonStreamService } from "../services/horizon-stream.service";
import { ContractEvent } from "../types/event-indexer.types";

export class EventIndexerController {
  /**
   * Get recent contract events with pagination
   * GET /api/events?limit=50&offset=0
   */
  async getRecentEvents(req: Request, res: Response): Promise<void> {
    try {
      const limit = parseInt(req.query.limit as string) || 50;
      const offset = parseInt(req.query.offset as string) || 0;

      const events = eventIndexerService.getRecentEvents(limit, offset);

      res.json({
        success: true,
        data: events,
        pagination: {
          limit,
          offset,
          total: events.length,
        },
      });
    } catch (error) {
      console.error(
        "[EventIndexerController] Error getting recent events:",
        error
      );
      res.status(500).json({
        success: false,
        error: "Failed to fetch events",
      });
    }
  }

  /**
   * Get events by contract ID
   * GET /api/events/contract/:contractId
   */
  async getEventsByContract(req: Request, res: Response): Promise<void> {
    try {
      const { contractId } = req.params;

      const events = eventIndexerService.getEventsByContract(contractId);

      res.json({
        success: true,
        data: events,
        count: events.length,
      });
    } catch (error) {
      console.error(
        "[EventIndexerController] Error getting events by contract:",
        error
      );
      res.status(500).json({
        success: false,
        error: "Failed to fetch contract events",
      });
    }
  }

  /**
   * Get events by transaction hash
   * GET /api/events/tx/:txHash
   */
  async getEventsByTxHash(req: Request, res: Response): Promise<void> {
    try {
      const { txHash } = req.params;

      const events = eventIndexerService.getEventsByTxHash(txHash);

      res.json({
        success: true,
        data: events,
        count: events.length,
      });
    } catch (error) {
      console.error(
        "[EventIndexerController] Error getting events by tx hash:",
        error
      );
      res.status(500).json({
        success: false,
        error: "Failed to fetch transaction events",
      });
    }
  }

  /**
   * Get cursor state (for debugging)
   * GET /api/events/cursor
   */
  async getCursorState(req: Request, res: Response): Promise<void> {
    try {
      const cursor = eventIndexerService.getCursorState();

      res.json({
        success: true,
        data: cursor,
      });
    } catch (error) {
      console.error(
        "[EventIndexerController] Error getting cursor state:",
        error
      );
      res.status(500).json({
        success: false,
        error: "Failed to fetch cursor state",
      });
    }
  }

  /**
   * Start streaming events from Horizon
   * POST /api/events/stream/start
   */
  async startStreaming(req: Request, res: Response): Promise<void> {
    try {
      await horizonStreamService.startStreaming();

      res.json({
        success: true,
        message: "Event streaming started",
      });
    } catch (error) {
      console.error("[EventIndexerController] Error starting stream:", error);
      res.status(500).json({
        success: false,
        error: "Failed to start streaming",
      });
    }
  }

  /**
   * Stop streaming events
   * POST /api/events/stream/stop
   */
  async stopStreaming(req: Request, res: Response): Promise<void> {
    try {
      horizonStreamService.stopStreaming();

      res.json({
        success: true,
        message: "Event streaming stopped",
      });
    } catch (error) {
      console.error("[EventIndexerController] Error stopping stream:", error);
      res.status(500).json({
        success: false,
        error: "Failed to stop streaming",
      });
    }
  }

  /**
   * Fetch historical events for an account
   * GET /api/events/account/:accountId
   */
  async getAccountEvents(req: Request, res: Response): Promise<void> {
    try {
      const { accountId } = req.params;
      const limit = parseInt(req.query.limit as string) || 200;

      const events = await horizonStreamService.fetchAccountEvents(
        accountId,
        limit
      );

      res.json({
        success: true,
        data: events,
        count: events.length,
      });
    } catch (error) {
      console.error(
        "[EventIndexerController] Error fetching account events:",
        error
      );
      res.status(500).json({
        success: false,
        error: "Failed to fetch account events",
      });
    }
  }

  /**
   * Get escrow-specific events (parsed)
   * GET /api/events/escrow/:escrowId
   */
  async getEscrowEvents(req: Request, res: Response): Promise<void> {
    try {
      const { escrowId } = req.params;

      // Get all events and filter for escrow-related ones
      const allEvents = eventIndexerService.getRecentEvents(1000, 0);
      const escrowEvents = allEvents.filter((e) => {
        // Check if event data contains escrow ID
        return (
          e.dataJson?.escrowId?.toString() === escrowId ||
          e.topicJson?.some((t: any) => t?.toString().includes(escrowId))
        );
      });

      res.json({
        success: true,
        data: escrowEvents,
        count: escrowEvents.length,
      });
    } catch (error) {
      console.error(
        "[EventIndexerController] Error getting escrow events:",
        error
      );
      res.status(500).json({
        success: false,
        error: "Failed to fetch escrow events",
      });
    }
  }
}

export const eventIndexerController = new EventIndexerController();
