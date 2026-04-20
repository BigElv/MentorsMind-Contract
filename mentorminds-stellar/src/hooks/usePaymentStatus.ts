import { useEffect, useRef, useState } from 'react';

type PaymentStatus = 'pending' | 'confirmed' | 'failed' | 'timeout';

interface PaymentState {
  status: PaymentStatus | null;
  txHash: string | null;
  ledgerSequence: number | null;
  errorCode: string | null;
  errorMessage: string | null;
  loading: boolean;
  error: string | null;
}

const POLL_INTERVAL_MS = 5_000;
const API_BASE = process.env.NEXT_PUBLIC_API_URL ?? '/api/v1';
const TERMINAL_STATUSES: PaymentStatus[] = ['confirmed', 'failed', 'timeout'];

export function usePaymentStatus(paymentId: string | null): PaymentState {
  const [state, setState] = useState<PaymentState>({
    status: null,
    txHash: null,
    ledgerSequence: null,
    errorCode: null,
    errorMessage: null,
    loading: false,
    error: null,
  });

  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    if (!paymentId) return;

    const fetchStatus = async () => {
      setState(prev => ({ ...prev, loading: true, error: null }));
      try {
        const res = await fetch(`${API_BASE}/payments/${paymentId}`);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);

        const data = await res.json();
        setState({
          status: data.status,
          txHash: data.txHash,
          ledgerSequence: data.ledgerSequence,
          errorCode: data.errorCode,
          errorMessage: data.errorMessage,
          loading: false,
          error: null,
        });

        if (TERMINAL_STATUSES.includes(data.status)) {
          clearInterval(intervalRef.current!);
        }
      } catch (err) {
        setState(prev => ({
          ...prev,
          loading: false,
          error: err instanceof Error ? err.message : 'Failed to fetch payment status',
        }));
      }
    };

    fetchStatus();
    intervalRef.current = setInterval(fetchStatus, POLL_INTERVAL_MS);

    return () => clearInterval(intervalRef.current!);
  }, [paymentId]);

  return state;
}
