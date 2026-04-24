/**
 * Test Suite: StellarMonitorService - Single Sync Mechanism
 * 
 * This test verifies that only one synchronization mechanism (stream OR poll)
 * is active at any given time to prevent duplicate DB updates.
 * 
 * Run with: npm test or jest stellar-monitor.service.test.ts
 */

describe('StellarMonitorService - Single Sync Mechanism', () => {
  let stellarMonitorService: any;
  let mockSetInterval: jest.Mock;
  let mockClearInterval: jest.Mock;
  let mockConsoleLog: jest.SpyInstance;
  let mockConsoleError: jest.SpyInstance;

  beforeEach(() => {
    // Mock timers
    mockSetInterval = jest.fn(() => 'timer-id');
    mockClearInterval = jest.fn();
    global.setInterval = mockSetInterval;
    global.clearInterval = mockClearInterval;

    // Mock console
    mockConsoleLog = jest.spyOn(console, 'log').mockImplementation(() => {});
    mockConsoleError = jest.spyOn(console, 'error').mockImplementation(() => {});

    // Clear module cache to get fresh instance
    jest.resetModules();
    const module = require('./stellar-monitor.service');
    stellarMonitorService = module.stellarMonitorService;
  });

  afterEach(() => {
    jest.restoreAllMocks();
    stellarMonitorService?.stopMonitoring();
  });

  describe('Stream Success Scenario', () => {
    it('should only have stream active when streaming succeeds', async () => {
      await stellarMonitorService.startPendingEscrowMonitoring();

      // Wait for async stream initialization
      await new Promise(resolve => setTimeout(resolve, 150));

      expect(stellarMonitorService.isStreamActive()).toBe(true);
      expect(stellarMonitorService.isPollingActive()).toBe(false);
      
      // Verify polling was NOT started
      expect(mockSetInterval).not.toHaveBeenCalled();
    });

    it('should not start polling manually when stream is active', async () => {
      await stellarMonitorService.startPendingEscrowMonitoring();
      await new Promise(resolve => setTimeout(resolve, 150));

      // Attempt to start polling
      stellarMonitorService.startPendingEscrowPolling();

      expect(stellarMonitorService.isStreamActive()).toBe(true);
      expect(stellarMonitorService.isPollingActive()).toBe(false);
      expect(mockSetInterval).not.toHaveBeenCalled();
    });
  });

  describe('Stream Failure & Fallback Scenario', () => {
    it('should fallback to polling when stream fails', async () => {
      // Mock stream to fail
      jest.spyOn(stellarMonitorService, 'startStreamPendingEscrows').mockRejectedValue(
        new Error('Stream connection failed')
      );

      await stellarMonitorService.startPendingEscrowMonitoring();
      await new Promise(resolve => setTimeout(resolve, 150));

      expect(stellarMonitorService.isStreamActive()).toBe(false);
      expect(stellarMonitorService.isPollingActive()).toBe(true);
      
      // Verify polling WAS started as fallback
      expect(mockSetInterval).toHaveBeenCalled();
      expect(mockConsoleError).toHaveBeenCalledWith(
        'Stream failed, falling back to polling:',
        expect.any(Error)
      );
    });
  });

  describe('Prevention of Duplicate Processing', () => {
    it('should skip polling when stream is active', async () => {
      await stellarMonitorService.startPendingEscrowMonitoring();
      await new Promise(resolve => setTimeout(resolve, 150));

      const processTransactionSpy = jest.spyOn(stellarMonitorService, 'processTransaction');

      // Manually trigger poll (should be skipped)
      await stellarMonitorService.pollPending();

      // Poll should skip processing because stream is active
      expect(processTransactionSpy).not.toHaveBeenCalled();
      expect(mockConsoleLog).toHaveBeenCalledWith('Skipping poll: stream is active');
    });

    it('should not start polling interval if already running', async () => {
      jest.spyOn(stellarMonitorService, 'startStreamPendingEscrows').mockRejectedValue(
        new Error('Stream failed')
      );

      await stellarMonitorService.startPendingEscrowMonitoring();
      await new Promise(resolve => setTimeout(resolve, 150));

      const setIntervalCallCount = mockSetInterval.mock.calls.length;

      // Try to start polling again
      stellarMonitorService.startPendingEscrowPolling();

      // Should not create another interval
      expect(mockSetInterval.mock.calls.length).toBe(setIntervalCallCount);
      expect(mockConsoleLog).toHaveBeenCalledWith('Polling already running');
    });
  });

  describe('Stop Monitoring', () => {
    it('should stop all monitoring when stopMonitoring is called', async () => {
      await stellarMonitorService.startPendingEscrowMonitoring();
      await new Promise(resolve => setTimeout(resolve, 150));

      expect(stellarMonitorService.isStreamActive()).toBe(true);

      await stellarMonitorService.stopMonitoring();

      expect(stellarMonitorService.isStreamActive()).toBe(false);
      expect(stellarMonitorService.isPollingActive()).toBe(false);
    });

    it('should stop polling when stopMonitoring is called', async () => {
      jest.spyOn(stellarMonitorService, 'startStreamPendingEscrows').mockRejectedValue(
        new Error('Stream failed')
      );

      await stellarMonitorService.startPendingEscrowMonitoring();
      await new Promise(resolve => setTimeout(resolve, 150));

      expect(stellarMonitorService.isPollingActive()).toBe(true);

      await stellarMonitorService.stopMonitoring();

      expect(stellarMonitorService.isPollingActive()).toBe(false);
      expect(mockClearInterval).toHaveBeenCalled();
    });
  });

  describe('State Recovery', () => {
    it('should handle stream failure and allow restart', async () => {
      // First attempt: stream fails
      jest.spyOn(stellarMonitorService, 'startStreamPendingEscrows').mockRejectedValueOnce(
        new Error('Stream connection failed')
      );

      await stellarMonitorService.startPendingEscrowMonitoring();
      await new Promise(resolve => setTimeout(resolve, 150));

      expect(stellarMonitorService.isStreamActive()).toBe(false);
      expect(stellarMonitorService.isPollingActive()).toBe(true);

      // Stop everything
      await stellarMonitorService.stopMonitoring();

      expect(stellarMonitorService.isStreamActive()).toBe(false);
      expect(stellarMonitorService.isPollingActive()).toBe(false);

      // Second attempt: stream succeeds
      await stellarMonitorService.startPendingEscrowMonitoring();
      await new Promise(resolve => setTimeout(resolve, 150));

      expect(stellarMonitorService.isStreamActive()).toBe(true);
      expect(stellarMonitorService.isPollingActive()).toBe(false);
    });
  });
});
