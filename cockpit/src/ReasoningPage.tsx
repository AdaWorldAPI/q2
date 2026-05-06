import { useShaderStream } from './hooks/useShaderStream';
import { EnergyField } from './components/EnergyField';
import { BusTicker } from './components/BusTicker';
import { ThoughtLog } from './components/ThoughtLog';
import { SceneBreadcrumb } from './components/SceneBreadcrumb';
import { FreeEnergyDial } from './components/FreeEnergyDial';
import { ErrorBoundary } from './components/ErrorBoundary';
import { DiagnosticsBadge, DiagnosticsOverlay } from './components/DiagnosticsOverlay';
import { useEndpointHealth } from './hooks/useEndpointHealth';
import { fmt, safeNum } from './diagnostics/safe';

/**
 * ReasoningPage — live AGI shader stream.
 *
 * Layout:
 *   [SceneBreadcrumb — top bar]
 *   [EnergyField Ψ] [BusTicker B] [FreeEnergyDial F]
 *   [ThoughtLog Γ — full width]
 */
export function ReasoningPage() {
  const stream = useShaderStream('/v1/shader/stream');
  useEndpointHealth(8000);

  return (
    <div className="shell reasoning-shell">
      {/* Top bar */}
      <section className="topbar">
        <div className="brand">
          <small>cognitive shader</small>
          <h1>REASONING</h1>
        </div>
        <div style={{ flex: 1, display: 'flex', alignItems: 'center', padding: '0 16px' }}>
          <SceneBreadcrumb scene={stream.currentScene} cycle={stream.cycle} />
        </div>
        <div className="top-actions">
          <span className={`badge ${stream.connected ? 'good' : ''}`}>
            {stream.connected ? 'Φ→Ψ→B→Γ live' : 'stream offline'}
          </span>
          {!stream.connected && (
            <button
              className="badge"
              style={{ cursor: 'pointer', background: 'none', fontFamily: 'var(--sans)', color: 'var(--yellow)', borderColor: 'rgba(255,181,71,0.3)' }}
              onClick={stream.reconnect}
            >
              reconnect
            </button>
          )}
          <span className="badge">{safeNum(stream.eventCount, 0, 'stream.eventCount')} events</span>
          <DiagnosticsBadge />
          <a href="/" className="badge" style={{ textDecoration: 'none' }}>← cockpit</a>
        </div>
      </section>

      {/* Main row: EnergyField · BusTicker · FreeEnergyDial */}
      <section className="reasoning-main">
        <div className="reasoning-left">
          <ErrorBoundary scope="EnergyField">
            <EnergyField resonance={stream.lastResonance} width={256} height={200} />
          </ErrorBoundary>
        </div>
        <div className="reasoning-center">
          <ErrorBoundary scope="BusTicker">
            <BusTicker items={stream.busHistory} maxItems={30} />
          </ErrorBoundary>
        </div>
        <div className="reasoning-right">
          <ErrorBoundary scope="FreeEnergyDial">
            <FreeEnergyDial freeEnergy={stream.freeEnergy} />
          </ErrorBoundary>
          {/* Stream source info — defensive against missing fields */}
          {stream.lastStream ? (
            <div style={{ padding: '8px', borderTop: '1px solid var(--border)', marginTop: 8 }}>
              <div style={{ fontSize: '10px', color: 'var(--muted)', marginBottom: 4 }}>Φ last stream</div>
              <div style={{ fontSize: '11px', fontFamily: 'var(--mono)', color: 'var(--text)' }}>
                src: {stream.lastStream.source ?? 'unknown'}
              </div>
              <div style={{ fontSize: '11px', fontFamily: 'var(--mono)', color: 'var(--muted)' }}>
                indices: [{Array.isArray(stream.lastStream.codebook_indices)
                  ? stream.lastStream.codebook_indices.slice(0, 4).join(', ')
                  : '—'}…]
              </div>
            </div>
          ) : (
            <div style={{ padding: '8px', borderTop: '1px solid var(--border)', marginTop: 8, fontSize: '10px', color: '#666' }}>
              Φ awaiting first stream event
            </div>
          )}
        </div>
      </section>

      {/* Thought log — full width */}
      <section className="reasoning-bottom">
        <ErrorBoundary scope="ThoughtLog">
          <ThoughtLog thoughts={stream.thoughtHistory} maxItems={80} />
        </ErrorBoundary>
      </section>

      {/* Status bar */}
      <footer className="status-bar">
        <div className="status-bar-left">
          <span className={`status-dot ${stream.connected ? 'online' : 'offline'}`} />
          <span>{stream.connected ? 'streaming' : 'offline'}</span>
          <span className="status-sep" />
          <span>Φ→Ψ→B→Γ pipeline</span>
          <span className="status-sep" />
          <span>{stream.thoughtHistory.length} thoughts · {stream.busHistory.length} bus commits</span>
          {stream.currentScene && (
            <>
              <span className="status-sep" />
              <span>
                act {stream.currentScene.act}/{stream.currentScene.total} · {stream.currentScene.name}
              </span>
            </>
          )}
        </div>
        <div className="status-bar-right">
          {stream.freeEnergy ? (
            <span style={{ color: stream.freeEnergy.below_homeostasis ? 'var(--green)' : 'var(--yellow)' }}>
              F={fmt(stream.freeEnergy.free_energy, 3, 'freeEnergy.free_energy')}
            </span>
          ) : (
            <span style={{ color: '#666' }}>F=—</span>
          )}
          <span className="status-sep" />
          <span>/v1/shader/stream</span>
          <span className="status-sep" />
          <span>localhost:2718</span>
        </div>
      </footer>

      {/* Diagnostics overlay — Shift+D to toggle */}
      <DiagnosticsOverlay />
    </div>
  );
}
