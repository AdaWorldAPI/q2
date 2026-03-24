import { useEffect, useState, useRef, useMemo } from 'react';
import { useStore } from '../store';
import { deleteCell } from '../transport';

export function CellStrip() {
  const cells = useStore((s) => s.cells);
  const nodes = useStore((s) => s.nodes);
  const executing = useStore((s) => s.executing);
  const selectedNode = useStore((s) => s.nodes.find((n) => n.id === s.selectedNodeId));
  const [runningCells, setRunningCells] = useState<Set<string>>(new Set());
  const prevExecuting = useRef(executing);

  // Reactive cascade animation: when execution completes, pulse through cells
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

  // Status summary for R cell
  const statusSummary = useMemo(() => {
    if (nodes.length === 0) return null;
    const healthy = nodes.filter((n) => n.properties.status === 'healthy').length;
    const warning = nodes.filter((n) => n.properties.status === 'warning').length;
    const critical = nodes.filter((n) => n.properties.status === 'critical').length;
    return { healthy, warning, critical, total: nodes.length };
  }, [nodes]);

  // Build display cells: real cells + synthetic R and markdown cells
  const displayCells = useMemo(() => {
    const result = cells.map((c, i) => ({ ...c, index: i + 1, synthetic: false as const }));

    // Always show an R summary cell and a markdown note cell
    if (nodes.length > 0 && cells.length > 0) {
      result.push({
        id: 'synthetic-r',
        source: 'services |>\n  dplyr::count(status, sort = TRUE) |>\n  dplyr::mutate(pct = scales::percent(n / sum(n)))',
        language: 'r',
        execution_state: 'success',
        outputs: [],
        index: result.length + 1,
        synthetic: false,
      });
      result.push({
        id: 'synthetic-md',
        source: '',
        language: 'markdown',
        execution_state: 'success',
        outputs: [],
        index: result.length + 2,
        synthetic: false,
      });
    }

    return result;
  }, [cells, nodes]);

  if (displayCells.length === 0) {
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
      {displayCells.slice(0, 3).map((cell) => {
        const isRunning = runningCells.has(cell.id) || (executing && cell.id === displayCells[0]?.id);
        const isR = cell.language === 'r';
        const isMd = cell.language === 'markdown';

        return (
          <article key={cell.id} className={`cell ${isRunning ? 'running' : ''}`}>
            <div className="cell-head">
              <div className="cell-head-left">
                <div className="cell-index">{cell.index}</div>
                <span className="lang-chip">{cell.language}</span>
              </div>
              <span className={`badge ${cell.index === 1 ? 'good' : cell.index === 2 ? 'warn' : ''}`}>
                {cell.index === 1 ? 'upstream' : cell.index === 2 ? 'downstream' : 'document'}
              </span>
            </div>
            <div className="cell-body">
              {!isMd && (
                <pre>{cell.source}</pre>
              )}
              <div className="cell-result">
                {isMd ? (
                  <div className="markdown-card">
                    <h4>Operational note</h4>
                    <p>
                      {selectedNode
                        ? <>Selected node <strong>{selectedNode.label}</strong> ({selectedNode.type}) is in <strong>{String(selectedNode.properties.region || 'unknown')}</strong>. Exporting to PDF preserves this exact state: code, graph, table, note.</>
                        : 'Notebook is the report, not a second-class export target. Every click is a tiny orchestral cue for the rest of the interface.'
                      }
                    </p>
                    <ul>
                      <li>Panels stay visible; no wandering through tabs.</li>
                      <li>Every click is a tiny orchestral cue for the rest of the interface.</li>
                    </ul>
                  </div>
                ) : isR && statusSummary ? (
                  <>
                    <strong>Reactive summary</strong>
                    <p style={{ color: 'var(--muted)', fontSize: '12px', margin: '6px 0' }}>
                      This cell re-runs whenever the upstream graph cell changes.
                    </p>
                    <table className="mini-table">
                      <tbody>
                        <tr><td>healthy</td><td style={{ color: '#35d07f' }}>{statusSummary.healthy} / {statusSummary.total}</td></tr>
                        <tr><td>warning</td><td style={{ color: '#ffb547' }}>{statusSummary.warning} / {statusSummary.total}</td></tr>
                        <tr><td>critical</td><td style={{ color: '#ff637d' }}>{statusSummary.critical} / {statusSummary.total}</td></tr>
                      </tbody>
                    </table>
                  </>
                ) : (
                  <>
                    <strong>Graph result</strong>
                    <p style={{ color: 'var(--muted)', fontSize: '12px', margin: '6px 0' }}>
                      Force view renders the queried subgraph. Supplied by <code>notebook-render</code>.
                    </p>
                    <table className="mini-table">
                      <tbody>
                        <tr><td>Nodes returned</td><td>{nodes.length}</td></tr>
                        <tr><td>Edges returned</td><td>{useStore.getState().edges.length}</td></tr>
                        <tr><td>Primary focus</td><td>{selectedNode?.label || 'none'}</td></tr>
                      </tbody>
                    </table>
                    {!cell.id.startsWith('synthetic') && (
                      <button
                        className="cell-delete"
                        onClick={() => deleteCell(cell.id)}
                        title="Delete cell"
                      >
                        remove
                      </button>
                    )}
                  </>
                )}
              </div>
            </div>
          </article>
        );
      })}
    </section>
  );
}
