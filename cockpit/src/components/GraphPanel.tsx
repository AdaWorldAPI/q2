import { useEffect, useRef, useCallback } from 'react';
import { Network, type Options } from 'vis-network';
import { DataSet } from 'vis-data';
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

const NETWORK_OPTIONS: Options = {
  nodes: {
    shape: 'dot',
    font: {
      color: '#8892b0',
      face: 'Inter, -apple-system, sans-serif',
      size: 11,
    },
    borderWidth: 1.5,
    shadow: false,
  },
  edges: {
    color: { color: '#2a3650', highlight: '#00bcd4', hover: '#4dd0e1' },
    font: {
      color: '#546078',
      size: 9,
      face: 'Inter, -apple-system, sans-serif',
      strokeWidth: 0,
      align: 'middle',
    },
    width: 1,
    smooth: { enabled: true, type: 'continuous', roundness: 0.2 },
  },
  physics: {
    solver: 'forceAtlas2Based',
    forceAtlas2Based: {
      gravitationalConstant: -60,
      centralGravity: 0.008,
      springLength: 140,
      springConstant: 0.04,
      damping: 0.4,
    },
    stabilization: { iterations: 120, fit: true },
  },
  interaction: {
    hover: true,
    tooltipDelay: 100,
    zoomView: true,
    dragView: true,
  },
  layout: { improvedLayout: true },
};

export function GraphPanel() {
  const containerRef = useRef<HTMLDivElement>(null);
  const networkRef = useRef<Network | null>(null);
  const nodes = useStore((s) => s.nodes);
  const edges = useStore((s) => s.edges);
  const selectedNodeId = useStore((s) => s.selectedNodeId);
  const selectNode = useStore((s) => s.selectNode);

  // Build network when graph data changes
  useEffect(() => {
    if (!containerRef.current || nodes.length === 0) {
      // Destroy old network if no data
      if (networkRef.current) {
        networkRef.current.destroy();
        networkRef.current = null;
      }
      return;
    }

    const visNodes = new DataSet(
      nodes.map((n) => {
        const color = TYPE_COLORS[n.type] || '#00bcd4';
        const connections =
          typeof n.properties.connections === 'number'
            ? n.properties.connections
            : 4;
        return {
          id: n.id,
          label: n.label,
          color: {
            background: color,
            border: color,
            highlight: { background: color, border: '#e8eaf6' },
            hover: { background: color, border: '#e8eaf6' },
          },
          size: 8 + Math.min(connections, 12) * 1.2,
          title: `${n.label}\n${n.type}`,
        };
      }),
    );

    const visEdges = new DataSet(
      edges.map((e, i) => ({
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
  }, [nodes, edges, selectNode]);

  // Sync external selection into vis-network
  useEffect(() => {
    if (!networkRef.current) return;
    networkRef.current.selectNodes(selectedNodeId ? [selectedNodeId] : []);
  }, [selectedNodeId]);

  const handleResetZoom = useCallback(() => {
    networkRef.current?.fit({ animation: true });
  }, []);

  return (
    <>
      <div className="panel-header">
        <span className="panel-title">Graph</span>
        <div className="panel-controls">
          <span className="panel-badge">{nodes.length} nodes</span>
          <div className="panel-divider" />
          <button className="panel-btn" onClick={handleResetZoom} title="Fit">
            <svg width="14" height="14" viewBox="0 0 14 14">
              <rect
                x="1"
                y="1"
                width="12"
                height="12"
                rx="2"
                stroke="currentColor"
                fill="none"
                strokeWidth="1.2"
              />
              <circle cx="7" cy="7" r="2" fill="currentColor" />
            </svg>
          </button>
        </div>
      </div>
      <div className="panel-body" id="graphCanvas">
        {nodes.length === 0 ? (
          <div className="graph-empty">
            <svg width="40" height="40" viewBox="0 0 40 40" opacity="0.4">
              <circle cx="14" cy="14" r="5" stroke="#00bcd4" strokeWidth="1.2" fill="none" />
              <circle cx="30" cy="10" r="3" stroke="#00bcd4" strokeWidth="1.2" fill="none" />
              <circle cx="26" cy="30" r="4" stroke="#00bcd4" strokeWidth="1.2" fill="none" />
              <line x1="18" y1="16" x2="28" y2="12" stroke="#00bcd4" strokeWidth="0.7" opacity="0.4" />
              <line x1="16" y1="18" x2="24" y2="27" stroke="#00bcd4" strokeWidth="0.7" opacity="0.4" />
            </svg>
            <div className="graph-empty-text">Run a query to see the graph</div>
          </div>
        ) : (
          <div ref={containerRef} style={{ width: '100%', height: '100%' }} />
        )}
      </div>
    </>
  );
}
