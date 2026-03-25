// Aiwar Explorer — table view (innocent CSV) → graph view → NARS reasoning
// Official 51 weapons visible first. Enrichment surfaces through "reasoning."

import { useState, useEffect, useRef, useMemo, useCallback } from 'react';
import { Network, type Options } from 'vis-network';
import { DataSet } from 'vis-data';
import { NarsPanel } from './NarsPanel';
import { CypherConsole } from './CypherConsole';
import { TruthBadge } from './TruthBadge';
import { AIWAR_QUERIES } from '../data/aiwar-queries';

interface AiwarNode {
  id: string;
  label: string;
  type: string;
  properties: Record<string, string | number>;
}

interface AiwarEdge {
  source: string;
  target: string;
  label: string;
}

interface AiwarExplorerProps {
  mcpCall: (tool: string, args: Record<string, unknown>) => Promise<any>;
}

const TYPE_COLORS: Record<string, string> = {
  System: '#00d4ff',
  Stakeholder: '#ff9800',
  Civic: '#4caf50',
  People: '#e040fb',
  Person: '#e040fb',
  Historical: '#ffab00',
  Weapon: '#00d4ff',
  Organization: '#ff9800',
  Nation: '#ff9800',
};

const VIS_OPTIONS: Options = {
  nodes: {
    shape: 'dot',
    font: { color: '#d9e9f9', face: 'Inter, system-ui, sans-serif', size: 11, strokeWidth: 3, strokeColor: '#0a0e17' },
    borderWidth: 2.5,
    shadow: { enabled: true, color: 'rgba(0,0,0,0.5)', size: 12, x: 0, y: 4 },
  },
  edges: {
    color: { color: 'rgba(125,162,186,0.3)', highlight: '#4dd0e1', hover: 'rgba(77,208,225,0.6)' },
    font: { color: 'rgba(147,169,191,0.6)', size: 9, face: 'Inter, system-ui, sans-serif', strokeWidth: 0, align: 'middle' },
    width: 1.2,
    smooth: { enabled: true, type: 'continuous', roundness: 0.12 },
    arrows: { to: { enabled: true, scaleFactor: 0.5 } },
  },
  physics: {
    solver: 'forceAtlas2Based',
    forceAtlas2Based: { gravitationalConstant: -100, centralGravity: 0.005, springLength: 180, springConstant: 0.03, damping: 0.4 },
    stabilization: { iterations: 200, fit: true },
  },
  interaction: { hover: true, tooltipDelay: 80, zoomView: true, dragView: true, dragNodes: true },
};

