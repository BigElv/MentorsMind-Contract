'use client';

import React, { useEffect, useState } from 'react';

interface NetworkStatusData {
  activeEndpoint: string;
  isHealthy: boolean;
  lastLedgerSequence: number;
  lastLedgerTime: string;
  isStalled: boolean;
  isCongested: boolean;
  latencyMs: number;
}

/**
 * NetworkStatus component displays the real-time health of the Stellar network connection.
 * It periodically fetches data from the backend monitor and handles various network states.
 */
export default function NetworkStatus() {
  const [status, setStatus] = useState<NetworkStatusData | null>(null);
  const [error, setError] = useState<boolean>(false);

  useEffect(() => {
    const fetchStatus = async () => {
      try {
        const resp = await fetch('/api/v1/network/status');
        if (!resp.ok) throw new Error('Failed to fetch network status');
        const json = await resp.json();
        setStatus(json.data);
        setError(false);
      } catch (err) {
        console.error('Error fetching network status:', err);
        setError(true);
      }
    };

    fetchStatus();
    // Poll every 30 seconds to match the backend monitoring interval
    const interval = setInterval(fetchStatus, 30000);
    return () => clearInterval(interval);
  }, []);

  // Default state when loading or error occurs
  if (error || !status) {
    return (
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px', color: '#94a3b8', fontSize: '0.85rem' }}>
        <span style={{ 
          width: '10px', 
          height: '10px', 
          borderRadius: '50%', 
          background: '#cbd5e1',
          display: 'inline-block' 
        }} />
        <span>Network Status: offline</span>
      </div>
    );
  }

  // Determine status aesthetics based on health flags
  let statusColor = '#22c55e'; // Green (Healthy)
  let statusText = 'Healthy';
  let glowColor = 'rgba(34, 197, 94, 0.4)';

  if (!status.isHealthy) {
    statusColor = '#ef4444'; // Red (Error/Down)
    statusText = 'Degraded';
    glowColor = 'rgba(239, 68, 68, 0.4)';
  } else if (status.isStalled) {
    statusColor = '#f97316'; // Orange (Stalled)
    statusText = 'Stalled';
    glowColor = 'rgba(249, 115, 22, 0.4)';
  } else if (status.isCongested) {
    statusColor = '#eab308'; // Yellow (Congested)
    statusText = 'Congested';
    glowColor = 'rgba(234, 179, 8, 0.4)';
  }

  return (
    <div 
      title={`Active Endpoint: ${status.activeEndpoint}\nLedger: ${status.lastLedgerSequence}\nLatency: ${status.latencyMs}ms\nLast Updated: ${new Date(status.lastLedgerTime).toLocaleTimeString()}`}
      style={{ 
        display: 'flex', 
        alignItems: 'center', 
        gap: '12px', 
        padding: '6px 14px', 
        background: 'rgba(255, 255, 255, 0.05)', 
        border: '1px solid rgba(255, 255, 255, 0.1)',
        backdropFilter: 'blur(4px)',
        borderRadius: '30px',
        fontSize: '0.75rem',
        fontWeight: 600,
        color: '#f8fafc',
        transition: 'all 0.3s ease'
      }}
    >
      <div style={{ position: 'relative', display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
        <span style={{ 
          width: '8px', 
          height: '8px', 
          borderRadius: '50%', 
          background: statusColor,
          zIndex: 2
        }} />
        <span style={{ 
          position: 'absolute',
          width: '14px', 
          height: '14px', 
          borderRadius: '50%', 
          background: glowColor,
          animation: 'ping 2s cubic-bezier(0, 0, 0.2, 1) infinite',
          zIndex: 1
        }} />
      </div>
      
      <div style={{ display: 'flex', flexDirection: 'column', lineHeight: 1.1 }}>
        <span style={{ opacity: 0.9 }}>Network: {statusText}</span>
        <span style={{ fontSize: '0.65rem', opacity: 0.5 }}>#{status.lastLedgerSequence} ({status.latencyMs}ms)</span>
      </div>

      <style>{`
        @keyframes ping {
          75%, 100% {
            transform: scale(2);
            opacity: 0;
          }
        }
      `}</style>
    </div>
  );
}
