/**
 * Boot-phase loading screen (bd-jit6pdwq Phase 2).
 *
 * Replaces the bare "Initializing q2-preview…" div. Three jobs:
 *
 * 1. Initializing copy, immediately.
 * 2. Retry visibility: the boot controller retries connect() without
 *    an attempt cap while the server is healthy — that must never look
 *    like a silent hang, so attempts past the first show a "retrying"
 *    line with the last failure.
 * 3. The Firefox hint: after `hintAfterMs` of waiting, explain the
 *    most likely localhost stall — Firefox serializes WebSocket
 *    opening handshakes per IP address across ALL tabs, so a stale
 *    preview tab with a hung server can block this one. See
 *    claude-notes/research/2026-06-11-firefox-ws-handshake-serialization.md
 */

import { useEffect, useState } from 'react';

const CONTAINER_STYLE: React.CSSProperties = {
  padding: 24,
  color: '#666',
  font: '14px -apple-system, Segoe UI, sans-serif',
  lineHeight: 1.5,
};

const HINT_STYLE: React.CSSProperties = {
  marginTop: 16,
  padding: '8px 12px',
  background: '#fff8e1',
  border: '1px solid #e0d4a8',
  borderRadius: 4,
  color: '#6b5d2e',
  maxWidth: 560,
};

export interface BootLoadingScreenProps {
  /** 1-based connect attempt from the boot controller. */
  attempt: number;
  /** The previous attempt's failure, if any. */
  lastError: Error | null;
  /** How long before the slow-connection hint appears. */
  hintAfterMs?: number;
}

export function BootLoadingScreen({
  attempt,
  lastError,
  hintAfterMs = 8000,
}: BootLoadingScreenProps) {
  const [showHint, setShowHint] = useState(false);

  useEffect(() => {
    const id = setTimeout(() => setShowHint(true), hintAfterMs);
    return () => clearTimeout(id);
  }, [hintAfterMs]);

  return (
    <div style={CONTAINER_STYLE}>
      <div>Initializing q2-preview…</div>
      {attempt > 1 && (
        <div style={{ marginTop: 8 }}>
          Retrying connection (attempt {attempt})
          {lastError ? <> — last error: {lastError.message}</> : null}
        </div>
      )}
      {showHint && (
        <div style={HINT_STYLE}>
          Still connecting. If this persists, another tab may be holding a
          stuck WebSocket connection to a local server — Firefox allows only
          one connecting WebSocket per host across all tabs. Closing stale
          preview tabs (or tabs pointing at stopped local servers) usually
          unblocks this.
        </div>
      )}
    </div>
  );
}
