import { useEffect, useMemo } from 'react';
import { useDiagnostics, diagSummary, type DiagEntry, type EndpointHealth } from '../diagnostics/store';

const LEVEL_COLORS: Record<string, string> = {
  info: '#4dd0e1',
  warn: '#ffb547',
  error: '#ff637d',
};

const STATUS_COLORS: Record<string, string> = {
  healthy: '#35d07f',
  degraded: '#ffb547',
  offline: '#ff637d',
  unknown: '#666',
};

function fmtTs(ts: number): string {
  if (!ts) return '—';
  const d = new Date(ts);
  return d.toLocaleTimeString('en-US', { hour12: false }) + '.' + String(d.getMilliseconds()).padStart(3, '0');
}

function age(ts: number): string {
  if (!ts) return '—';
  const s = Math.round((Date.now() - ts) / 1000);
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.round(s / 60)}m`;
  return `${Math.round(s / 3600)}h`;
}

export function DiagnosticsBadge() {
  const summary = useDiagnostics((s) => diagSummary(s));
  const sse = useDiagnostics((s) => s.sse);
  const endpoints = useDiagnostics((s) => s.endpoints);
  const toggle = useDiagnostics((s) => s.toggleOverlay);

  // Aggregate worst endpoint state
  const worstEndpoint = useMemo(() => {
    const order: Record<string, number> = { healthy: 0, unknown: 1, degraded: 2, offline: 3 };
    let worst = 'healthy';
    for (const e of Object.values(endpoints)) {
      if ((order[e.status] ?? 0) > (order[worst] ?? 0)) worst = e.status;
    }
    return worst;
  }, [endpoints]);

  // Combined level: take the worst of (entries, endpoints, sse)
  const level: 'good' | 'warn' | 'error' = useMemo(() => {
    if (summary.level === 'error' || worstEndpoint === 'offline') return 'error';
    if (summary.level === 'warn' || worstEndpoint === 'degraded' || !sse.connected) return 'warn';
    return 'good';
  }, [summary, worstEndpoint, sse.connected]);

  const dotColor = level === 'error' ? '#ff637d' : level === 'warn' ? '#ffb547' : '#35d07f';
  const text = level === 'error'
    ? `${summary.errorCount} ERR`
    : level === 'warn'
      ? `${summary.warnCount + summary.errorCount} warn`
      : 'healthy';

  return (
    <button
      onClick={toggle}
      title="Diagnostics overlay (Shift+D)"
      className="badge"
      style={{
        cursor: 'pointer',
        background: 'none',
        fontFamily: 'var(--mono)',
        color: dotColor,
        borderColor: dotColor + '60',
        display: 'flex',
        alignItems: 'center',
        gap: 4,
      }}
    >
      <span style={{
        display: 'inline-block',
        width: 6,
        height: 6,
        borderRadius: 3,
        background: dotColor,
        boxShadow: level !== 'good' ? `0 0 6px ${dotColor}` : 'none',
        animation: level === 'error' ? 'pulse 1.2s infinite' : undefined,
      }} />
      {text}
    </button>
  );
}

function EndpointRow({ ep }: { ep: EndpointHealth }) {
  return (
    <tr>
      <td>
        <span className="diag-dot" style={{ background: STATUS_COLORS[ep.status] }} />
        {ep.label}
      </td>
      <td className="diag-mono diag-muted">{ep.url}</td>
      <td className="diag-mono">{ep.lastStatus ? `HTTP ${ep.lastStatus}` : '—'}</td>
      <td className="diag-mono">{ep.lastDurationMs ? `${ep.lastDurationMs}ms` : '—'}</td>
      <td className="diag-mono diag-muted">{age(ep.lastChecked)}</td>
      <td style={{ color: ep.lastError ? '#ff637d' : '#888' }}>
        {ep.lastError ?? (ep.status === 'healthy' ? 'ok' : '')}
      </td>
    </tr>
  );
}

function EntryRow({ e }: { e: DiagEntry }) {
  return (
    <tr>
      <td className="diag-mono diag-muted">{fmtTs(e.ts)}</td>
      <td>
        <span className="diag-dot" style={{ background: LEVEL_COLORS[e.level] }} />
        <span style={{ color: LEVEL_COLORS[e.level], textTransform: 'uppercase', fontSize: 9 }}>{e.level}</span>
      </td>
      <td className="diag-mono">{e.source}</td>
      <td className="diag-mono">{e.category}</td>
      <td className="diag-mono">{e.field ?? e.endpoint ?? '—'}</td>
      <td>
        {e.message}
        {e.expected && (
          <div className="diag-mono diag-muted" style={{ fontSize: 10 }}>
            expected: <span style={{ color: '#35d07f' }}>{e.expected}</span> · received: <span style={{ color: '#ff637d' }}>{e.received}</span>
          </div>
        )}
      </td>
    </tr>
  );
}

export function DiagnosticsOverlay() {
  const open = useDiagnostics((s) => s.overlayOpen);
  const setOpen = useDiagnostics((s) => s.setOverlayOpen);
  const entries = useDiagnostics((s) => s.entries);
  const endpoints = useDiagnostics((s) => s.endpoints);
  const sse = useDiagnostics((s) => s.sse);
  const fieldNaN = useDiagnostics((s) => s.fieldNaNCount);
  const clear = useDiagnostics((s) => s.clear);
  const paused = useDiagnostics((s) => s.paused);
  const setPaused = useDiagnostics((s) => s.setPaused);

  // Keyboard shortcut: Shift+D opens/closes
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.shiftKey && (e.key === 'D' || e.key === 'd')) {
        e.preventDefault();
        setOpen(!open);
      } else if (e.key === 'Escape' && open) {
        setOpen(false);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, setOpen]);

  if (!open) return null;

  const recent = [...entries].reverse().slice(0, 200);
  const endpointList = Object.values(endpoints).sort((a, b) => a.label.localeCompare(b.label));
  const nanFields = Object.entries(fieldNaN).sort((a, b) => b[1] - a[1]);

  return (
    <div className="diag-overlay">
      <div className="diag-panel">
        <div className="diag-header">
          <div>
            <h2>Diagnostics — wiring map</h2>
            <small>Shift+D to toggle · Esc to close</small>
          </div>
          <div className="diag-header-actions">
            <button onClick={() => setPaused(!paused)}>{paused ? 'resume' : 'pause'}</button>
            <button onClick={clear}>clear log</button>
            <button onClick={() => setOpen(false)}>×</button>
          </div>
        </div>

        <div className="diag-body">
          {/* SSE state */}
          <section className="diag-section">
            <h3>SSE — /v1/shader/stream</h3>
            <div className="diag-grid">
              <div>
                <span className="diag-dot" style={{ background: sse.connected ? STATUS_COLORS.healthy : STATUS_COLORS.offline }} />
                {sse.connected ? 'connected' : 'disconnected'}
              </div>
              <div>reconnects: <span className="diag-mono">{sse.reconnectCount}</span></div>
              <div>last event: <span className="diag-mono">{sse.lastEventType}</span> · {age(sse.lastEventTs)} ago</div>
              <div>bytes: <span className="diag-mono">{sse.bytesReceived}</span></div>
              <div>url: <span className="diag-mono diag-muted">{sse.url}</span></div>
            </div>
          </section>

          {/* Endpoints */}
          <section className="diag-section">
            <h3>Endpoints ({endpointList.length})</h3>
            {endpointList.length === 0 ? (
              <div className="diag-muted">No endpoint probes registered yet.</div>
            ) : (
              <table className="diag-table">
                <thead>
                  <tr>
                    <th>label</th><th>url</th><th>status</th><th>lat</th><th>checked</th><th>last error / shape</th>
                  </tr>
                </thead>
                <tbody>
                  {endpointList.map((ep) => <EndpointRow key={ep.url} ep={ep} />)}
                </tbody>
              </table>
            )}
          </section>

          {/* NaN field tracker */}
          {nanFields.length > 0 && (
            <section className="diag-section">
              <h3>NaN / shape mismatches by field</h3>
              <table className="diag-table">
                <thead>
                  <tr><th>field</th><th>count</th></tr>
                </thead>
                <tbody>
                  {nanFields.map(([k, v]) => (
                    <tr key={k}>
                      <td className="diag-mono">{k}</td>
                      <td className="diag-mono" style={{ color: '#ffb547' }}>{v}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </section>
          )}

          {/* Recent entries */}
          <section className="diag-section">
            <h3>Recent ({entries.length} total · showing {recent.length})</h3>
            {recent.length === 0 ? (
              <div className="diag-muted">No diagnostic entries yet. All systems quiet.</div>
            ) : (
              <table className="diag-table">
                <thead>
                  <tr>
                    <th>time</th><th>lvl</th><th>src</th><th>cat</th><th>field/ep</th><th>message</th>
                  </tr>
                </thead>
                <tbody>
                  {recent.map((e) => <EntryRow key={e.id} e={e} />)}
                </tbody>
              </table>
            )}
          </section>
        </div>
      </div>
    </div>
  );
}
