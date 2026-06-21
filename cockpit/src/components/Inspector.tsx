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
  const dpr = window.devicePixelRatio || 1;
  canvas.width = w * dpr;
  canvas.height = h * dpr;
  ctx.scale(dpr, dpr);
  canvas.style.width = w + 'px';
  canvas.style.height = h + 'px';

  ctx.clearRect(0, 0, w, h);
  const color = STATUS_COLORS[status] || '#4dd0e1';
  const max = Math.max(...values) + 4;
  const min = Math.min(...values) - 4;

  // Grid lines
  ctx.strokeStyle = 'rgba(77, 208, 225, 0.08)';
  ctx.lineWidth = 0.5;
  for (let i = 1; i <= 3; i++) {
    const y = (h / 4) * i;
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(w, y);
    ctx.stroke();
  }

  // Fill area under curve
  const gradient = ctx.createLinearGradient(0, 0, 0, h);
  gradient.addColorStop(0, color.replace(')', ', 0.15)').replace('rgb', 'rgba'));
  gradient.addColorStop(1, 'rgba(0,0,0,0)');
  ctx.fillStyle = gradient;
  ctx.beginPath();
  values.forEach((v, i) => {
    const x = (w - 16) * (i / (values.length - 1)) + 8;
    const y = h - 10 - ((v - min) / (max - min)) * (h - 20);
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  const lastX = (w - 16) + 8;
  ctx.lineTo(lastX, h);
  ctx.lineTo(8, h);
  ctx.closePath();
  ctx.fill();

  // Line
  ctx.strokeStyle = color;
  ctx.lineWidth = 2;
  ctx.lineJoin = 'round';
  ctx.lineCap = 'round';
  ctx.beginPath();
  values.forEach((v, i) => {
    const x = (w - 16) * (i / (values.length - 1)) + 8;
    const y = h - 10 - ((v - min) / (max - min)) * (h - 20);
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  ctx.stroke();

  // Terminal dot (latest point)
  const lastVal = values[values.length - 1];
  const lx = (w - 16) + 8;
  const ly = h - 10 - ((lastVal - min) / (max - min)) * (h - 20);
  ctx.fillStyle = color;
  ctx.beginPath();
  ctx.arc(lx, ly, 3.5, 0, Math.PI * 2);
  ctx.fill();
  // Glow ring
  ctx.strokeStyle = color;
  ctx.lineWidth = 1;
  ctx.globalAlpha = 0.4;
  ctx.beginPath();
  ctx.arc(lx, ly, 6, 0, Math.PI * 2);
  ctx.stroke();
  ctx.globalAlpha = 1;
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
      .map((e) => {
        const isOutbound = e.source === selectedNodeId;
        return {
          kind: e.label,
          peer: isOutbound ? e.target : e.source,
          direction: isOutbound ? 'outbound' : 'inbound',
          arrow: isOutbound ? '\u2192' : '\u2190',
        };
      });
  }, [edges, selectedNodeId]);

  // Compute degree
  const degree = useMemo(() => {
    if (!selectedNodeId) return 0;
    return edges.filter((e) => e.source === selectedNodeId || e.target === selectedNodeId).length;
  }, [edges, selectedNodeId]);

  // Generate sparkline data from CPU property
  const sparkData = useMemo(() => {
    if (!node) return [];
    const cpu = typeof node.properties.cpu === 'number' ? node.properties.cpu * 100 : 40;
    return Array.from({ length: 12 }, (_, i) =>
      Math.round(cpu + (Math.sin(i * 0.7) * 8) + (Math.cos(i * 0.3) * 4) + (i * 0.8)),
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
            <h2>Properties</h2>
            <span>properties &middot; metadata &middot; neighbors &middot; sparkline</span>
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
  const statusColor = STATUS_COLORS[statusStr] || '#35d07f';

  return (
    <section className="panel sidebar">
      <div className="panel-header">
        <div className="panel-title">
          <h2>Properties</h2>
          <span>properties &middot; metadata &middot; neighbors &middot; sparkline</span>
        </div>
        <div className="signal">live linked</div>
      </div>
      <div className="sidebar-body">
        {/* Node identity card */}
        <div className="node-card">
          <div className="node-identity">
            <div className="node-avatar" style={{ borderColor: statusColor, background: `${color}18` }}>
              <span style={{ color }}>{node.type.slice(0, 2).toUpperCase()}</span>
            </div>
            <div>
              <h3 style={{ color }}>{node.label}</h3>
              <span className="node-type-label" style={{ color: statusColor }}>
                {node.type} &middot; {statusStr}
              </span>
            </div>
          </div>
        </div>

        {/* Attributes */}
        <div>
          <div className="section-label">attributes</div>
          <div className="prop-grid">
            <div className="prop-row"><div className="k">id</div><div><code>{node.id}</code></div></div>
            <div className="prop-row"><div className="k">region</div><div>{String(node.properties.region || '')}</div></div>
            <div className="prop-row"><div className="k">status</div><div style={{ color: statusColor }}>{statusStr}</div></div>
            <div className="prop-row"><div className="k">compute</div><div>{typeof node.properties.cpu === 'number' ? `${(node.properties.cpu * 100).toFixed(0)}% cpu, degree ${degree}` : 'n/a'}</div></div>
            <div className="prop-row"><div className="k">memory</div><div>{typeof node.properties.memory_gb === 'number' ? `${node.properties.memory_gb} GB` : 'n/a'}</div></div>
            <div className="prop-row"><div className="k">connections</div><div>{String(node.properties.connections || degree)}</div></div>
          </div>
        </div>

        {/* Connections */}
        {connections.length > 0 && (
          <div>
            <div className="section-label">connections <span style={{ opacity: 0.5 }}>({connections.length})</span></div>
            <div className="connections-list">
              {connections.map((c, i) => (
                <button
                  key={i}
                  className="connection-card"
                  onClick={() => selectNode(c.peer)}
                >
                  <div className="conn-info">
                    <span className={`conn-arrow ${c.direction}`}>{c.arrow}</span>
                    <div>
                      <b>{c.peer}</b>
                      <span className="conn-dir">{c.direction}</span>
                    </div>
                  </div>
                  <span className="conn-kind">{c.kind}</span>
                </button>
              ))}
            </div>
          </div>
        )}

        {/* Sparkline */}
        <div className="spark-card">
          <div className="section-label">latency trend</div>
          <canvas ref={sparkRef} width={320} height={82} />
          <div className="spark-footer">single-click updates every instrument on screen</div>
        </div>
      </div>
    </section>
  );
}
