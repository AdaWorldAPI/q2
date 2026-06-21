import { useState, useMemo } from 'react';
import { useStore } from '../store';

type SortDir = 1 | -1;

const STATUS_COLORS: Record<string, string> = {
  healthy: '#35d07f', warning: '#ffb547', critical: '#ff637d',
};

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

  // Degree + connection count map
  const degreeMap = useMemo(() => {
    const m: Record<string, number> = {};
    edges.forEach((e) => {
      m[e.source] = (m[e.source] || 0) + 1;
      m[e.target] = (m[e.target] || 0) + 1;
    });
    return m;
  }, [edges]);

  const sorted = useMemo(() => {
    return [...filteredNodes].sort((a, b) => {
      let va: string | number;
      let vb: string | number;

      if (sortKey === 'label') { va = a.label; vb = b.label; }
      else if (sortKey === 'type') { va = a.type; vb = b.type; }
      else if (sortKey === 'conns') { va = degreeMap[a.id] || 0; vb = degreeMap[b.id] || 0; }
      else { va = a.properties[sortKey] ?? ''; vb = b.properties[sortKey] ?? ''; }

      if (typeof va === 'number' && typeof vb === 'number') return (va - vb) * sortDir;
      return String(va).localeCompare(String(vb)) * sortDir;
    });
  }, [filteredNodes, sortKey, sortDir, degreeMap]);

  const handleSort = (key: string) => {
    if (sortKey === key) {
      setSortDir((d) => (d === 1 ? -1 : 1) as SortDir);
    } else {
      setSortKey(key);
      setSortDir(1);
    }
  };

  const columns = [
    { key: 'label', label: 'Label' },
    { key: 'type', label: 'Type' },
    { key: 'region', label: 'Region' },
    { key: 'status', label: 'Status' },
    { key: 'cpu', label: 'CPU' },
    { key: 'memory_gb', label: 'Mem (GB)' },
    { key: 'conns', label: 'Conns' },
  ];

  return (
    <section className="panel table-panel">
      <div className="panel-header">
        <div className="panel-title">
          <h2>Results</h2>
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
          <span className="badge">
            <svg width="12" height="12" viewBox="0 0 12 12" style={{ verticalAlign: '-1px', marginRight: '4px' }}>
              <path d="M6 1v7M3 5l3 3 3-3M2 10h8" stroke="currentColor" strokeWidth="1.2" fill="none" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
            export
          </span>
          <span className="badge good">selection synced</span>
        </div>
      </div>
      <div className="table-scroll">
        <table>
          <thead>
            <tr>
              {columns.map((col) => (
                <th key={col.key} onClick={() => handleSort(col.key)}>
                  {col.label}
                  {sortKey === col.key && (
                    <span style={{ marginLeft: '4px', opacity: 0.7 }}>
                      {sortDir === 1 ? '\u2191' : '\u2193'}
                    </span>
                  )}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {sorted.map((node) => {
              const statusStr = String(node.properties.status || '');
              const statusColor = STATUS_COLORS[statusStr] || '#93a9bf';
              const cpu = typeof node.properties.cpu === 'number' ? node.properties.cpu : null;
              const mem = typeof node.properties.memory_gb === 'number' ? node.properties.memory_gb : null;
              return (
                <tr
                  key={node.id}
                  className={node.id === selectedNodeId ? 'active' : ''}
                  onClick={() => selectNode(node.id)}
                >
                  <td><strong>{node.label}</strong></td>
                  <td>{node.type}</td>
                  <td>{String(node.properties.region || '')}</td>
                  <td>
                    <span className="status-pill" style={{ color: statusColor, borderColor: `${statusColor}40` }}>
                      {statusStr.toUpperCase()}
                    </span>
                  </td>
                  <td className={cpu !== null && cpu > 0.7 ? 'td-hot' : ''}>
                    {cpu !== null ? cpu.toFixed(2) : ''}
                  </td>
                  <td>{mem !== null ? mem.toFixed(1) : ''}</td>
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
