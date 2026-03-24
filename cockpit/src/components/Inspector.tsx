import { useEffect, useRef, useMemo } from 'react';
import { useStore } from '../store';

const TYPE_COLORS: Record<string, string> = {
  Server: '#4dd0e1', Gateway: '#4dd0e1', Database: '#ffd166', Cache: '#ff7043',
  LoadBalancer: '#66bb6a', Monitor: '#ab47bc', Queue: '#ef5350', CDN: '#42a5f5',
  DNS: '#78909c', Secrets: '#8d6e63', Search: '#7e57c2', Service: '#9b8cff', Worker: '#9ccc65',
};

const STATUS_COLORS: Record<string, string> = {
  healthy: '#35d07f', warning: '#ffb547', critical: '#ff637d',
};

function drawSparkline(canvas: HTMLCanvasElement, values: number[], status: string) {
  const ctx = canvas.getContext('2d');
  if (!ctx) return;
  const w = canvas.width;
  const h = canvas.height;
  ctx.clearRect(0, 0, w, h);
  const color = STATUS_COLORS[status] || '#4dd0e1';
  const max = Math.max(...values) + 4;
  const min = Math.min(...values) - 4;

  // Grid lines
  ctx.strokeStyle = 'rgba(77, 208, 225, 0.10)';
  ctx.lineWidth = 1;
  for (let i = 1; i <= 3; i++) {
    const y = (h / 4) * i;
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(w, y);
    ctx.stroke();
  }

  // Line
  ctx.strokeStyle = color;
  ctx.lineWidth = 2.4;
  ctx.beginPath();
  values.forEach((v, i) => {
    const x = (w - 16) * (i / (values.length - 1)) + 8;
    const y = h - 10 - ((v - min) / (max - min)) * (h - 20);
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  ctx.stroke();

  // Dots
  ctx.fillStyle = color;
  values.forEach((v, i) => {
    const x = (w - 16) * (i / (values.length - 1)) + 8;
    const y = h - 10 - ((v - min) / (max - min)) * (h - 20);
    ctx.beginPath();
    ctx.arc(x, y, 2.4, 0, Math.PI * 2);
    ctx.fill();
  });
}

export function Inspector() {
  const selectedNodeId = useStore((s) => s.selectedNodeId);
  const nodes = useStore((s) => s.nodes);
  const edges = useStore((s) => s.edges);
  const selectNode = useStore((s) => s.selectNode);
  const sparkRef = useRef<HTMLCanvasElement>(null);

  const node = nodes.find((n) => n.id === selectedNodeId);

  const connections = useMemo(() => {
    if (!selectedNodeId) return [];
    return edges
      .filter((e) => e.source === selectedNodeId || e.target === selectedNodeId)
      .map((e) => ({
        kind: e.label,
        peer: e.source === selectedNodeId ? e.target : e.source,
        direction: e.source === selectedNodeId ? 'outbound' : 'inbound',
      }));
  }, [edges, selectedNodeId]);

  // Generate synthetic sparkline data from CPU property
  const sparkData = useMemo(() => {
    if (!node) return [];
    const cpu = typeof node.properties.cpu === 'number' ? node.properties.cpu * 100 : 40;
    return Array.from({ length: 10 }, (_, i) =>
      Math.round(cpu + (Math.sin(i * 0.8) * 6) + (i * 1.5)),
    );
  }, [node]);

  useEffect(() => {
    if (sparkRef.current && sparkData.length > 0 && node) {
      drawSparkline(sparkRef.current, sparkData, String(node.properties.status || 'healthy'));
    }
  }, [sparkData, node]);

  if (!node) {
    return (
      <section className="panel sidebar">
        <div className="panel-header">
          <div className="panel-title">
            <h2>Node intelligence</h2>
            <span>properties &middot; neighbors &middot; sparkline</span>
          </div>
        </div>
        <div className="sidebar-body sidebar-empty">
          <svg width="48" height="48" viewBox="0 0 48 48" opacity="0.25">
            <circle cx="16" cy="16" r="6" stroke="#4dd0e1" strokeWidth="1.5" fill="none" />
            <circle cx="36" cy="12" r="4" stroke="#4dd0e1" strokeWidth="1.5" fill="none" />
            <circle cx="32" cy="36" r="5" stroke="#4dd0e1" strokeWidth="1.5" fill="none" />
            <line x1="21" y1="19" x2="33" y2="14" stroke="#4dd0e1" strokeWidth="0.8" opacity="0.4" />
            <line x1="19" y1="21" x2="29" y2="32" stroke="#4dd0e1" strokeWidth="0.8" opacity="0.4" />
          </svg>
          <div style={{ fontSize: '12px', color: 'var(--muted)', marginTop: '12px' }}>
            Select a node to view intelligence
          </div>
        </div>
      </section>
    );
  }

  const color = TYPE_COLORS[node.type] || '#4dd0e1';
  const statusStr = String(node.properties.status || 'healthy');

  return (
    <section className="panel sidebar">
      <div className="panel-header">
        <div className="panel-title">
          <h2>Node intelligence</h2>
          <span>properties &middot; neighbors &middot; sparkline</span>
        </div>
        <div className="signal">live linked</div>
      </div>
      <div className="sidebar-body">
        {/* Node card */}
        <div className="node-card">
          <h3 style={{ color }}>{node.label}</h3>
          <div className="node-meta">
            <span>{node.type}</span>
            <span style={{ color: STATUS_COLORS[statusStr], borderColor: `${STATUS_COLORS[statusStr]}33` }}>
              {statusStr}
            </span>
            {node.properties.region && <span>{String(node.properties.region)}</span>}
          </div>
          <div className="prop-grid">
            <div className="prop-row"><div className="k">id</div><div><code>{node.id}</code></div></div>
            {Object.entries(node.properties).map(([key, val]) => (
              <div className="prop-row" key={key}>
                <div className="k">{key}</div>
                <div style={key === 'status' ? { color: STATUS_COLORS[String(val)] } : undefined}>
                  {typeof val === 'number' ? (Number.isInteger(val) ? val : val.toFixed(2)) : String(val)}
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Connections */}
        {connections.length > 0 && (
          <div>
            <div className="section-label">connections</div>
            <div className="connections-list">
              {connections.map((c, i) => (
                <button
                  key={i}
                  className="connection-card"
                  onClick={() => selectNode(c.peer)}
                >
                  <div>
                    <b>{c.peer}</b>
                    <br />
                    <span style={{ color: 'var(--muted)', fontSize: '11px' }}>{c.direction}</span>
                  </div>
                  <div style={{ color: 'var(--accent-2)' }}>{c.kind}</div>
                </button>
              ))}
            </div>
          </div>
        )}

        {/* Sparkline */}
        <div className="spark-card">
          <div className="section-label">cpu trend</div>
          <canvas ref={sparkRef} width={320} height={82} />
        </div>
      </div>
      <div className="graph-footer" style={{ borderTop: '1px solid var(--border)' }}>
        <div className="footer-note">single-click updates every instrument</div>
      </div>
    </section>
  );
}
