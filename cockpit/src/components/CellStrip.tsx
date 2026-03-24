import { useStore } from '../store';
import { deleteCell } from '../transport';

const LANG_COLORS: Record<string, string> = {
  gremlin: 'gremlin',
  cypher: 'cypher',
  sparql: 'sparql',
  r: 'r',
  markdown: 'md',
  rust: 'md',
};

export function CellStrip() {
  const cells = useStore((s) => s.cells);

  return (
    <>
      <div className="panel-header">
        <span className="panel-title">Notebook</span>
        <span className="panel-badge">{cells.length} cells</span>
      </div>
      <div className="panel-body cells-body">
        {cells.length === 0 ? (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              padding: '20px',
              color: 'var(--text-muted)',
              fontSize: '12px',
            }}
          >
            Execute a query to add cells
          </div>
        ) : (
          cells.map((cell, index) => (
            <div
              key={cell.id}
              className={`cell ${cell.execution_state === 'running' ? 'running' : ''}`}
            >
              <div className="cell-gutter">
                <span
                  className={`cell-type-indicator ${LANG_COLORS[cell.language] || 'md'}`}
                >
                  {cell.language.slice(0, 3).toUpperCase()}
                </span>
                <span className="cell-execution-order">[{index + 1}]</span>
              </div>
              <div className="cell-content">
                <div className="cell-source">
                  <pre>{cell.source}</pre>
                </div>
                {cell.outputs.length > 0 && (
                  <div className="cell-output">
                    <div className="cell-output-label">
                      Output
                      <button
                        style={{
                          float: 'right',
                          background: 'none',
                          border: 'none',
                          color: 'var(--text-muted)',
                          cursor: 'pointer',
                          fontSize: '10px',
                        }}
                        onClick={() => deleteCell(cell.id)}
                        title="Delete cell"
                      >
                        {'\u2715'}
                      </button>
                    </div>
                    {cell.outputs.map((output, oi) => (
                      <div key={oi} className="cell-output-content">
                        {output.type === 'error' ? (
                          <pre style={{ color: 'var(--red)' }}>{output.content}</pre>
                        ) : output.type === 'graph' ? (
                          <div style={{ color: 'var(--accent)', fontSize: '11px' }}>
                            Graph rendered above
                          </div>
                        ) : output.type === 'html' ? (
                          <div dangerouslySetInnerHTML={{ __html: output.content }} />
                        ) : (
                          <pre>{output.content}</pre>
                        )}
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          ))
        )}
      </div>
    </>
  );
}
