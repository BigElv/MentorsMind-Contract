import React, { useEffect, useState } from 'react';
import { fetchDashboardStats, downloadAnalyticsReport, DashboardStats } from '../../services/analytics.service';

const BlockchainAnalytics: React.FC = () => {
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    loadStats();
  }, []);

  const loadStats = async () => {
    try {
      setLoading(true);
      const data = await fetchDashboardStats();
      setStats(data);
      setError(null);
    } catch (err: any) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  };

  if (loading) return <div className="p-8 text-center text-gray-400">Loading Analytics...</div>;
  if (error) return <div className="p-8 text-center text-red-500">Error: {error}</div>;
  if (!stats) return null;

  return (
    <div className="p-8 bg-gray-900 min-h-screen text-white font-sans">
      <div className="flex justify-between items-center mb-10">
        <div>
          <h1 className="text-4xl font-extrabold bg-clip-text text-transparent bg-gradient-to-r from-blue-400 to-emerald-400">
            Blockchain Analytics
          </h1>
          <p className="text-gray-400 mt-2 text-lg">Real-time network metrics and performance tracking</p>
        </div>
        <button
          onClick={downloadAnalyticsReport}
          className="px-6 py-3 bg-gradient-to-r from-blue-600 to-blue-500 hover:from-blue-500 hover:to-blue-400 text-white rounded-xl shadow-lg transition-all transform hover:scale-105 font-semibold flex items-center gap-2"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M4 16v1a2 2 0 002 2h12a2 2 0 002-2v-1M7 10l5 5m0 0l5-5m-5 5V3" />
          </svg>
          Export CSV Report
        </button>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-6 mb-10">
        <MetricCard
          title="Total Transactions"
          value={stats.totalVolume.count.toLocaleString()}
          subtitle={`${stats.totalVolume.xlmValue} XLM Volume`}
          icon={<TxIcon />}
          color="from-blue-500/20 to-blue-600/5"
        />
        <MetricCard
          title="Success Rate"
          value={`${stats.successRate.toFixed(1)}%`}
          subtitle="Confirmed Transactions"
          icon={<CheckIcon />}
          color="from-emerald-500/20 to-emerald-600/5"
        />
        <MetricCard
          title="Total Fees Paid"
          value={`${stats.feeMetrics.total} XLM`}
          subtitle={`Avg: ${stats.feeMetrics.average} XLM`}
          icon={<FeeIcon />}
          color="from-purple-500/20 to-purple-600/5"
        />
        <MetricCard
          title="Escrow Usage"
          value={stats.escrowMetrics.total.toString()}
          subtitle={`Dispute Rate: ${stats.escrowMetrics.disputeRate}%`}
          icon={<EscrowIcon />}
          color="from-amber-500/20 to-amber-600/5"
        />
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-8">
        <div className="bg-gray-800/50 backdrop-blur-xl p-8 rounded-3xl border border-gray-700 shadow-2xl">
          <h2 className="text-2xl font-bold mb-6 flex items-center gap-2">
            <span className="w-2 h-8 bg-blue-500 rounded-full"></span>
            Asset Distribution
          </h2>
          <div className="space-y-6">
            {Object.entries(stats.assetDistribution).map(([asset, count]) => (
              <div key={asset} className="flex items-center gap-4">
                <div className="w-16 font-mono text-blue-400 font-bold">{asset}</div>
                <div className="flex-1 bg-gray-700 h-4 rounded-full overflow-hidden">
                  <div
                    className="bg-gradient-to-r from-blue-500 to-emerald-500 h-full rounded-full transition-all duration-1000"
                    style={{ width: `${(count / stats.totalVolume.count) * 100}%` }}
                  ></div>
                </div>
                <div className="w-12 text-right text-gray-400 font-medium">{count}</div>
              </div>
            ))}
          </div>
        </div>

        <div className="bg-gray-800/50 backdrop-blur-xl p-8 rounded-3xl border border-gray-700 shadow-2xl flex flex-col justify-center items-center text-center">
          <div className="w-20 h-20 bg-blue-500/20 rounded-full flex items-center justify-center mb-4">
            <svg className="w-10 h-10 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M13 7h8m0 0v8m0-8l-8 8-4-4-6 6" />
            </svg>
          </div>
          <h2 className="text-2xl font-bold mb-2">Detailed Trends</h2>
          <p className="text-gray-400 max-w-sm">
            Interactive time-series charts for transaction volume and fee fluctuations are coming in the next update.
          </p>
        </div>
      </div>
    </div>
  );
};

const MetricCard: React.FC<{ title: string; value: string; subtitle: string; icon: React.ReactNode; color: string }> = ({
  title,
  value,
  subtitle,
  icon,
  color,
}) => (
  <div className={`bg-gradient-to-br ${color} p-6 rounded-3xl border border-white/5 backdrop-blur-sm shadow-xl hover:shadow-2xl transition-all duration-300 border-gray-800`}>
    <div className="flex justify-between items-start mb-4">
      <div className="p-3 bg-black/30 rounded-2xl">{icon}</div>
    </div>
    <h3 className="text-gray-400 text-sm font-medium uppercase tracking-wider">{title}</h3>
    <div className="text-3xl font-extrabold my-1">{value}</div>
    <div className="text-gray-500 text-sm font-medium">{subtitle}</div>
  </div>
);

const TxIcon = () => (
  <svg className="w-6 h-6 text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
  </svg>
);

const CheckIcon = () => (
  <svg className="w-6 h-6 text-emerald-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M5 13l4 4L19 7" />
  </svg>
);

const FeeIcon = () => (
  <svg className="w-6 h-6 text-purple-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M12 8c-1.657 0-3 1.343-3 3s1.343 3 3 3 3-1.343 3-3-1.343-3-3-3zM12 2C6.477 2 2 6.477 2 12s4.477 10 10 10 10-4.477 10-10S17.523 2 12 2z" />
  </svg>
);

const EscrowIcon = () => (
  <svg className="w-6 h-6 text-amber-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
  </svg>
);

export default BlockchainAnalytics;