export function AiwarExplorer({ mcpCall }: AiwarExplorerProps) {
  const [nodes, setNodes] = useState<AiwarNode[]>([]);
  const [edges, setEdges] = useState<AiwarEdge[]>([]);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [view, setView] = useState<'table' | 'graph'>('table');
  const [typeFilter, setTypeFilter] = useState<string>('all');
  const [discoveredCount, setDiscoveredCount] = useState(0);
  const [loading, setLoading] = useState(false);
  const graphRef = useRef<HTMLDivElement>(null);
  const networkRef = useRef<Network | null>(null);

  // Load initial data
  useEffect(() => {
    loadDataset();
  }, []);

  const loadDataset = async () => {
    setLoading(true);
    try {
      const result = await mcpCall('demo_load', { dataset: 'aiwar' });
      if (result?.outputs) {
        const graphOutput = result.outputs.find((o: any) => o.type === 'graph');
        if (graphOutput) {
          const data = JSON.parse(graphOutput.content);
          setNodes(data.nodes || []);
          setEdges(data.edges || []);
        }
      }
    } finally {
      setLoading(false);
    }
  };

  // Type counts
  const typeCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    nodes.forEach((n) => {
      counts[n.type] = (counts[n.type] || 0) + 1;
    });
    return counts;
  }, [nodes]);

  // Filtered nodes
  const filtered = useMemo(() => {
    if (typeFilter === 'all') return nodes;
    return nodes.filter((n) => n.type === typeFilter);
  }, [nodes, typeFilter]);

  const filteredIds = useMemo(() => new Set(filtered.map((n) => n.id)), [filtered]);
  const filteredEdges = useMemo(
    () => edges.filter((e) => filteredIds.has(e.source) && filteredIds.has(e.target)),
    [edges, filteredIds],
  );

  const selectedNode = nodes.find((n) => n.id === selectedNodeId);

  // vis-network
  useEffect(() => {
    if (view !== 'graph' || !graphRef.current || filtered.length === 0) return;

    const visNodes = new DataSet(
      filtered.map((n) => ({
        id: n.id,
        label: n.label,
        color: {
          background: 'rgba(10,14,23,0.9)',
          border: TYPE_COLORS[n.type] || '#4dd0e1',
          highlight: { background: 'rgba(10,14,23,0.95)', border: '#4dd0e1' },
        },
        size: 14,
        title: `${n.label}\n${n.type}`,
      })),
    );
    const visEdges = new DataSet(
      filteredEdges.map((e, i) => ({ id: `e-${i}`, from: e.source, to: e.target, label: e.label })),
    );

    const net = new Network(graphRef.current, { nodes: visNodes, edges: visEdges }, VIS_OPTIONS);
    net.on('click', (p) => setSelectedNodeId(p.nodes.length > 0 ? (p.nodes[0] as string) : null));
    networkRef.current = net;

    return () => { net.destroy(); networkRef.current = null; };
  }, [view, filtered, filteredEdges]);

  // MCP wrappers
  const executeQuery = useCallback(async (code: string, lang: string) => {
    const result = await mcpCall('cell_execute', { code, lang });
    return {
      raw_output: result?.source || JSON.stringify(result),
      elapsed_ms: result?.elapsed_ms || 0,
      graph_json: result?.outputs?.find((o: any) => o.type === 'graph')?.content,
    };
  }, [mcpCall]);

  const runInference = useCallback(async (_nodeId: string, depth: number) => {
    const result = await mcpCall('graph_infer', { min_confidence: 0.4, max_hops: depth });
    setDiscoveredCount(result?.inferred_edges || 0);
    return (result?.edges || []).map((e: any) => ({
      source: e.source,
      target: e.target,
      rel_type: e.rel_type,
      truth: e.truth,
      inference_type: e.inference_type,
      via: e.via || [],
    }));
  }, [mcpCall]);

  const searchChina = useCallback(async (query: string) => {
    // This would call the XAI/Grok search via ADA_XAI
    await mcpCall('cell_execute', { code: `// LIVE SEARCH: "${query}"`, lang: 'markdown' });
  }, [mcpCall]);

  return (
    <div className="aiwar-explorer">
      {/* Header */}
      <div className="aiwar-header">
        <h2>AI in Warfare Research</h2>
        <div className="aiwar-stats">
          <span className="badge">{nodes.length} nodes</span>
          <span className="badge">{edges.length} edges</span>
          {discoveredCount > 0 && (
            <span className="badge warn">+{discoveredCount} discovered</span>
          )}
        </div>
        <div className="aiwar-view-toggle">
          <button className={`pill ${view === 'table' ? 'active' : ''}`} onClick={() => setView('table')}>
            Table View
          </button>
          <button className={`pill ${view === 'graph' ? 'active' : ''}`} onClick={() => setView('graph')}>
            Graph View
          </button>
        </div>
      </div>

      <div className="aiwar-body">
        {/* Left: entity type list */}
        <div className="aiwar-entities panel">
          <div className="panel-header">
            <div className="panel-title"><h2>Entities</h2></div>
          </div>
          <div className="rail-body">
            <button
              className={`pill ${typeFilter === 'all' ? 'active' : ''}`}
              onClick={() => setTypeFilter('all')}
            >
              All ({nodes.length})
            </button>
            {Object.entries(typeCounts).map(([type, count]) => (
              <button
                key={type}
                className={`pill ${typeFilter === type ? 'active' : ''}`}
                onClick={() => setTypeFilter(type)}
                style={{ color: TYPE_COLORS[type] }}
              >
                {type} ({count})
              </button>
            ))}
          </div>
        </div>

        {/* Center: table or graph */}
        <div className="aiwar-center panel">
          {loading ? (
            <div className="graph-empty"><div className="graph-empty-text">Loading aiwar data...</div></div>
          ) : view === 'table' ? (
            <div className="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>Name</th>
                    <th>Type</th>
                    <th>Status</th>
                    <th>Details</th>
                  </tr>
                </thead>
                <tbody>
                  {filtered.map((n) => (
                    <tr
                      key={n.id}
                      className={n.id === selectedNodeId ? 'active' : ''}
                      onClick={() => setSelectedNodeId(n.id)}
                    >
                      <td><strong>{n.label}</strong></td>
                      <td style={{ color: TYPE_COLORS[n.type] }}>{n.type}</td>
                      <td>{String(n.properties.currentStatus || n.properties.status || '')}</td>
                      <td style={{ color: 'var(--muted)', fontSize: '11px' }}>
                        {String(n.properties.militaryUse || n.properties.type || '')}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <div ref={graphRef} style={{ width: '100%', height: '100%' }} />
          )}

          {/* Reason button */}
          {selectedNode && (
            <div className="aiwar-reason-bar">
              <span>Selected: <strong>{selectedNode.label}</strong> ({selectedNode.type})</span>
              <button className="button nars-reason-btn" onClick={() => runInference(selectedNode.id, 3)}>
                Reason About This
              </button>
            </div>
          )}
        </div>

        {/* Right: inspector + NARS */}
        <div className="aiwar-inspector panel">
          <div className="panel-header">
            <div className="panel-title"><h2>{selectedNode ? 'Details' : 'Inspector'}</h2></div>
            {selectedNode && <div className="signal">live linked</div>}
          </div>
          <div className="sidebar-body">
            {selectedNode ? (
              <>
                <div className="node-card">
                  <h3 style={{ color: TYPE_COLORS[selectedNode.type] }}>{selectedNode.label}</h3>
                  <div className="node-meta">
                    <span style={{ color: TYPE_COLORS[selectedNode.type] }}>{selectedNode.type}</span>
                  </div>
                  <div className="prop-grid">
                    {Object.entries(selectedNode.properties).map(([k, v]) => (
                      <div className="prop-row" key={k}>
                        <div className="k">{k}</div>
                        <div>{String(v)}</div>
                      </div>
                    ))}
                  </div>
                </div>

                <NarsPanel
                  selectedNode={selectedNodeId}
                  onRunInference={runInference}
                  onSearchChina={searchChina}
                />
              </>
            ) : (
              <div className="sidebar-empty">
                <div style={{ fontSize: '12px', color: 'var(--muted)', marginTop: '12px' }}>
                  Select a weapon system to inspect
                </div>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Bottom: Cypher console */}
      <div className="aiwar-console panel">
        <CypherConsole
          preloadedQueries={AIWAR_QUERIES}
          onExecute={executeQuery}
          onNarsReason={(code) => runInference(code, 3)}
        />
      </div>
    </div>
  );
}
