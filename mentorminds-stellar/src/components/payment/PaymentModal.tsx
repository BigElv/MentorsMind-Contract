import { useEffect, useMemo, useState, type CSSProperties } from 'react';

import { TransactionBuilderService } from '../../services/transaction.builder';
import type {
  SupportedAssetCode,
  TransactionSimulationResult,
  TransactionSubmissionResult,
} from '../../types/transaction.types';
import type { StellarNetwork } from '../../types/stellar.types';

type PaymentModalProps = {
  isOpen: boolean;
  sourceAccount: string;
  destinationAccount: string;
  amount: string;
  assetCode: SupportedAssetCode;
  sessionId?: string;
  network?: StellarNetwork;
  horizonUrl?: string;
  onCancel: () => void;
  onConfirm?: (result: TransactionSubmissionResult) => void;
};

function buildTransactionService({
  sourceAccount,
  destinationAccount,
  amount,
  assetCode,
  sessionId,
  network,
  horizonUrl,
}: Omit<PaymentModalProps, 'isOpen' | 'onCancel' | 'onConfirm'>): TransactionBuilderService {
  const builder = new TransactionBuilderService()
    .from(sourceAccount)
    .to(destinationAccount)
    .amount(amount)
    .asset(assetCode)
    .withRetryPolicy({
      maxAttempts: 3,
      initialDelayMs: 1500,
      backoffMultiplier: 2,
    });

  if (sessionId) {
    builder.memo(sessionId);
  }

  if (network) {
    builder.onNetwork(network);
  }

  if (horizonUrl) {
    builder.withHorizonUrl(horizonUrl);
  }

  return builder;
}

function truncateMiddle(value: string, visibleChars = 6): string {
  if (value.length <= visibleChars * 2) {
    return value;
  }

  return `${value.slice(0, visibleChars)}...${value.slice(-visibleChars)}`;
}

