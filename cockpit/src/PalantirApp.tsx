import { useEffect, useMemo, useState, useCallback, useRef } from 'react';
import { useStore } from './store';
import { connectSSE } from './transport';
import { QueryBar } from './components/QueryBar';
import { GraphPanel } from './components/GraphPanel';
import { Inspector } from './components/Inspector';
import { ResultTable } from './components/ResultTable';
import { CellStrip } from './components/CellStrip';
import { LeftRail } from './components/LeftRail';
import { convertAiwarGraph, type ReasoningResult } from './data/aiwar-seed';

/**
 * PalantirApp — IDENTICAL layout to DemoApp (the Palantir screenshot),
 * but loaded with the 221-node aiwar graph instead of seed data.
 * Same shell, same panels, same glass morphism. Different data.
 */
export function PalantirApp() {
  const connected = useStore((s) => s.connected);
  const nodes = useStore((s) => s.nodes);
  const edges = useStore((s) => s.edges);
  const cells = useStore((s) => s.cells);
  const executing = useStore((s) => s.executing);
  const setGraphData = useStore((s) => s.setGraphData);

  const [loaded, setLoaded] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const loadedRef = useRef(false);

  useEffect(() => { connectSSE(); }, []);

  // Load aiwar data into the store on mount
  useEffect(() => {
    if (loadedRef.current) return;
    loadedRef.current = true;
    (async () => {
      try {
        const res = await fetch('/aiwar_graph.json');
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const raw = await res.json();
        const { nodes: gNodes, edges: gEdges } = convertAiwarGraph(raw);
        setGraphData(gNodes, gEdges);
        setLoaded(true);
      } catch (err) {
        setLoadError(err instanceof Error ? err.message : 'Failed to load');
        setLoaded(true);
      }
    })();
  }, [setGraphData]);

  const cellCount = useMemo(() => {
    const running = cells.filter((c) => c.execution_state === 'running').length;
    return { total: cells.length, running };
  }, [cells]);

  return (
    <div className="shell">
      {/* Row 1: Top bar — same as DemoApp */}
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
          {loadError && <span className="badge hot" title={loadError}>data error</span>}
          <a href="/debug" className="badge" style={{ textDecoration: 'none', cursor: 'pointer', color: '#e040fb', borderColor: 'rgba(224,64,251,0.2)' }}>neural debug</a>
          <a href="/demo" className="badge" style={{ textDecoration: 'none', cursor: 'pointer' }}>infra demo</a>
          <span className="badge good">notebook saved</span>
        </div>
      </section>

      {/* Row 2: Left rail / Graph / Inspector — SAME components as DemoApp */}
      <LeftRail />
      <GraphPanel />
      <Inspector />

      {/* Row 3: Result table */}
      <ResultTable />

      {/* Row 4: Notebook cells */}
      <CellStrip />

      {/* Status bar (bottom) */}
      <footer className="status-bar">
        <div className="status-bar-left">
          <span className={`status-dot ${connected ? 'online' : 'offline'}`} />
          <span>{connected ? 'Connected' : 'Offline'}</span>
          <span className="status-sep" />
          <span>lance-graph v0.4.2</span>
          <span className="status-sep" />
          <span>{nodes.length} vertices &middot; {edges.length} edges</span>
          <span className="status-sep" />
          <span>{loaded ? 'aiwar loaded' : 'loading...'}</span>
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

      {/* Tooltip container */}
      <div className="tooltip" id="tooltip" />
    </div>
  );
}
