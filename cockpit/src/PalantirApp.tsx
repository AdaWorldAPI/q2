import { useEffect, useMemo, useState, useRef } from 'react';
import { useStore, type GraphNode, type GraphEdge } from './store';
import { connectSSE, executeQuery, hydrateCockpit } from './transport';
import { QueryBar } from './components/QueryBar';
import { GraphPanel } from './components/GraphPanel';
import { Inspector } from './components/Inspector';
import { ResultTable } from './components/ResultTable';
import { CellStrip } from './components/CellStrip';
import { LeftRail } from './components/LeftRail';
import { DataStatusBar } from './components/DataStatusBar';
import { AnalystPanel } from './components/AnalystPanel';
import { ErrorBoundary } from './components/ErrorBoundary';
import { DiagnosticsBadge, DiagnosticsOverlay } from './components/DiagnosticsOverlay';
import { useEndpointHealth } from './hooks/useEndpointHealth';
import { convertAiwarGraph } from './data/aiwar-seed';

interface DataSource {
  name: string;
  file: string;
  status: 'loading' | 'loaded' | 'error' | 'not_found' | 'found' | 'embedded' | 'empty' | 'static';
  detail: string;
  elapsed_ms?: number;
  count?: number;
}

/**
 * PalantirApp — IDENTICAL Palantir layout, hydrated through lance-graph.
 *
 * Hydration order:
 * 1. Try MCP: MATCH (n) RETURN n  → lance-graph returns graph_json
 * 2. Fallback: fetch /aiwar_graph.json → static JSON
 * 3. Fallback: keep seed data (24 infra nodes)
 */
