import { useEffect, useRef } from 'react';
import type { WireBusDto } from '../hooks/useShaderStream';
import { clamp, fmt, safeNum } from '../diagnostics/safe';

interface BusTickerProps {
  items: WireBusDto[];
  maxItems?: number;
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
            const idx = safeNum(bus.codebook_index, -1, 'bus.codebook_index');
            const energy = clamp(bus.energy, 0, 1, 'bus.energy');
            const cycleCount = safeNum(bus.cycle_count, 0, 'bus.cycle_count');
            const converged = bus.converged === true;
            const topKList = Array.isArray(bus.top_k) ? bus.top_k : [];
            const tip = topKList
              .map((entry) => {
                if (!Array.isArray(entry) || entry.length !== 2) return null;
                const [eIdx, eEng] = entry;
                return `${safeNum(eIdx, 0)}=${fmt(eEng, 2)}`;
              })
              .filter(Boolean)
              .join(', ');
            return (
              <div key={i} className={`bus-ticker-row ${converged ? 'converged' : ''}`}>
                <span className="bus-codebook" title={tip || 'no top-k'}>
                  [{idx >= 0 ? String(idx).padStart(4, '0') : '????'}]
                </span>
                <span className="bus-energy-bar">
                  <span
                    className="bus-energy-fill"
                    style={{ width: `${Math.round(energy * 100)}%` }}
                  />
                </span>
                <span className="bus-energy-val">{fmt(energy, 3, 'bus.energy')}</span>
                <span className="bus-cycle">{cycleCount}c</span>
                {converged && <span className="bus-converged">✓</span>}
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
