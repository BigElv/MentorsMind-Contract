export const horizonConfig = {
  primary: process.env.HORIZON_URL ?? 'https://horizon-testnet.stellar.org',
  backup: process.env.HORIZON_BACKUP_URL ?? 'https://horizon.stellar.org',
  healthyResponseTimeoutMs: 5000,
  stallThresholdLedgers: 10,
};
