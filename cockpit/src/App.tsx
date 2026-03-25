import { useEffect, useMemo } from 'react';
import { useStore } from './store';
import { connectSSE } from './transport';
import { QueryBar } from './components/QueryBar';
import { GraphPanel } from './components/GraphPanel';
import { Inspector } from './components/Inspector';
import { ResultTable } from './components/ResultTable';
import { CellStrip } from './components/CellStrip';
import { LeftRail } from './components/LeftRail';

export function App() {
  const connected = useStore((s) => s.connected);
  const nodes = useStore((s) => s.nodes);
  const edges = useStore((s) => s.edges);
  const cells = useStore((s) => s.cells);
  const executing = useStore((s) => s.executing);

  useEffect(() => {
    connectSSE();
  }, []);

  const cellCount = useMemo(() => {
    const running = cells.filter((c) => c.execution_state === 'running').length;
    return { total: cells.length, running };
  }, [cells]);

  return (
    <div className="shell">
      {/* Row 1: Top bar */}
      <section className="topbar">
        <div className="brand">
          <small>q2 graph notebook</small>
          <h1>Cockpit</h1>
        </div>
        <QueryBar />
        <div className="top-actions">
          <span className={`badge ${connected ? 'good' : ''}`}>
            {connected ? 'mcp /sse live' : 'disconnected'}
          </span>
          <span className="badge">export PDF</span>
          <span className="badge good">notebook saved</span>
        </div>
      </section>

      {/* Row 2: Left rail / Graph / Inspector */}
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
          <span>3.2ms</span>
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
