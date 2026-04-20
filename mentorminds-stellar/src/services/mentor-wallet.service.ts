const HORIZON_URL = process.env.NEXT_PUBLIC_HORIZON_URL ?? 'https://horizon-testnet.stellar.org';
const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? '/api/v1';

export type AssetCode = 'XLM' | 'USDC' | 'PYUSD';

export interface Balance {
  asset: AssetCode;
  amount: string;
}

export interface TransactionItem {
  id: string;
  txHash: string;
  createdAt: string;
  amount: string;
  asset: string;
  from: string;
  to: string;
  type: 'payment' | 'path_payment' | 'account_merge' | 'unknown';
}

function parseBalanceEntry(b: any): Balance | null {
  if (b.asset_type === 'native') {
    return { asset: 'XLM', amount: b.balance };
  }
  if (b.asset_code === 'USDC') {
    return { asset: 'USDC', amount: b.balance };
  }
  if (b.asset_code === 'PYUSD') {
    return { asset: 'PYUSD', amount: b.balance };
  }
  return null;
}

export async function fetchAccountBalances(address: string): Promise<Balance[]> {
  const res = await fetch(`${HORIZON_URL}/accounts/${address}`);
  if (!res.ok) {
    throw new Error('Failed to fetch account');
  }
  const data = await res.json();
  const balances = (data.balances ?? []).map(parseBalanceEntry).filter(Boolean) as Balance[];
  const codes = new Set<AssetCode>(['XLM', 'USDC', 'PYUSD']);
  const map = new Map<AssetCode, string>();
  for (const b of balances) {
    map.set(b.asset, b.amount);
  }
  for (const c of codes) {
    if (!map.has(c)) map.set(c, '0');
  }
  return [...map.entries()].map(([asset, amount]) => ({ asset, amount }));
}

export async function fetchTransactions(address: string, opts?: { asset?: AssetCode; startDate?: string; endDate?: string }): Promise<TransactionItem[]> {
  const url = new URL(`${HORIZON_URL}/accounts/${address}/payments`);
  url.searchParams.set('limit', '200');
  url.searchParams.set('order', 'desc');
  const res = await fetch(url.toString());
  if (!res.ok) {
    throw new Error('Failed to fetch payments');
  }
  const data = await res.json();
  const records: any[] = data._embedded?.records ?? [];
  const items: TransactionItem[] = records.map(r => {
    const amount = r.amount ?? r.starting_balance ?? '0';
    const asset = r.asset_code ?? 'XLM';
    const txHash = r.transaction_hash ?? r.id;
    const from = r.from ?? r.source_account ?? '';
    const to = r.to ?? r.account ?? '';
    return {
      id: r.id,
      txHash,
      createdAt: r.created_at,
      amount,
      asset,
      from,
      to,
      type: r.type === 'payment' ? 'payment' : r.type === 'path_payment_strict_send' || r.type === 'path_payment_strict_receive' ? 'path_payment' : r.type === 'account_merge' ? 'account_merge' : 'unknown',
    };
  });
  const filterByAsset = (item: TransactionItem) => (opts?.asset ? item.asset === opts.asset : true);
  const withinDates = (item: TransactionItem) => {
    const t = new Date(item.createdAt).getTime();
    if (opts?.startDate && t < new Date(opts.startDate).getTime()) return false;
    if (opts?.endDate && t > new Date(opts.endDate).getTime()) return false;
    return true;
  };
  return items.filter(i => filterByAsset(i) && withinDates(i));
}

export function streamPayments(address: string, onEvent: (payload: any) => void): () => void {
  const url = `${HORIZON_URL}/accounts/${address}/payments?cursor=now&stream=true`;
  const es = new EventSource(url);
  es.onmessage = ev => {
    try {
      const data = JSON.parse(ev.data);
      onEvent(data);
    } catch {}
  };
  es.onerror = () => {};
  return () => es.close();
}

export async function getEarningsSummary(address: string): Promise<{ pending: Record<string, string>; confirmed: Record<string, string> }> {
  const res = await fetch(`${API_BASE}/mentor-wallet/${address}/summary`);
  if (!res.ok) {
    return { pending: {}, confirmed: {} };
  }
  const data = await res.json();
  const toStringMap = (obj: Record<string, unknown>) =>
    Object.fromEntries(Object.entries(obj).map(([k, v]) => [k, typeof v === 'bigint' ? v.toString() : String(v as any)]));
  return {
    pending: toStringMap(data.pending ?? {}),
    confirmed: toStringMap(data.confirmed ?? {}),
  };
}

export async function createPayoutRequest(payload: { mentorAddress: string; amount: string; assetCode: AssetCode; destination?: string; note?: string }) {
  const res = await fetch(`${API_BASE}/mentor-wallet/payout-requests`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  if (!res.ok) {
    const e = await res.json().catch(() => ({}));
    throw new Error(e.error ?? 'Failed to create payout request');
  }
  return res.json();
}

export async function listPayoutRequests(address: string) {
  const res = await fetch(`${API_BASE}/mentor-wallet/${address}/payout-requests`);
  if (!res.ok) return [];
  return res.json();
}

export function connectActivityStream(address: string, onUpdate: (data: any) => void): () => void {
  const url = `${API_BASE}/mentor-wallet/stream/${address}`;
  const es = new EventSource(url);
  es.addEventListener('payment_update', ev => {
    try {
      onUpdate(JSON.parse((ev as MessageEvent).data));
    } catch {}
  });
  return () => es.close();
}

export function exportTransactionsCsv(rows: TransactionItem[]): string {
  const headers = ['Date', 'Type', 'Amount', 'Asset', 'From', 'To', 'TxHash'];
  const lines = rows.map(r =>
    [r.createdAt, r.type, r.amount, r.asset, r.from, r.to, r.txHash].map(v => `"${String(v ?? '').replace(/"/g, '""')}"`).join(',')
  );
  return [headers.join(','), ...lines].join('\n');
}

