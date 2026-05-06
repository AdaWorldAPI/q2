import { useRef, useEffect } from 'react';
import type { WireThoughtStruct } from '../hooks/useShaderStream';
import { fmt, safeNum, safeStr } from '../diagnostics/safe';

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

  const safeThoughts = Array.isArray(thoughts) ? thoughts : [];
  const visible = safeThoughts.slice(-maxItems);

  return (
    <div className="thought-log">
      <div className="panel-header" style={{ padding: '4px 8px' }}>
        <div className="panel-title">
          <span style={{ fontSize: '11px', color: 'var(--muted)' }}>Γ crystallized thoughts</span>
          <span style={{ fontSize: '10px', color: 'var(--muted)', marginLeft: 8 }}>
            {safeThoughts.length} committed
          </span>
        </div>
      </div>
      <div ref={listRef} className="thought-log-list">
        {visible.length === 0 ? (
          <div className="thought-log-empty">
            awaiting convergence… <small style={{ color: '#666' }}>(Shift+D for diagnostics)</small>
          </div>
        ) : (
          visible.map((t, i) => {
            if (!t || typeof t !== 'object') return null;
            const styleStr = safeStr(t.style, 'idle', 'thought.style');
            const bus = t.bus ?? null;
            const codebookIdx = bus ? safeNum(bus.codebook_index, -1, 'thought.bus.codebook_index') : -1;
            const energy = bus ? safeNum(bus.energy, 0, 'thought.bus.energy') : 0;
            const text = safeStr(
              t.text ?? null,
              codebookIdx >= 0 ? `codebook[${codebookIdx}] e=${fmt(energy, 3)}` : 'no bus payload',
              'thought.text',
            );
            return (
              <div key={i} className="thought-row">
                <span
                  className="thought-style"
                  style={{ color: STYLE_COLORS[styleStr] ?? 'var(--muted)' }}
                >
                  {styleStr.slice(0, 3)}
                </span>
                <span className="thought-codebook">
                  [{codebookIdx >= 0 ? codebookIdx : '????'}]
                </span>
                <span className="thought-text">{text}</span>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
