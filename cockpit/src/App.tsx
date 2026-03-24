import { useEffect } from 'react';
import { useStore } from './store';
import { connectSSE } from './transport';
import { QueryBar } from './components/QueryBar';
import { GraphPanel } from './components/GraphPanel';
import { Inspector } from './components/Inspector';
import { ResultTable } from './components/ResultTable';
import { CellStrip } from './components/CellStrip';

export function App() {
  const connected = useStore((s) => s.connected);

  useEffect(() => {
    connectSSE();
  }, []);

  return (
    <>
      {/* Top bar */}
      <div id="topbar">
        <div className="topbar-left">
          <span className="logo">
            q<span className="logo-accent">2</span>
          </span>
          <div className="breadcrumb">
            <span className="breadcrumb-item">notebooks</span>
            <span className="breadcrumb-sep">/</span>
            <span className="breadcrumb-item active">network-topology</span>
          </div>
        </div>

        <div className="topbar-center">
          <QueryBar />
        </div>

        <div className="topbar-right">
          <div className="topbar-status">
            <span
              className={`status-dot ${connected ? 'connected' : ''}`}
            />
            <span>{connected ? 'Connected' : 'Disconnected'}</span>
          </div>
        </div>
      </div>

      {/* Cockpit grid */}
      <div id="cockpit">
        <div id="graphPanel" className="panel">
          <GraphPanel />
        </div>
        <div id="propertiesPanel" className="panel">
          <Inspector />
        </div>
        <div id="tablePanel" className="panel">
          <ResultTable />
        </div>
        <div id="cellStack" className="panel">
          <CellStrip />
        </div>
      </div>

      {/* Status bar */}
      <div id="statusbar">
        <div className="statusbar-left">
          <span className="status-item">
            <span
              className={`status-dot ${connected ? 'connected' : ''}`}
            />
            lance-graph
          </span>
          <span className="status-item">MCP 2024-11-05</span>
        </div>
        <div className="statusbar-right">
          <span className="status-item">q2 v0.1.0</span>
        </div>
      </div>
    </>
  );
}
