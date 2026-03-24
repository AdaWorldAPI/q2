import { useEffect, useRef, useCallback, useMemo } from 'react';
import { Network, type Options } from 'vis-network';
import { DataSet } from 'vis-data';
import { useStore } from '../store';

const TYPE_COLORS: Record<string, string> = {
  Server: '#4dd0e1',
  Gateway: '#4dd0e1',
  Database: '#ffd166',
  Cache: '#ff7043',
  LoadBalancer: '#66bb6a',
  Monitor: '#ab47bc',
  Queue: '#ef5350',
  CDN: '#42a5f5',
  DNS: '#78909c',
  Secrets: '#8d6e63',
  Search: '#7e57c2',
  Service: '#9b8cff',
  Worker: '#9ccc65',
};

const STATUS_COLORS: Record<string, string> = {
  healthy: '#35d07f',
  warning: '#ffb547',
  critical: '#ff637d',
};

const NETWORK_OPTIONS: Options = {
  nodes: {
    shape: 'dot',
    font: {
      color: '#d9e9f9',
      face: 'Inter, ui-sans-serif, system-ui, sans-serif',
      size: 12,
      strokeWidth: 3,
      strokeColor: '#0a0e17',
    },
    borderWidth: 2.2,
    shadow: { enabled: true, color: 'rgba(0,0,0,0.4)', size: 12, x: 0, y: 4 },
  },
  edges: {
    color: { color: 'rgba(125,162,186,0.42)', highlight: '#4dd0e1', hover: 'rgba(77,208,225,0.7)' },
    font: {
      color: 'rgba(147,169,191,0.72)',
      size: 10,
      face: 'Inter, ui-sans-serif, system-ui, sans-serif',
      strokeWidth: 0,
      align: 'middle',
    },
    width: 1.4,
    smooth: { enabled: true, type: 'continuous', roundness: 0.15 },
    selectionWidth: 2,
    hoverWidth: 0.5,
  },
  physics: {
    solver: 'forceAtlas2Based',
    forceAtlas2Based: {
      gravitationalConstant: -80,
      centralGravity: 0.006,
      springLength: 160,
      springConstant: 0.035,
      damping: 0.4,
    },
    stabilization: { iterations: 150, fit: true },
  },
  interaction: {
    hover: true,
    tooltipDelay: 80,
    zoomView: true,
    dragView: true,
    multiselect: false,
  },
  layout: { improvedLayout: true },
};

