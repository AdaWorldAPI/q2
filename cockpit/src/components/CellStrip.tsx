import { useEffect, useState, useRef, useMemo } from 'react';
import { useStore } from '../store';
import { deleteCell } from '../transport';

// Seed notebook cells that always render (matching screenshot 2)
const SEED_CELLS = [
  {
    id: 'seed-md',
    language: 'markdown',
    source: '',
    index: 0,
  },
  {
    id: 'seed-gremlin',
    language: 'gremlin',
    source: `g.V().hasLabel('server')
    .project('name', 'region', 'cpu')
    .by('name')
    .by('region')
    .by('cpu_avg')`,
    index: 1,
  },
  {
    id: 'seed-r',
    language: 'r',
    source: `library(ggplot2)

cpu_data <- as.data.frame(cell[1])
ggplot(cpu_data, aes(x = name, y = cpu, fill = region)) +
    geom_col() +
    theme_minimal() +
    labs(title = "CPU Usage by Server")`,
    index: 2,
  },
  {
    id: 'seed-sparql',
    language: 'sparql',
    source: `SELECT ?server ?uptime
WHERE {
    ?server a :Server ;
            :hasUptime ?uptime ;
            :inRegion ?region .
    FILTER(?region = "us-east-1")
}
ORDER BY DESC(?uptime)`,
    index: 3,
  },
];

const LANG_COLORS: Record<string, string> = {
  gremlin: '#4dd0e1',
  cypher: '#4dd0e1',
  r: '#66bb6a',
  sparql: '#ab47bc',
  markdown: '#93a9bf',
  python: '#ffd166',
};

