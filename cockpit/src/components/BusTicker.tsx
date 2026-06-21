import { useEffect, useRef } from 'react';
import type { WireShaderBus } from '../hooks/useShaderStream';
import { clamp, fmt, safeNum, safeStr } from '../diagnostics/safe';

interface BusTickerProps {
  items: WireShaderBus[];
  maxItems?: number;
}

/** Format a u64-like fingerprint hash as a short hex tag. */
function fpHex(n: number): string {
  if (!Number.isFinite(n)) return '????????';
  // JS numbers can't hold full u64 — the bridge folds to a hash anyway.
  // Take low 32 bits of the absolute value as a stable display tag.
  const u = (n >>> 0).toString(16).padStart(8, '0');
  return u;
}

export function BusTicker({ items, maxItems = 20 }: BusTickerProps) {
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [items]);

  const safeItems = Array.isArray(items) ? items : [];
  const visible = safeItems.slice(-maxItems);

  return (
    <div className="bus-ticker">
      <div className="panel-header" style={{ padding: '4px 8px' }}>
        <div className="panel-title">
          <span style={{ fontSize: '11px', color: 'var(--muted)' }}>B bus commits</span>
          <span style={{ fontSize: '10px', color: 'var(--muted)', marginLeft: 8 }}>
            {safeItems.length} total
          </span>
        </div>
      </div>
      <div ref={listRef} className="bus-ticker-list">
        {visible.length === 0 ? (
          <div className="bus-ticker-empty">
            waiting for stream… <small style={{ color: '#666' }}>(Shift+D for diagnostics)</small>
          </div>
        ) : (
          visible.map((bus, i) => {
            if (!bus || typeof bus !== 'object') return null;
            const fpHash = safeNum(bus.cycle_fingerprint_hash, 0, 'bus.cycle_fingerprint_hash');
            const fpTag = fpHex(fpHash);
            const top = Array.isArray(bus.resonance?.top_k) ? bus.resonance.top_k : [];
            const energy = clamp(top[0]?.resonance, 0, 1, 'bus.resonance.top_k[0].resonance');
            const cyclesUsed = safeNum(bus.resonance?.cycles_used, 0, 'bus.resonance.cycles_used');
            const merge = safeStr(bus.gate?.merge, '—', 'bus.gate.merge');
            const edges = safeNum(bus.emitted_edge_count, 0, 'bus.emitted_edge_count');
            const tip = top
              .slice(0, 5)
              .map((h) => {
                if (!h || typeof h !== 'object') return null;
                return `${safeNum(h.row, 0)}=${fmt(h.resonance, 2)}`;
              })
              .filter(Boolean)
              .join(', ');
            return (
              <div key={i} className={`bus-ticker-row ${merge !== '—' ? 'converged' : ''}`}>
                <span className="bus-codebook" title={tip || 'no top-k'}>
                  [{fpTag}]
                </span>
                <span className="bus-energy-bar">
                  <span
                    className="bus-energy-fill"
                    style={{ width: `${Math.round(energy * 100)}%` }}
                  />
                </span>
                <span className="bus-energy-val">{fmt(energy, 3, 'bus.resonance.top_k[0].resonance')}</span>
                <span className="bus-cycle">{cyclesUsed}c</span>
                <span className="bus-converged" title={`merge=${merge} · edges=${edges}`}>
                  {merge}
                </span>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