export function PalantirApp() {
  const connected = useStore((s) => s.connected);
  const nodes = useStore((s) => s.nodes);
  const edges = useStore((s) => s.edges);
  const cells = useStore((s) => s.cells);
  const executing = useStore((s) => s.executing);
  const setGraphData = useStore((s) => s.setGraphData);

  const [showAnalyst, setShowAnalyst] = useState(false);
  const [dataSources, setDataSources] = useState<DataSource[]>([
    { name: 'Aiwar Graph', file: 'aiwar_graph.json', status: 'loading', detail: 'Connecting...' },
    { name: 'Enrichment Cypher', file: 'cypher/*.cypher', status: 'loading', detail: 'Checking...' },
    { name: 'Neural Diagnosis', file: 'neural_diagnosis.json', status: 'loading', detail: 'Checking...' },
    { name: 'Aiwar CSV', file: 'aiwarcloud-table.csv', status: 'loading', detail: 'Checking...' },
  ]);
  const loadedRef = useRef(false);

  useEffect(() => { connectSSE(); }, []);

  // Poll endpoint health every 8s
  useEndpointHealth(8000);

  // Hydrate data through MCP → static JSON fallback → seed data
  useEffect(() => {
    if (loadedRef.current) return;
    loadedRef.current = true;

    const updateSource = (name: string, update: Partial<DataSource>) => {
      setDataSources(prev => prev.map(s => s.name === name ? { ...s, ...update } : s));
    };

    (async () => {
      // Strategy 0: Try live graph engine API (direct Rust, no MCP overhead)
      let hydrated = await hydrateCockpit();
      if (hydrated) {
        updateSource('Aiwar Graph', {
          status: 'loaded',
          detail: 'live graph engine (neo4j-emulating, NARS-enabled)',
        });
      }

      // Strategy 1: Try MCP Cypher query for live data from lance-graph
      try {
        const cell = await executeQuery('MATCH (n) RETURN n LIMIT 500', 'cypher');
        const graphOutput = cell.outputs.find(o => o.type === 'graph');
        if (graphOutput) {
          const data = JSON.parse(graphOutput.content);
          if (data.nodes?.length > 0) {
            setGraphData(data.nodes as GraphNode[], data.edges as GraphEdge[]);
            updateSource('Aiwar Graph', {
              status: 'loaded',
              detail: `${data.nodes.length} nodes via lance-graph MCP`,
            });
            hydrated = true;
          }
        }
        if (!hydrated) {
          const textOut = cell.outputs.find(o => o.type === 'text');
          updateSource('Aiwar Graph', {
            status: 'loaded',
            detail: `MCP returned: ${textOut?.content?.slice(0, 60) || 'no graph data'}`,
          });
        }
      } catch {
        // MCP not available — try static JSON fallback
      }

      // Strategy 2: Fetch static JSON if MCP didn't hydrate
      if (!hydrated) {
        try {
          const res = await fetch('/aiwar_graph.json');
          if (res.ok) {
            const raw = await res.json();
            const { nodes: gNodes, edges: gEdges } = convertAiwarGraph(raw);
            if (gNodes.length > 0) {
              setGraphData(gNodes, gEdges);
              updateSource('Aiwar Graph', {
                status: 'loaded',
                detail: `${gNodes.length} nodes from static JSON`,
              });
              hydrated = true;
            }
          } else {
            updateSource('Aiwar Graph', {
              status: 'not_found',
              detail: `HTTP ${res.status} — aiwar_graph.json not served`,
            });
          }
        } catch (err) {
          updateSource('Aiwar Graph', {
            status: 'error',
            detail: err instanceof Error ? err.message : 'JSON parse failed',
          });
        }
      }

      if (!hydrated) {
        updateSource('Aiwar Graph', {
          status: 'error',
          detail: 'Using 24-node seed data (aiwar not available)',
        });
      }

      // Check other data sources via /api/data/status
      try {
        const res = await fetch('/api/data/status');
        if (res.ok) {
          const status = await res.json();
          if (status.sources) {
            for (const src of status.sources) {
              updateSource(src.name, {
                status: src.status,
                detail: src.detail,
                elapsed_ms: src.elapsed_ms,
                count: src.count,
              });
            }
          }
        }
      } catch {
        // /api/data/status not available (Node demo server)
        // Check static files individually
        for (const file of ['neural_diagnosis.json', 'aiwar_weapons.json']) {
          try {
            const r = await fetch(`/${file}`, { method: 'HEAD' });
            const name = file === 'neural_diagnosis.json' ? 'Neural Diagnosis' : 'Aiwar CSV';
            updateSource(name, {
              status: r.ok ? 'found' : 'not_found',
              detail: r.ok ? `Served from static` : `HTTP ${r.status}`,
            });
          } catch {
            // ignore
          }
        }
      }
    })();
  }, [setGraphData]);

  const cellCount = useMemo(() => {
    const running = cells.filter((c) => c.execution_state === 'running').length;
    return { total: cells.length, running };
  }, [cells]);

  return (
    <div className="shell">
      {/* Row 1: Top bar */}
      <section className="topbar">
        <div className="brand">
          <small>q2 graph engine</small>
          <h1>AIWAR</h1>
        </div>
        <QueryBar />
        <div className="top-actions">
          <span className={`badge ${connected ? 'good' : ''}`}>
            {connected ? 'mcp /sse live' : 'disconnected'}
          </span>
          <button
            className={`badge ${showAnalyst ? 'good' : ''}`}
            style={{ cursor: 'pointer', color: showAnalyst ? '#35d07f' : '#ffb547', borderColor: showAnalyst ? 'rgba(53,208,127,0.3)' : 'rgba(255,181,71,0.2)', background: 'none', fontFamily: 'var(--sans)' }}
            onClick={() => setShowAnalyst(!showAnalyst)}
          >
            {showAnalyst ? 'close analyst' : 'analyst'}
          </button>
          <a href="/reasoning" className="badge" style={{ textDecoration: 'none', cursor: 'pointer', color: '#35d07f', borderColor: 'rgba(53,208,127,0.2)' }}>Φ→Γ reasoning</a>
          <a href="/debug" className="badge" style={{ textDecoration: 'none', cursor: 'pointer', color: '#e040fb', borderColor: 'rgba(224,64,251,0.2)' }}>neural debug</a>
          <a href="/demo" className="badge" style={{ textDecoration: 'none', cursor: 'pointer' }}>infra demo</a>
          <DiagnosticsBadge />
          <span className="badge good">notebook saved</span>
        </div>
      </section>

      {/* Row 2: Left rail / Graph / Inspector — each wrapped so a crash doesn't kill the shell */}
      <ErrorBoundary scope="LeftRail"><LeftRail /></ErrorBoundary>
      <ErrorBoundary scope="GraphPanel"><GraphPanel /></ErrorBoundary>
      <ErrorBoundary scope="Inspector"><Inspector /></ErrorBoundary>

      {/* Row 3: Result table OR Analyst panel */}
      {showAnalyst ? (
        <section className="panel table-panel" style={{ overflow: 'auto' }}>
          <div className="panel-header">
            <div className="panel-title">
              <h2>Political Analyst</h2>
              <span>NARS causality chains &middot; 6 analysis buckets &middot; thinking styles</span>
            </div>
          </div>
          <ErrorBoundary scope="AnalystPanel"><AnalystPanel /></ErrorBoundary>
        </section>
      ) : (
        <ErrorBoundary scope="ResultTable"><ResultTable /></ErrorBoundary>
      )}

      {/* Row 4: Notebook cells */}
      <ErrorBoundary scope="CellStrip"><CellStrip /></ErrorBoundary>

      {/* Status bar with data source indicators */}
      <footer className="status-bar">
        <div className="status-bar-left">
          <span className={`status-dot ${connected ? 'online' : 'offline'}`} />
          <span>{connected ? 'Connected' : 'Offline'}</span>
          <span className="status-sep" />
          <span>lance-graph v0.4.2</span>
          <span className="status-sep" />
          <span>{nodes.length} vertices &middot; {edges.length} edges</span>
          <span className="status-sep" />
          <DataStatusBar sources={dataSources} />
        </div>
        <div className="status-bar-right">
          {executing && <span className="status-executing">executing&hellip;</span>}
          <span>Cell [{cellCount.total}] {cellCount.running > 0 ? 'running' : 'idle'}</span>
          <span className="status-sep" />
          <span>MCP: 3 tools</span>
          <span className="status-sep" />
          <span>localhost:2718</span>
        </div>
      </footer>

      <div className="tooltip" id="tooltip" />

      {/* Diagnostics overlay — Shift+D to toggle */}
      <DiagnosticsOverlay />
    </div>
  );
}
