const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? '/api/v1';

export interface DashboardStats {
  totalVolume: {
    count: number;
    xlmValue: string;
  };
  successRate: number;
  feeMetrics: {
    average: string;
    total: string;
  };
  assetDistribution: Record<string, number>;
  escrowMetrics: {
    total: number;
    disputeRate: number;
  };
}

export async function fetchDashboardStats(): Promise<DashboardStats> {
  const res = await fetch(`${API_BASE}/analytics/stats`);
  if (!res.ok) {
    throw new Error('Failed to fetch analytics stats');
  }
  return res.json();
}

export async function downloadAnalyticsReport() {
  const res = await fetch(`${API_BASE}/analytics/report`);
  if (!res.ok) {
    throw new Error('Failed to download report');
  }
  const blob = await res.blob();
  const url = window.URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `blockchain-analytics-${new Date().toISOString().split('T')[0]}.csv`;
  document.body.appendChild(a);
  a.click();
  a.remove();
}