export function GraphPanel() {
  const containerRef = useRef<HTMLDivElement>(null);
  const networkRef = useRef<Network | null>(null);
  const nodes = useStore((s) => s.nodes);
  const edges = useStore((s) => s.edges);
  const filter = useStore((s) => s.filter);
  const selectedNodeId = useStore((s) => s.selectedNodeId);
  const selectNode = useStore((s) => s.selectNode);

  // Filter nodes based on left rail selection
  const filteredNodes = useMemo(() => {
    if (filter === 'all') return nodes;
    if (filter === 'warning') return nodes.filter((n) => n.properties.status !== 'healthy');
    return nodes.filter((n) => n.type === filter);
  }, [nodes, filter]);

  const filteredIds = useMemo(() => new Set(filteredNodes.map((n) => n.id)), [filteredNodes]);
  const filteredEdges = useMemo(
    () => edges.filter((e) => filteredIds.has(e.source) && filteredIds.has(e.target)),
    [edges, filteredIds],
  );

  // Derive legend from visible node types
  const legendTypes = useMemo(() => {
    const types = new Set(filteredNodes.map((n) => n.type));
    return Array.from(types).slice(0, 6);
  }, [filteredNodes]);

  // Status counts for footer badges
  const statusCounts = useMemo(() => {
    const healthy = filteredNodes.filter((n) => n.properties.status === 'healthy').length;
    const warning = filteredNodes.filter((n) => n.properties.status === 'warning').length;
    const critical = filteredNodes.filter((n) => n.properties.status === 'critical').length;
    return { healthy, warning, critical };
  }, [filteredNodes]);

  useEffect(() => {
    if (!containerRef.current || filteredNodes.length === 0) {
      if (networkRef.current) {
        networkRef.current.destroy();
        networkRef.current = null;
      }
      return;
    }

    const visNodes = new DataSet(
      filteredNodes.map((n) => {
        const color = TYPE_COLORS[n.type] || '#4dd0e1';
        const statusColor = STATUS_COLORS[String(n.properties.status)] || '#35d07f';
        const connections =
          typeof n.properties.connections === 'number' ? n.properties.connections : 4;
        return {
          id: n.id,
          label: n.label,
          color: {
            background: 'rgba(10, 14, 23, 0.95)',
            border: color,
            highlight: { background: 'rgba(10, 14, 23, 0.95)', border: statusColor },
            hover: { background: 'rgba(10, 14, 23, 0.85)', border: color },
          },
          size: 12 + Math.min(connections, 12) * 1.5,
          title: `${n.label}\n${n.type} \u00b7 ${n.properties.status}\n${n.properties.region}`,
        };
      }),
    );

    const visEdges = new DataSet(
      filteredEdges.map((e, i) => ({
        id: `e-${i}`,
        from: e.source,
        to: e.target,
        label: e.label,
      })),
    );

    const network = new Network(
      containerRef.current,
      { nodes: visNodes, edges: visEdges },
      NETWORK_OPTIONS,
    );

    network.on('click', (params) => {
      if (params.nodes.length > 0) {
        selectNode(params.nodes[0] as string);
      } else {
        selectNode(null);
      }
    });

    networkRef.current = network;

    return () => {
      network.destroy();
      networkRef.current = null;
    };
  }, [filteredNodes, filteredEdges, selectNode]);

  useEffect(() => {
    if (!networkRef.current) return;
    networkRef.current.selectNodes(selectedNodeId ? [selectedNodeId] : []);
  }, [selectedNodeId]);

  const handleFit = useCallback(() => {
    networkRef.current?.fit({ animation: true });
  }, []);

  const selectedNode = nodes.find((n) => n.id === selectedNodeId);

  return (
    <section className="panel graph-panel">
      <div className="panel-header">
        <div className="panel-title">
          <h2>Graph result</h2>
          <span>force-directed &middot; linked to sidebar + table + cells</span>
        </div>
        <div className="signal">selection drives all views</div>
      </div>
      <div className="graph-stage" id="graphStage">
        {filteredNodes.length === 0 ? (
          <div className="graph-empty">
            <svg width="48" height="48" viewBox="0 0 48 48" opacity="0.3">
              <circle cx="16" cy="16" r="6" stroke="#4dd0e1" strokeWidth="1.5" fill="none" />
              <circle cx="36" cy="12" r="4" stroke="#4dd0e1" strokeWidth="1.5" fill="none" />
              <circle cx="32" cy="36" r="5" stroke="#4dd0e1" strokeWidth="1.5" fill="none" />
              <line x1="21" y1="19" x2="33" y2="14" stroke="#4dd0e1" strokeWidth="0.8" opacity="0.4" />
              <line x1="19" y1="21" x2="29" y2="32" stroke="#4dd0e1" strokeWidth="0.8" opacity="0.4" />
            </svg>
            <div className="graph-empty-text">Run a query to see the graph</div>
          </div>
        ) : (
          <>
            <div ref={containerRef} style={{ width: '100%', height: '100%' }} />
            {/* Overlay cards */}
            <div className="graph-overlay">
              <div className="overlay-card">
                <b>Auto mode</b>
                <p>
                  {selectedNode
                    ? `${selectedNode.label} selected. Click another node to update all panels.`
                    : 'Click a node to propagate selection through all panels.'}
                </p>
              </div>
              <div className="overlay-card overlay-card-bottom">
                <b>Viewport</b>
                <p>Wheel to zoom. Drag to pan. Drag nodes to rearrange.</p>
              </div>
            </div>
          </>
        )}
      </div>
      <div className="graph-footer">
        <div className="legend">
          {legendTypes.map((type) => (
            <span key={type} style={{ color: TYPE_COLORS[type] || '#4dd0e1' }}>
              {type.toLowerCase()}
            </span>
          ))}
        </div>
        <div className="mini-status">
          {selectedNode && (
            <span className="badge good">selected / {selectedNode.label}</span>
          )}
          {statusCounts.warning > 0 && (
            <span className="badge warn">{statusCounts.warning} warning</span>
          )}
          {statusCounts.critical > 0 && (
            <span className="badge hot">{statusCounts.critical} critical</span>
          )}
          <button className="badge" onClick={handleFit} style={{ cursor: 'pointer' }}>
            fit view
          </button>
        </div>
      </div>
    </section>
  );
}
