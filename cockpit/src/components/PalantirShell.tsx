import { useEffect } from 'react';
import { DataSourceCard } from './DataSourceCard';
import { connectSSE } from '../transport';
import { useStore } from '../store';
import { AIWAR_QUERIES } from '../data/aiwar-seed';

interface PalantirShellProps {
  onLaunchDemo: () => void;
  onLaunchAiwar: () => void;
  onLaunchNotebook: () => void;
}

export function PalantirShell({ onLaunchDemo, onLaunchAiwar, onLaunchNotebook }: PalantirShellProps) {
  const connected = useStore((s) => s.connected);

  useEffect(() => {
    connectSSE();
  }, []);

  return (
    <div className="palantir-shell">
      {/* Header */}
      <header className="palantir-header">
        <div className="palantir-brand">
          <small>q2 graph engine</small>
          <h1>Q2 Cockpit</h1>
        </div>
        <div className="palantir-status">
          <span className={`badge ${connected ? 'good' : ''}`}>
            {connected ? 'Connected: lance-graph v0.4.2' : 'Disconnected'}
          </span>
        </div>
      </header>

      {/* Data source cards */}
      <section className="palantir-cards">
        <DataSourceCard
          title="Corporate Demo"
          subtitle="Infrastructure topology"
          nodeCount="24"
          edgeCount="32"
          description="Static seed data — servers, databases, services, load balancers. The original cockpit mockup."
          accent="#4dd0e1"
          onLaunch={onLaunchDemo}
        />
        <DataSourceCard
          title="AIWAR Graph"
          subtitle="AI in Warfare Research"
          nodeCount="221"
          edgeCount="356"
          description="51 AI weapons systems from academic research. Nations, companies, people, kill chains, surveillance."
          accent="#00d4ff"
          onLaunch={onLaunchAiwar}
        />
        <DataSourceCard
          title="Reasoning Notebook"
          subtitle="NARS + LLM inference"
          nodeCount="51+"
          edgeCount="growing"
          description="Project next week. OBSERVE, INFER, SEARCH, PROJECT, REVISE. Graph grows with each interaction."
          accent="#e040fb"
          onLaunch={onLaunchNotebook}
        />
        <DataSourceCard
          title="Custom Dataset"
          subtitle="Upload your own"
          nodeCount="?"
          edgeCount="?"
          description="Upload a .json graph file or connect to a remote lance-graph instance."
          accent="#93a9bf"
          onLaunch={() => {}}
          actionLabel="Upload"
        />
      </section>

      {/* Recent queries */}
      <section className="palantir-recent panel">
        <div className="panel-header">
          <div className="panel-title">
            <h2>Analytical Queries</h2>
            <span>pre-loaded from aiwar query collection</span>
          </div>
        </div>
        <div className="palantir-query-list">
          {AIWAR_QUERIES.slice(0, 4).map((q, i) => (
            <div key={i} className="palantir-query-item">
              <div className="palantir-query-title">{q.title}</div>
              <code className="palantir-query-code">{q.code.slice(0, 80)}...</code>
              <div className="palantir-query-desc">{q.description}</div>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
