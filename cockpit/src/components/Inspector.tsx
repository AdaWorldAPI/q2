import { useStore } from '../store';

const TYPE_COLORS: Record<string, string> = {
  Server: '#00bcd4',
  Gateway: '#4dd0e1',
  Database: '#ffc107',
  Cache: '#ff7043',
  LoadBalancer: '#66bb6a',
  Monitor: '#ab47bc',
  Queue: '#ef5350',
  CDN: '#42a5f5',
  DNS: '#78909c',
  Secrets: '#8d6e63',
  Search: '#7e57c2',
  Service: '#26c6da',
  Worker: '#9ccc65',
};

function StatusBadge({ status }: { status: string }) {
  const cls =
    status === 'healthy'
      ? 'healthy'
      : status === 'warning'
        ? 'warning'
        : 'critical';
  return <span className={`status-badge ${cls}`}>{status}</span>;
}

export function Inspector() {
  const selectedNodeId = useStore((s) => s.selectedNodeId);
  const nodes = useStore((s) => s.nodes);
  const edges = useStore((s) => s.edges);
  const selectNode = useStore((s) => s.selectNode);

  const node = nodes.find((n) => n.id === selectedNodeId);

  const outgoing = edges.filter((e) => e.source === selectedNodeId);
  const incoming = edges.filter((e) => e.target === selectedNodeId);

  return (
    <>
      <div className="panel-header">
        <span className="panel-title">Inspector</span>
        {node && (
          <span className="panel-badge">{outgoing.length + incoming.length} connections</span>
        )}
      </div>
      <div className="panel-body properties-body">
        {!node ? (
          <div
            style={{
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              justifyContent: 'center',
              height: '100%',
              color: 'var(--text-muted)',
              gap: '12px',
              padding: '20px',
              textAlign: 'center',
            }}
          >
            <svg width="40" height="40" viewBox="0 0 40 40" opacity="0.4">
              <circle cx="14" cy="14" r="5" stroke="#00bcd4" strokeWidth="1.2" fill="none" />
              <circle cx="30" cy="10" r="3" stroke="#00bcd4" strokeWidth="1.2" fill="none" />
              <circle cx="26" cy="30" r="4" stroke="#00bcd4" strokeWidth="1.2" fill="none" />
              <line x1="18" y1="16" x2="28" y2="12" stroke="#00bcd4" strokeWidth="0.7" opacity="0.4" />
              <line x1="16" y1="18" x2="24" y2="27" stroke="#00bcd4" strokeWidth="0.7" opacity="0.4" />
            </svg>
            <div style={{ fontSize: '12px' }}>Select a node to view properties</div>
          </div>
        ) : (
          <>
            {/* Node header */}
            <div className="prop-section">
              <div className="prop-node-header">
                <span
                  className="prop-node-icon"
                  style={{
                    background: `${TYPE_COLORS[node.type] || '#00bcd4'}22`,
                    border: `1.5px solid ${TYPE_COLORS[node.type] || '#00bcd4'}`,
                    boxShadow: `0 0 10px ${TYPE_COLORS[node.type] || '#00bcd4'}33`,
                  }}
                />
                <div>
                  <div className="prop-node-label">{node.label}</div>
                  <div
                    className="prop-node-type"
                    style={{ color: TYPE_COLORS[node.type] || '#00bcd4' }}
                  >
                    {node.type}
                  </div>
                </div>
              </div>
            </div>

            {/* Attributes */}
            <div className="prop-section">
              <div className="prop-section-title">Attributes</div>
              <table className="prop-table">
                <tbody>
                  <tr>
                    <td className="prop-key">id</td>
                    <td className="prop-val">{node.id}</td>
                  </tr>
                  {Object.entries(node.properties).map(([key, val]) => (
                    <tr key={key}>
                      <td className="prop-key">{key}</td>
                      <td className="prop-val">
                        {key === 'status' ? (
                          <StatusBadge status={String(val)} />
                        ) : typeof val === 'number' ? (
                          Number.isInteger(val) ? val : val.toFixed(2)
                        ) : (
                          String(val)
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

            {/* Connections */}
            {(outgoing.length > 0 || incoming.length > 0) && (
              <div className="prop-section">
                <div className="prop-section-title">
                  Connections{' '}
                  <span className="prop-count">{outgoing.length + incoming.length}</span>
                </div>
                <div className="prop-edges">
                  {outgoing.map((e, i) => {
                    const target = nodes.find((n) => n.id === e.target);
                    return (
                      <div
                        key={`out-${i}`}
                        className="prop-edge"
                        onClick={() => selectNode(e.target)}
                      >
                        <span className="edge-direction outgoing">{'\u2192'}</span>
                        <span className="edge-label">{e.label}</span>
                        <span className="edge-target">{target?.label || e.target}</span>
                      </div>
                    );
                  })}
                  {incoming.map((e, i) => {
                    const source = nodes.find((n) => n.id === e.source);
                    return (
                      <div
                        key={`in-${i}`}
                        className="prop-edge"
                        onClick={() => selectNode(e.source)}
                      >
                        <span className="edge-direction incoming">{'\u2190'}</span>
                        <span className="edge-label">{e.label}</span>
                        <span className="edge-target">{source?.label || e.source}</span>
                      </div>
                    );
                  })}
                </div>
              </div>
            )}
          </>
        )}
      </div>
    </>
  );
}
