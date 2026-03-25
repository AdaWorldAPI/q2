// Multi-cell Cypher console — executes queries through lance-graph via MCP

import { useState, useCallback } from 'react';

interface CypherCell {
  id: string;
  code: string;
  lang: string;
  result: string | null;
  elapsed_ms: number | null;
  running: boolean;
}

interface CypherConsoleProps {
  preloadedQueries: Array<{ label: string; code: string; lang: string }>;
  onExecute: (code: string, lang: string) => Promise<{ raw_output: string; elapsed_ms: number; graph_json?: string }>;
  onNarsReason?: (code: string) => void;
}

export function CypherConsole({ preloadedQueries, onExecute, onNarsReason }: CypherConsoleProps) {
  const [cells, setCells] = useState<CypherCell[]>(() =>
    preloadedQueries.slice(0, 3).map((q, i) => ({
      id: `cypher-${i}`,
      code: q.code,
      lang: q.lang,
      result: null,
      elapsed_ms: null,
      running: false,
    })),
  );

  const runCell = useCallback(async (id: string) => {
    const cell = cells.find((c) => c.id === id);
    if (!cell) return;

    setCells((prev) => prev.map((c) => (c.id === id ? { ...c, running: true } : c)));

    try {
      const result = await onExecute(cell.code, cell.lang);
      setCells((prev) =>
        prev.map((c) =>
          c.id === id
            ? { ...c, result: result.raw_output, elapsed_ms: result.elapsed_ms, running: false }
            : c,
        ),
      );
    } catch (e) {
      setCells((prev) =>
        prev.map((c) =>
          c.id === id ? { ...c, result: `Error: ${e}`, running: false } : c,
        ),
      );
    }
  }, [cells, onExecute]);

  const updateCode = useCallback((id: string, code: string) => {
    setCells((prev) => prev.map((c) => (c.id === id ? { ...c, code } : c)));
  }, []);

  const addCell = useCallback(() => {
    setCells((prev) => [
      ...prev,
      { id: `cypher-${Date.now()}`, code: '', lang: 'cypher', result: null, elapsed_ms: null, running: false },
    ]);
  }, []);

  return (
    <div className="cypher-console">
      <div className="cypher-header">
        <div className="section-label">Cypher Console</div>
        <button className="badge" onClick={addCell} style={{ cursor: 'pointer' }}>
          + Add Cell
        </button>
      </div>
      <div className="cypher-cells">
        {cells.map((cell, i) => (
          <div key={cell.id} className={`cypher-cell ${cell.running ? 'running' : ''}`}>
            <div className="cypher-cell-head">
              <span className="cell-index">{i + 1}</span>
              <span className="lang-chip">{cell.lang}</span>
              <div style={{ flex: 1 }} />
              <button
                className="button"
                onClick={() => runCell(cell.id)}
                disabled={cell.running}
                style={{ padding: '6px 10px', fontSize: '11px' }}
              >
                {cell.running ? 'running...' : 'Run'}
              </button>
              {onNarsReason && cell.result && (
                <button
                  className="button"
                  onClick={() => onNarsReason(cell.code)}
                  style={{ padding: '6px 10px', fontSize: '11px' }}
                >
                  NARS Reason
                </button>
              )}
            </div>
            <textarea
              className="cypher-input"
              value={cell.code}
              onChange={(e) => updateCode(cell.id, e.target.value)}
              rows={Math.min(cell.code.split('\n').length + 1, 6)}
              spellCheck={false}
            />
            {cell.result && (
              <div className="cypher-result">
                <div className="cypher-result-meta">
                  {cell.elapsed_ms !== null && <span className="badge">{cell.elapsed_ms}ms</span>}
                </div>
                <pre>{cell.result.slice(0, 2000)}</pre>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
