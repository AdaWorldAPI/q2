import { useState } from 'react';
import { AIWAR_QUERIES } from '../data/aiwar-seed';
import { executeQuery } from '../transport';

export function CypherConsole() {
  const [activeCell, setActiveCell] = useState(0);
  const [results, setResults] = useState<Record<number, string>>({});
  const [running, setRunning] = useState<number | null>(null);

  const handleRun = async (index: number) => {
    setRunning(index);
    try {
      const cell = await executeQuery(AIWAR_QUERIES[index].code, 'cypher');
      const textOutput = cell.outputs.find((o) => o.type === 'text' || o.type === 'table');
      setResults((prev) => ({
        ...prev,
        [index]: textOutput?.content || 'Query executed successfully',
      }));
    } catch (err) {
      setResults((prev) => ({
        ...prev,
        [index]: `Error: ${err instanceof Error ? err.message : 'Unknown error'}`,
      }));
    } finally {
      setRunning(null);
    }
  };

  return (
    <section className="panel cypher-console">
      <div className="panel-header">
        <div className="panel-title">
          <h2>Cypher Console</h2>
          <span>pre-loaded analytical queries</span>
        </div>
        <span className="badge">{AIWAR_QUERIES.length} queries</span>
      </div>
      <div className="cypher-cells">
        {AIWAR_QUERIES.map((q, i) => (
          <div
            key={i}
            className={`cypher-cell ${activeCell === i ? 'active' : ''} ${running === i ? 'running' : ''}`}
            onClick={() => setActiveCell(i)}
          >
            <div className="cypher-cell-head">
              <span className="cypher-cell-index">[{i + 1}]</span>
              <span className="cypher-cell-title">{q.title}</span>
              <div className="cypher-cell-actions">
                <button
                  className="cypher-run-btn"
                  onClick={(e) => { e.stopPropagation(); handleRun(i); }}
                  disabled={running !== null}
                >
                  {running === i ? '\u23F3' : '\u25B8'} Run
                </button>
              </div>
            </div>
            {activeCell === i && (
              <>
                <div className="cypher-cell-desc">{q.description}</div>
                <pre className="cypher-cell-code"><code>{q.code}</code></pre>
                {results[i] && (
                  <div className="cypher-cell-result">
                    <strong>Result</strong>
                    <pre>{results[i]}</pre>
                  </div>
                )}
              </>
            )}
          </div>
        ))}
      </div>
    </section>
  );
}