export function CellStrip() {
  const cells = useStore((s) => s.cells);
  const nodes = useStore((s) => s.nodes);
  const executing = useStore((s) => s.executing);
  const selectedNode = useStore((s) => s.nodes.find((n) => n.id === s.selectedNodeId));
  const [runningCells, setRunningCells] = useState<Set<string>>(new Set());
  const prevExecuting = useRef(executing);

  // Reactive cascade animation
  useEffect(() => {
    if (prevExecuting.current && !executing && cells.length > 0) {
      const ids = cells.map((c) => c.id);
      ids.forEach((id, i) => {
        setTimeout(() => {
          setRunningCells((prev) => new Set([...prev, id]));
          setTimeout(() => {
            setRunningCells((prev) => {
              const next = new Set(prev);
              next.delete(id);
              return next;
            });
          }, 1200);
        }, i * 260);
      });
    }
    prevExecuting.current = executing;
  }, [executing, cells]);

  // Status summary
  const statusSummary = useMemo(() => {
    if (nodes.length === 0) return null;
    const healthy = nodes.filter((n) => n.properties.status === 'healthy').length;
    const warning = nodes.filter((n) => n.properties.status === 'warning').length;
    const critical = nodes.filter((n) => n.properties.status === 'critical').length;
    return { healthy, warning, critical, total: nodes.length };
  }, [nodes]);

  // Combine seed cells with real cells
  const displayCells = useMemo(() => {
    if (cells.length > 0) {
      return cells.map((c, i) => ({ ...c, index: i + 1, seed: false }));
    }
    // Show seed cells when no real cells exist
    return SEED_CELLS.map((c, i) => ({ ...c, execution_state: 'idle', outputs: [] as never[], index: i, seed: true }));
  }, [cells]);

  // Take first 4 cells for the strip
  const visibleCells = displayCells.slice(0, 4);

  if (visibleCells.length === 0) {
    return (
      <section className="cells-panel cells-empty">
        <div style={{
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          height: '100%', color: 'var(--muted)', fontSize: '12px',
          letterSpacing: '0.06em', textTransform: 'uppercase',
          gridColumn: '1 / -1',
        }}>
          Execute a query to populate cells
        </div>
      </section>
    );
  }

  return (
    <section className="cells-panel">
      <div className="cells-header">
        <div className="panel-title">
          <h2>Notebook</h2>
        </div>
        <button className="badge" style={{ cursor: 'pointer' }}>+ Add cell</button>
      </div>
      <div className="cells-strip">
        {visibleCells.map((cell) => {
          const isRunning = runningCells.has(cell.id) || (executing && cell.index === 1);
          const langColor = LANG_COLORS[cell.language] || '#4dd0e1';
          const isMd = cell.language === 'markdown';

          return (
            <article key={cell.id} className={`cell ${isRunning ? 'running' : ''}`}>
              <div className="cell-head">
                <div className="cell-head-left">
                  <div className="cell-index" style={{ borderColor: `${langColor}40`, color: langColor }}>
                    {isMd ? 'MD' : cell.language === 'gremlin' ? 'GR' : cell.language === 'sparql' ? 'SQ' : cell.language.toUpperCase().slice(0, 2)}
                  </div>
                  <span className="lang-chip">{cell.language}</span>
                </div>
                <span className={`badge ${cell.index <= 1 ? 'good' : cell.index === 2 ? 'warn' : ''}`}>
                  {cell.index <= 1
                    ? 'upstream'
                    : cell.index === 2
                    ? 'downstream'
                    : 'document view'}
                </span>
              </div>
              <div className="cell-body">
                {isMd ? (
                  <div className="markdown-card">
                    <h4>Network Topology Analysis</h4>
                    <p>
                      {selectedNode
                        ? <>Selected node <strong>{selectedNode.label}</strong> belongs to <strong>{String(selectedNode.properties.region || 'unknown')}</strong>. Exporting to PDF should preserve this exact state: code, graph, table, note.</>
                        : 'Notebook is the report, not a second-class export target. Panels stay visible; no wandering through tabs.'}
                    </p>
                    <ul>
                      <li>Every click is a tiny orchestral cue for the rest of the interface.</li>
                      <li>Reactive cells re-run when upstream data changes.</li>
                    </ul>
                  </div>
                ) : (
                  <>
                    <pre><code style={{ color: langColor }}>{cell.source}</code></pre>
                    <div className="cell-result">
                      {cell.language === 'r' && statusSummary ? (
                        <>
                          <strong>Reactive summary</strong>
                          <p className="cell-result-desc">
                            This cell re-runs whenever the upstream graph cell changes. In the real
                            product, the R kernel would emit structured output and charts without
                            a manual run-all ritual.
                          </p>
                          <table className="mini-table">
                            <tbody>
                              <tr><td>healthy</td><td style={{ color: '#35d07f' }}>{statusSummary.healthy} / {statusSummary.total}</td></tr>
                              <tr><td>warning</td><td style={{ color: '#ffb547' }}>{statusSummary.warning} / {statusSummary.total}</td></tr>
                              <tr><td>critical</td><td style={{ color: '#ff637d' }}>{statusSummary.critical} / {statusSummary.total}</td></tr>
                            </tbody>
                          </table>
                        </>
                      ) : cell.language === 'gremlin' ? (
                        <>
                          <strong>Graph result</strong>
                          <p className="cell-result-desc">
                            Local force view renders the queried subgraph as SVG/HTML. This would be supplied
                            by notebook-render and hydrated into the cockpit panel above.
                          </p>
                          <table className="mini-table">
                            <tbody>
                              <tr><td>Nodes returned</td><td>{nodes.length}</td></tr>
                              <tr><td>Edges returned</td><td>{useStore.getState().edges.length}</td></tr>
                              <tr><td>Primary focus</td><td>{selectedNode?.label || 'none'}</td></tr>
                            </tbody>
                          </table>
                        </>
                      ) : (
                        <>
                          <strong>SPARQL result</strong>
                          <p className="cell-result-desc">
                            Federated query against the knowledge graph. Results would populate
                            the table panel above with semantic triples.
                          </p>
                        </>
                      )}
                      {!cell.seed && (
                        <button
                          className="cell-delete"
                          onClick={() => deleteCell(cell.id)}
                          title="Delete cell"
                        >
                          remove
                        </button>
                      )}
                    </div>
                  </>
                )}
              </div>
            </article>
          );
        })}
      </div>
    </section>
  );
}
