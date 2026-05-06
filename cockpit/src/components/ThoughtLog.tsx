import { useRef, useEffect } from 'react';
import type { WireThoughtStruct } from '../hooks/useShaderStream';

interface ThoughtLogProps {
  thoughts: WireThoughtStruct[];
  maxItems?: number;
}

const STYLE_COLORS: Record<string, string> = {
  Exploiting: '#35d07f',
  Focused: '#00d4ff',
  Exploring: '#ffb547',
  Abstract: '#e040fb',
};

export function ThoughtLog({ thoughts, maxItems = 50 }: ThoughtLogProps) {
  const listRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [thoughts]);

  const visible = thoughts.slice(-maxItems);

  return (
    <div className="thought-log">
      <div className="panel-header" style={{ padding: '4px 8px' }}>
        <div className="panel-title">
          <span style={{ fontSize: '11px', color: 'var(--muted)' }}>Γ crystallized thoughts</span>
          <span style={{ fontSize: '10px', color: 'var(--muted)', marginLeft: 8 }}>
            {thoughts.length} committed
          </span>
        </div>
      </div>
      <div ref={listRef} className="thought-log-list">
        {visible.length === 0 ? (
          <div className="thought-log-empty">awaiting convergence…</div>
        ) : (
          visible.map((t, i) => (
            <div key={i} className="thought-row">
              <span
                className="thought-style"
                style={{ color: STYLE_COLORS[t.style] ?? 'var(--muted)' }}
              >
                {t.style.slice(0, 3)}
              </span>
              <span className="thought-codebook">[{t.bus.codebook_index}]</span>
              <span className="thought-text">
                {t.text ?? `codebook[${t.bus.codebook_index}] e=${t.bus.energy.toFixed(3)}`}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
