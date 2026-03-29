export const HORIZON_CONFIG = {
  endpoints: [
    'https://horizon.stellar.org',
    'https://horizon-public.stellar.org',
  ],
  checkIntervalMs: 30000,
  maxStallDurationMs: 60000,
  ledgerDriftLimit: 300, // 5 minutes
  congestionCapacityThreshold: 0.9,
};
