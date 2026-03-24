import { useEffect } from 'react';
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

  useEffect(() => {
    connectSSE();
  }, []);

  return (
    <div className="shell">
      {/* ── Row 1: Top bar ── */}
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
        </div>
      </section>

      {/* ── Row 2: Left rail / Graph / Inspector ── */}
      <LeftRail />
      <GraphPanel />
      <Inspector />

      {/* ── Row 3: Result table ── */}
      <ResultTable />

      {/* ── Row 4: Notebook cells ── */}
      <CellStrip />

      {/* Tooltip container */}
      <div className="tooltip" id="tooltip" />
    </div>
  );
}
