import { useEffect, useRef } from 'react';
import type { WireBusDto } from '../hooks/useShaderStream';

interface BusTickerProps {
  items: WireBusDto[];
  maxItems?: number;
}

export function BusTicker({ items, maxItems = 20 }: BusTickerProps) {
  const listRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom on new items
  useEffect(() => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [items]);

  const visible = items.slice(-maxItems);

  return (
    <div className="bus-ticker">
      <div className="panel-header" style={{ padding: '4px 8px' }}>
        <div className="panel-title">
          <span style={{ fontSize: '11px', color: 'var(--muted)' }}>B bus commits</span>
          <span style={{ fontSize: '10px', color: 'var(--muted)', marginLeft: 8 }}>
            {items.length} total
          </span>
        </div>
      </div>
      <div ref={listRef} className="bus-ticker-list">
        {visible.length === 0 ? (
          <div className="bus-ticker-empty">waiting for stream…</div>
        ) : (
          visible.map((bus, i) => (
            <div key={i} className={`bus-ticker-row ${bus.converged ? 'converged' : ''}`}>
              <span className="bus-codebook" title={`top-k: ${bus.top_k.map(([idx, e]) => `${idx}=${e.toFixed(2)}`).join(', ')}`}>
                [{String(bus.codebook_index).padStart(4, '0')}]
              </span>
              <span className="bus-energy-bar">
                <span
                  className="bus-energy-fill"
                  style={{ width: `${Math.round(bus.energy * 100)}%` }}
                />
              </span>
              <span className="bus-energy-val">{bus.energy.toFixed(3)}</span>
              <span className="bus-cycle">{bus.cycle_count}c</span>
              {bus.converged && <span className="bus-converged">✓</span>}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
