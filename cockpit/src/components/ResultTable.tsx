import { useState, useMemo } from 'react';
import { useStore } from '../store';

type SortDir = 1 | -1;

export function ResultTable() {
  const nodes = useStore((s) => s.nodes);
  const edges = useStore((s) => s.edges);
  const filter = useStore((s) => s.filter);
  const selectedNodeId = useStore((s) => s.selectedNodeId);
  const selectNode = useStore((s) => s.selectNode);
  const searchTerm = useStore((s) => s.searchTerm);
  const setSearchTerm = useStore((s) => s.setSearchTerm);
  const [sortKey, setSortKey] = useState('label');
  const [sortDir, setSortDir] = useState<SortDir>(1);

  // Apply filter
  const filteredNodes = useMemo(() => {
    let result = nodes;
    if (filter !== 'all') {
      if (filter === 'warning') {
        result = result.filter((n) => n.properties.status !== 'healthy');
      } else {
        result = result.filter((n) => n.type === filter);
      }
    }
    if (searchTerm) {
      const q = searchTerm.toLowerCase();
      result = result.filter((n) =>
        [n.id, n.label, n.type, ...Object.values(n.properties).map(String)]
          .join(' ')
          .toLowerCase()
          .includes(q),
      );
    }
    return result;
  }, [nodes, filter, searchTerm]);

  const sorted = useMemo(() => {
    return [...filteredNodes].sort((a, b) => {
      const va = sortKey === 'label' ? a.label : sortKey === 'type' ? a.type : a.properties[sortKey];
      const vb = sortKey === 'label' ? b.label : sortKey === 'type' ? b.type : b.properties[sortKey];
      if (typeof va === 'number' && typeof vb === 'number') return (va - vb) * sortDir;
      return String(va ?? '').localeCompare(String(vb ?? '')) * sortDir;
    });
  }, [filteredNodes, sortKey, sortDir]);

  const handleSort = (key: string) => {
    if (sortKey === key) {
      setSortDir((d) => (d === 1 ? -1 : 1) as SortDir);
    } else {
      setSortKey(key);
      setSortDir(1);
    }
  };

  // Degree column
  const degreeMap = useMemo(() => {
    const m: Record<string, number> = {};
    edges.forEach((e) => {
      m[e.source] = (m[e.source] || 0) + 1;
      m[e.target] = (m[e.target] || 0) + 1;
    });
    return m;
  }, [edges]);

  const columns = [
    { key: 'label', label: 'Entity' },
    { key: 'type', label: 'Type' },
    { key: 'status', label: 'Status' },
    { key: 'region', label: 'Region' },
    { key: 'cpu', label: 'CPU' },
    { key: 'memory', label: 'Memory' },
  ];

  return (
    <section className="panel table-panel">
      <div className="panel-header">
        <div className="panel-title">
          <h2>Result table</h2>
          <span>dense &middot; sortable &middot; same entities as the graph</span>
        </div>
        <div className="signal">row click = graph highlight</div>
      </div>
      <div className="table-toolbar">
        <input
          className="table-search"
          placeholder="filter nodes, status, region..."
          value={searchTerm}
          onChange={(e) => setSearchTerm(e.target.value)}
        />
        <div className="mini-status">
          <span className="badge">{filteredNodes.length} rows</span>
          <span className="badge good">selection synced</span>
        </div>
      </div>
      <div className="table-scroll">
        <table>
          <thead>
            <tr>
              {columns.map((col) => (
                <th key={col.key} onClick={() => handleSort(col.key)} data-sort={col.key}>
                  {col.label}
                  {sortKey === col.key && (
                    <span style={{ marginLeft: '4px', opacity: 0.7 }}>
                      {sortDir === 1 ? '\u2191' : '\u2193'}
                    </span>
                  )}
                </th>
              ))}
              <th onClick={() => handleSort('degree')}>Degree</th>
            </tr>
          </thead>
          <tbody>
            {sorted.map((node) => {
              const statusStr = String(node.properties.status || '');
              const statusColor =
                statusStr === 'healthy' ? '#35d07f' : statusStr === 'warning' ? '#ffb547' : '#ff637d';
              return (
                <tr
                  key={node.id}
                  className={node.id === selectedNodeId ? 'active' : ''}
                  onClick={() => selectNode(node.id)}
                >
                  <td>{node.label}</td>
                  <td>{node.type}</td>
                  <td style={{ color: statusColor }}>{statusStr}</td>
                  <td>{String(node.properties.region || '')}</td>
                  <td>
                    {typeof node.properties.cpu === 'number'
                      ? (node.properties.cpu * 100).toFixed(0) + '%'
                      : String(node.properties.cpu || '')}
                  </td>
                  <td>
                    {typeof node.properties.memory === 'number'
                      ? node.properties.memory.toFixed(1) + ' GB'
                      : String(node.properties.memory || '')}
                  </td>
                  <td>{degreeMap[node.id] || 0}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}
