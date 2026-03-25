// PalantirApp — dynamic cockpit landing + dataset explorer
// / route: corporate Palantir dashboard connected to lance-graph

import { useState, useEffect, useCallback } from 'react';
import { AiwarExplorer } from './components/AiwarExplorer';
import { useStore } from './store';
import { QueryBar } from './components/QueryBar';
import { GraphPanel } from './components/GraphPanel';
import { Inspector } from './components/Inspector';
import { ResultTable } from './components/ResultTable';
import { CellStrip } from './components/CellStrip';
import { LeftRail } from './components/LeftRail';

type ActiveView = 'landing' | 'corporate' | 'aiwar' | 'notebook';

// MCP call helper
async function mcpCall(tool: string, args: Record<string, unknown>) {
  const res = await fetch('/mcp/message', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id: Date.now(),
      method: 'tools/call',
      params: { name: tool, arguments: args },
    }),
  });
  const json = await res.json();
  if (json.error) throw new Error(json.error.message);
  return json.result;
}

interface DataSourceCardProps {
  title: string;
  description: string;
  nodes: number;
  edges: string;
  accent: string;
  icon: string;
  onClick: () => void;
}

function DataSourceCard({ title, description, nodes, edges, accent, icon, onClick }: DataSourceCardProps) {
  return (
    <button className="datasource-card" onClick={onClick}>
      <div className="datasource-icon" style={{ color: accent, borderColor: `${accent}40` }}>
        {icon}
      </div>
      <h3 style={{ color: accent }}>{title}</h3>
      <p>{description}</p>
      <div className="datasource-stats">
        <span>{nodes} nodes</span>
        <span>{edges}</span>
      </div>
      <div className="datasource-launch" style={{ borderColor: `${accent}30`, color: accent }}>
        Launch &rarr;
      </div>
    </button>
  );
}

export function PalantirApp() {
  // Default to corporate cockpit (Image 1 layout with seed data)
  const [view, setView] = useState<ActiveView>('corporate');
  const [connected, setConnected] = useState(false);
  const setGraphData = useStore((s) => s.setGraphData);

  // SSE connection
  useEffect(() => {
    const es = new EventSource('/mcp/sse');
    es.onopen = () => setConnected(true);
    es.onmessage = () => {};
    es.onerror = () => setConnected(false);
    return () => es.close();
  }, []);

  const loadCorporate = useCallback(async () => {
    setView('corporate');
    // Load the infrastructure demo through MCP
    try {
      const result = await mcpCall('demo_load', { dataset: 'infrastructure' });
      if (result?.outputs) {
        const graphOutput = result.outputs.find((o: any) => o.type === 'graph');
        if (graphOutput) {
          const data = JSON.parse(graphOutput.content);
          if (data.nodes && data.edges) {
            setGraphData(data.nodes, data.edges);
          }
        }
      }
    } catch {
      // Use seed data already in store
    }
  }, [setGraphData]);

  // Landing page
  if (view === 'landing') {
    return (
      <div className="palantir-landing">
        <header className="palantir-topbar">
          <div className="brand">
            <small>q2 graph engine</small>
            <h1>Cockpit</h1>
          </div>
          <div className="top-actions">
            <span className={`badge ${connected ? 'good' : ''}`}>
              {connected ? 'lance-graph connected' : 'connecting...'}
            </span>
          </div>
        </header>

        <main className="palantir-grid">
          <DataSourceCard
            title="Corporate Demo"
            description="Network topology — servers, databases, caches, load balancers, queues"
            nodes={24}
            edges="32 edges"
            accent="#4dd0e1"
            icon="&#9678;"
            onClick={loadCorporate}
          />
          <DataSourceCard
            title="AI War Graph"
            description="AI weapons systems — academic dataset with NARS reasoning"
            nodes={221}
            edges="356 edges"
            accent="#ff9800"
            icon="&#9733;"
            onClick={() => setView('aiwar')}
          />
          <DataSourceCard
            title="Reasoning Notebook"
            description="Project conflict trajectories — NARS + LLM + temporal inference"
            nodes={51}
            edges="growing"
            accent="#ab47bc"
            icon="&#9881;"
            onClick={() => setView('notebook')}
          />
          <DataSourceCard
            title="Custom Dataset"
            description="Upload .json or .csv — lance-graph parses and indexes"
            nodes={0}
            edges="upload"
            accent="#78909c"
            icon="&#8853;"
            onClick={() => {}}
          />
        </main>

        <footer className="status-bar">
          <div className="status-bar-left">
            <span className={`status-dot ${connected ? 'online' : 'offline'}`} />
            <span>{connected ? 'Connected' : 'Offline'}</span>
            <span className="status-sep" />
            <span>lance-graph v0.4.2</span>
            <span className="status-sep" />
            <span>Arrow 57 &middot; DataFusion 51</span>
          </div>
          <div className="status-bar-right">
            <span>MCP: 18 tools</span>
            <span className="status-sep" />
            <span>Rust 1.94</span>
          </div>
        </footer>
      </div>
    );
  }

  // Aiwar explorer
  if (view === 'aiwar') {
    return (
      <div className="palantir-explorer">
        <header className="palantir-topbar">
          <button className="pill" onClick={() => setView('landing')}>&larr; Back</button>
          <div className="brand">
            <small>q2 &rsaquo; aiwar</small>
            <h1>AI in Warfare</h1>
          </div>
          <div className="top-actions">
            <span className={`badge ${connected ? 'good' : ''}`}>
              {connected ? 'lance-graph live' : 'disconnected'}
            </span>
          </div>
        </header>
        <AiwarExplorer mcpCall={mcpCall} />
      </div>
    );
  }

  // Reasoning notebook (placeholder — next PR)
  if (view === 'notebook') {
    return (
      <div className="palantir-explorer">
        <header className="palantir-topbar">
          <button className="pill" onClick={() => setView('landing')}>&larr; Back</button>
          <div className="brand">
            <small>q2 &rsaquo; reasoning</small>
            <h1>Notebook</h1>
          </div>
        </header>
        <div style={{ padding: '40px', textAlign: 'center', color: 'var(--muted)' }}>
          <h2>Reasoning Notebook</h2>
          <p>OBSERVE &rarr; INFER &rarr; SEARCH &rarr; PROJECT &rarr; REVISE</p>
          <p style={{ marginTop: '16px', fontSize: '12px' }}>Coming in the next PR. The 5-cell notebook that builds a living knowledge graph.</p>
        </div>
      </div>
    );
  }

  // Corporate cockpit (reuses existing components with live MCP data)
  return (
    <div className="shell">
      <section className="topbar">
        <div className="brand">
          <button className="pill" onClick={() => setView('landing')} style={{ marginBottom: '4px' }}>
            &larr; Back
          </button>
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
      <LeftRail />
      <GraphPanel />
      <Inspector />
      <ResultTable />
      <CellStrip />
      <footer className="status-bar">
        <div className="status-bar-left">
          <span className={`status-dot ${connected ? 'online' : 'offline'}`} />
          <span>{connected ? 'Connected' : 'Offline'}</span>
          <span className="status-sep" />
          <span>lance-graph v0.4.2</span>
        </div>
        <div className="status-bar-right">
          <span>MCP: 18 tools</span>
        </div>
      </footer>
    </div>
  );
}
