import { useState, useMemo } from 'react';
import { useStore } from '../store';

type SortDir = 'asc' | 'desc' | null;

export function ResultTable() {
  const nodes = useStore((s) => s.nodes);
  const selectedNodeId = useStore((s) => s.selectedNodeId);
  const selectNode = useStore((s) => s.selectNode);
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortDir, setSortDir] = useState<SortDir>(null);

  // Derive column list from first node's properties + fixed cols
  const columns = useMemo(() => {
    if (nodes.length === 0) return [];
    const propKeys = Object.keys(nodes[0].properties);
    return [
      { key: 'label', label: 'Name', numeric: false },
      { key: 'type', label: 'Type', numeric: false },
      ...propKeys.map((k) => ({
        key: k,
        label: k,
        numeric: typeof nodes[0].properties[k] === 'number',
      })),
    ];
  }, [nodes]);

  const sorted = useMemo(() => {
    if (!sortCol || !sortDir) return nodes;
    return [...nodes].sort((a, b) => {
      const va = sortCol === 'label' ? a.label : sortCol === 'type' ? a.type : a.properties[sortCol];
      const vb = sortCol === 'label' ? b.label : sortCol === 'type' ? b.type : b.properties[sortCol];
      const sa = typeof va === 'string' ? va.toLowerCase() : va ?? '';
      const sb = typeof vb === 'string' ? vb.toLowerCase() : vb ?? '';
      if (sa < sb) return sortDir === 'asc' ? -1 : 1;
      if (sa > sb) return sortDir === 'asc' ? 1 : -1;
      return 0;
    });
  }, [nodes, sortCol, sortDir]);

  const handleSort = (key: string) => {
    if (sortCol === key) {
      setSortDir(sortDir === 'asc' ? 'desc' : sortDir === 'desc' ? null : 'asc');
      if (sortDir === 'desc') setSortCol(null);
    } else {
      setSortCol(key);
      setSortDir('asc');
    }
  };

  const getValue = (node: (typeof nodes)[0], key: string) => {
    if (key === 'label') return node.label;
    if (key === 'type') return node.type;
    const v = node.properties[key];
    if (typeof v === 'number') return Number.isInteger(v) ? v : v.toFixed(2);
    return String(v ?? '');
  };

  return (
    <>
      <div className="panel-header">
        <span className="panel-title">Results</span>
        <span className="panel-badge">{nodes.length} rows</span>
      </div>
      <div className="panel-body table-body">
        {nodes.length === 0 ? (
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              height: '100%',
              color: 'var(--text-muted)',
              fontSize: '12px',
              padding: '20px',
            }}
          >
            No results
          </div>
        ) : (
          <table className="result-table">
            <thead>
              <tr>
                {columns.map((col) => (
                  <th
                    key={col.key}
                    className={`sortable ${col.numeric ? 'numeric' : ''} ${sortCol === col.key ? 'sorted' : ''}`}
                    onClick={() => handleSort(col.key)}
                    data-col={col.key}
                  >
                    {col.label}
                    <span className="sort-icon">
                      {sortCol === col.key
                        ? sortDir === 'asc'
                          ? '\u2191'
                          : '\u2193'
                        : '\u2195'}
                    </span>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {sorted.map((node) => (
                <tr
                  key={node.id}
                  className={node.id === selectedNodeId ? 'selected' : ''}
                  onClick={() => selectNode(node.id)}
                >
                  {columns.map((col) => {
                    const val = getValue(node, col.key);
                    return (
                      <td key={col.key} className={col.numeric ? 'numeric' : ''}>
                        {col.key === 'status' ? (
                          <span
                            className={`status-badge ${val === 'healthy' ? 'healthy' : val === 'warning' ? 'warning' : 'critical'}`}
                          >
                            {val}
                          </span>
                        ) : (
                          String(val)
                        )}
                      </td>
                    );
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </>
  );
}
