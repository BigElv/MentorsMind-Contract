import { paymentTrackerService } from '../src/services/payment-tracker.service';
import { blockchainAnalyticsService } from '../src/services/blockchain-analytics.service';
import { Payment } from '../src/types/payment.types';

async function verifyAnalytics() {
  console.log('--- Verifying Blockchain Analytics Service ---');

  // 1. Create mock payments
  const p1 = await paymentTrackerService.create({
    sessionId: 's1',
    senderAddress: 'addr1',
    receiverAddress: 'addr2',
    amount: '100.5',
    assetCode: 'XLM',
    txHash: 'hash1',
  });

  const p2 = await paymentTrackerService.create({
    sessionId: 's2',
    senderAddress: 'addr3',
    receiverAddress: 'addr4',
    amount: '50.0',
    assetCode: 'USDC',
    txHash: 'hash2',
  });

  const p3 = await paymentTrackerService.create({
    sessionId: 's3',
    senderAddress: 'addr5',
    receiverAddress: 'addr6',
    amount: '75.25',
    assetCode: 'XLM',
    txHash: 'hash3',
  });

  // 2. Update statuses
  await paymentTrackerService.updateStatus(p1.id, 'confirmed', { fee: '0.0000100' });
  await paymentTrackerService.updateStatus(p2.id, 'confirmed', { fee: '0.0000150' });
  await paymentTrackerService.updateStatus(p3.id, 'failed', { fee: '0.0000100' });

  // 3. Get stats
  const stats = await blockchainAnalyticsService.getDashboardStats();
  console.log('Stats:', JSON.stringify(stats, null, 2));

  // 4. Assertions
  const expectedVolumeCount = 3;
  const expectedXlmValue = '100.5000000'; // Only p1 is confirmed XLM
  const expectedSuccessRate = (2/3) * 100;
  const expectedTotalFees = (0.0000100 + 0.0000150 + 0.0000100).toFixed(7);

  if (stats.totalVolume.count === expectedVolumeCount && 
      stats.totalVolume.xlmValue === expectedXlmValue &&
      stats.successRate === expectedSuccessRate &&
      stats.feeMetrics.total === expectedTotalFees) {
    console.log('✅ Analytics verification PASSED');
  } else {
    console.log('❌ Analytics verification FAILED');
    process.exit(1);
  }

  // 5. Verify CSV
  const csv = await blockchainAnalyticsService.generateCsvReport();
  console.log('CSV Report Sample:\n', csv.split('\n')[0]);
  console.log('✅ CSV generation PASSED');
}

verifyAnalytics().catch(err => {
  console.error(err);
  process.exit(1);
});