export default function PaymentModal(props: PaymentModalProps) {
  const {
    isOpen,
    sourceAccount,
    destinationAccount,
    amount,
    assetCode,
    sessionId,
    network,
    horizonUrl,
    onCancel,
    onConfirm,
  } = props;

  const [simulating, setSimulating] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [simulation, setSimulation] = useState<TransactionSimulationResult | null>(null);
  const [result, setResult] = useState<TransactionSubmissionResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isOpen) {
      setSimulating(false);
      setSubmitting(false);
      setSimulation(null);
      setResult(null);
      setError(null);
      return;
    }

    let active = true;

    async function runDryRun() {
      setSimulating(true);
      setSubmitting(false);
      setResult(null);
      setError(null);

      const nextSimulation = await buildTransactionService({
        sourceAccount,
        destinationAccount,
        amount,
        assetCode,
        sessionId,
        network,
        horizonUrl,
      }).simulateTransaction();

      if (!active) {
        return;
      }

      setSimulation(nextSimulation);
      if (!nextSimulation.success) {
        setError(nextSimulation.error ?? 'Dry-run failed');
      }
      setSimulating(false);
    }

    runDryRun();

    return () => {
      active = false;
    };
  }, [amount, assetCode, destinationAccount, horizonUrl, isOpen, network, sessionId, sourceAccount]);

  const canConfirm = useMemo(() => {
    return Boolean(simulation?.success) && !simulating && !submitting && !result;
  }, [result, simulation, simulating, submitting]);

  async function handlePay() {
    try {
      setSubmitting(true);
      setError(null);

      const submission = await buildTransactionService({
        sourceAccount,
        destinationAccount,
        amount,
        assetCode,
        sessionId,
        network,
        horizonUrl,
      }).signAndSubmit();

      setResult(submission);
      onConfirm?.(submission);
    } catch (submissionError) {
      setError(submissionError instanceof Error ? submissionError.message : 'Transaction submission failed');
    } finally {
      setSubmitting(false);
    }
  }

  if (!isOpen) {
    return null;
  }

  return (
    <div role="dialog" aria-modal="true" style={styles.backdrop}>
      <div style={styles.modal}>
        <h3 style={styles.title}>Confirm Stellar Payment</h3>

        <div style={styles.section}>
          <p style={styles.subtitle}>Payment Summary</p>
          <p>
            <strong>
              {amount} {assetCode}
            </strong>
          </p>
          <p>From: {truncateMiddle(sourceAccount)}</p>
          <p>To: {truncateMiddle(destinationAccount)}</p>
          {sessionId && <p>Session Memo: {sessionId}</p>}
        </div>

        {simulating && <p>Running Horizon dry-run checks...</p>}

        {!simulating && simulation && (
          <>
            <div style={styles.section}>
              <p style={styles.subtitle}>Dry-Run Result</p>
              <p>
                Estimated Fee: <strong>{simulation.fee.suggestedFeeXlm} XLM</strong>
              </p>
              {simulation.sourceBalance && <p>Source Balance: {simulation.sourceBalance}</p>}
              {simulation.destinationBalance && <p>Destination Balance: {simulation.destinationBalance}</p>}
              <p>Memo Type: {simulation.memoType}</p>
              {simulation.memoValue && <p>Memo Value: {truncateMiddle(simulation.memoValue, 10)}</p>}
            </div>

            {simulation.warnings.length > 0 && (
              <div style={styles.section}>
                <p style={styles.subtitle}>Warnings</p>
                <ul style={styles.list}>
                  {simulation.warnings.map((warning: string) => (
                    <li key={warning}>{warning}</li>
                  ))}
                </ul>
              </div>
            )}
          </>
        )}

        {result && (
          <div style={styles.successBox}>
            <p style={styles.successTitle}>Transaction submitted successfully.</p>
            <p>
              Horizon Hash: <strong>{result.hash}</strong>
            </p>
            {result.ledger && <p>Ledger: {result.ledger}</p>}
            {result.explorerUrl && (
              <p>
                <a href={result.explorerUrl} target="_blank" rel="noreferrer">
                  View on Stellar Expert
                </a>
              </p>
            )}
          </div>
        )}

        {error && (
          <p role="alert" style={styles.error}>
            {error}
          </p>
        )}

        <div style={styles.actions}>
          <button type="button" onClick={onCancel} disabled={submitting}>
            {result ? 'Close' : 'Cancel'}
          </button>
          <button type="button" disabled={!canConfirm} onClick={handlePay}>
            {submitting ? 'Waiting for Freighter...' : 'Sign & Submit'}
          </button>
        </div>
      </div>
    </div>
  );
}

const styles: Record<string, CSSProperties> = {
  backdrop: {
    position: 'fixed',
    inset: 0,
    background: 'rgba(15, 23, 42, 0.45)',
    display: 'grid',
    placeItems: 'center',
    zIndex: 1000,
    padding: 20,
  },
  modal: {
    width: 'min(640px, 94vw)',
    borderRadius: 16,
    background: '#ffffff',
    padding: 24,
    boxShadow: '0 24px 60px rgba(15, 23, 42, 0.2)',
  },
  title: {
    marginTop: 0,
    marginBottom: 12,
  },
  subtitle: {
    marginTop: 0,
    marginBottom: 8,
    fontWeight: 700,
  },
  section: {
    marginTop: 14,
    padding: 12,
    borderRadius: 10,
    background: '#f8fafc',
  },
  list: {
    margin: 0,
    paddingLeft: 20,
  },
  successBox: {
    marginTop: 14,
    padding: 14,
    borderRadius: 10,
    background: '#ecfdf3',
    color: '#166534',
  },
  successTitle: {
    marginTop: 0,
    marginBottom: 8,
    fontWeight: 700,
  },
  actions: {
    marginTop: 20,
    display: 'flex',
    justifyContent: 'flex-end',
    gap: 10,
  },
  error: {
    marginTop: 12,
    color: '#b91c1c',
  },
};
