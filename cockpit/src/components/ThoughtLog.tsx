import { useRef, useEffect } from 'react';
import type { WireShaderCrystal } from '../hooks/useShaderStream';
import { fmt, safeNum } from '../diagnostics/safe';

interface ThoughtLogProps {
  crystals: WireShaderCrystal[];
  maxItems?: number;
}

/** Fold a numeric fingerprint hash into a short hex signature. */
function signature(n: number): string {
  if (!Number.isFinite(n)) return '????????';
  return (n >>> 0).toString(16).padStart(8, '0');
}

/** Color a crystal row by meta confidence. */
function confColor(c: number): string {
  if (!Number.isFinite(c)) return 'var(--muted)';
  if (c >= 0.8) return '#35d07f';
  if (c >= 0.6) return '#00d4ff';
  if (c >= 0.4) return '#ffb547';
  return '#e040fb';
}

export function ThoughtLog({ crystals, maxItems = 50 }: ThoughtLogProps) {
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [crystals]);

  const safeCrystals = Array.isArray(crystals) ? crystals : [];
  const visible = safeCrystals.slice(-maxItems);

  return (
    <div className="thought-log">
      <div className="panel-header" style={{ padding: '4px 8px' }}>
        <div className="panel-title">
          <span style={{ fontSize: '11px', color: 'var(--muted)' }}>Γ crystallized cycles</span>
          <span style={{ fontSize: '10px', color: 'var(--muted)', marginLeft: 8 }}>
            {safeCrystals.length} committed
          </span>
        </div>
      </div>
      <div ref={listRef} className="thought-log-list">
        {visible.length === 0 ? (
          <div className="thought-log-empty">
            awaiting convergence… <small style={{ color: '#666' }}>(Shift+D for diagnostics)</small>
          </div>
        ) : (
          visible.map((crystal, i) => {
            if (!crystal || typeof crystal !== 'object') return null;
            const bus = crystal.bus ?? null;
            const meta = crystal.meta ?? null;
            const conf = meta ? safeNum(meta.confidence, 0, 'crystal.meta.confidence') : 0;
            const fpHash = bus ? safeNum(bus.cycle_fingerprint_hash, 0, 'crystal.bus.cycle_fingerprint_hash') : 0;
            const sig = signature(fpHash);
            const top = bus && Array.isArray(bus.resonance?.top_k) ? bus.resonance.top_k : [];
            const topRow = top[0]?.row ?? null;
            const persisted = crystal.persisted_row;
            const admit = meta?.should_admit_ignorance === true;
            return (
              <div key={i} className="thought-row">
                <span
                  className="thought-style"
                  style={{ color: confColor(conf) }}
                  title={`confidence=${fmt(conf, 3)}${admit ? ' · admit ignorance' : ''}`}
                >
                  c={fmt(conf, 2, 'crystal.meta.confidence')}
                </span>
                <span className="thought-codebook" title={`fingerprint=${sig}`}>
                  [{topRow !== null && Number.isFinite(topRow) ? topRow : '????'}]
                </span>
                <span className="thought-text">
                  sig {sig}
                  {persisted !== null && persisted !== undefined && (
                    <span style={{ color: 'var(--muted)', marginLeft: 6 }}>· row {persisted}</span>
                  )}
                  {admit && <span style={{ color: '#ffb547', marginLeft: 6 }}>· ⚠ ignorance</span>}
                </span>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
