import { useState, useMemo, useEffect, useRef, useCallback } from 'react';
import { Network, type Options } from 'vis-network';
import { DataSet } from 'vis-data';
import type { GraphNode, GraphEdge } from '../store';
import type { AiwarWeapon } from '../hooks/useAiwarData';
import { NarsPanel } from './NarsPanel';
import { CypherConsole } from './CypherConsole';
import { AIWAR_TYPE_COLORS, type ReasoningResult } from '../data/aiwar-seed';

interface AiwarExplorerProps {
  nodes: GraphNode[];
  edges: GraphEdge[];
  weapons: AiwarWeapon[];
  onBack: () => void;
}

type ViewMode = 'table' | 'graph';
type FilterKey = 'all' | 'System' | 'Stakeholder' | 'CivicSystem' | 'Person' | 'HistoricalSystem';

const FILTERS: { key: FilterKey; label: string }[] = [
  { key: 'all', label: 'All' },
  { key: 'System', label: 'Systems' },
  { key: 'Stakeholder', label: 'Stakeholders' },
  { key: 'CivicSystem', label: 'Civic' },
  { key: 'Person', label: 'People' },
  { key: 'HistoricalSystem', label: 'Historical' },
];

const MILITARY_FILTERS = ['Intelligence', 'Command', 'Robot', 'Weapon', 'Logistics', 'Prediction', 'Mapping'];

const GRAPH_OPTIONS: Options = {
  nodes: {
    shape: 'dot',
    font: { color: '#d9e9f9', face: 'Inter, system-ui, sans-serif', size: 11, strokeWidth: 3, strokeColor: '#0a0e17' },
    borderWidth: 2.5,
    shadow: { enabled: true, color: 'rgba(0,0,0,0.5)', size: 14, x: 0, y: 4 },
  },
  edges: {
    color: { color: 'rgba(125,162,186,0.30)', highlight: '#00d4ff', hover: 'rgba(0,212,255,0.5)' },
    font: { color: 'rgba(147,169,191,0.55)', size: 8, face: 'Inter, system-ui, sans-serif', strokeWidth: 0, align: 'middle' },
    width: 1,
    smooth: { enabled: true, type: 'continuous', roundness: 0.12 },
    arrows: { to: { enabled: true, scaleFactor: 0.4, type: 'arrow' } },
  },
  physics: {
    solver: 'forceAtlas2Based',
    forceAtlas2Based: { gravitationalConstant: -80, centralGravity: 0.005, springLength: 150, springConstant: 0.025, damping: 0.4, avoidOverlap: 0.3 },
    stabilization: { iterations: 200, fit: true },
  },
  interaction: { hover: true, tooltipDelay: 80, zoomView: true, dragView: true, dragNodes: true },
  layout: { improvedLayout: true },
};

export function AiwarExplorer({ nodes, edges, weapons, onBack }: AiwarExplorerProps) {
  const [view, setView] = useState<ViewMode>('table');
  const [filter, setFilter] = useState<FilterKey>('all');
  const [milFilter, setMilFilter] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [search, setSearch] = useState('');
  const [sortKey, setSortKey] = useState('label');
  const [sortDir, setSortDir] = useState<1 | -1>(1);
  const [enrichedNodes, setEnrichedNodes] = useState<GraphNode[]>([]);
  const [enrichedEdges, setEnrichedEdges] = useState<GraphEdge[]>([]);
  const graphRef = useRef<HTMLDivElement>(null);
  const networkRef = useRef<Network | null>(null);

  // Combine official + enrichment nodes
  const allNodes = useMemo(() => {
    const seen = new Set(nodes.map((n) => n.id));
    return [...nodes, ...enrichedNodes.filter((n) => !seen.has(n.id))];
  }, [nodes, enrichedNodes]);

  const allEdges = useMemo(() => [...edges, ...enrichedEdges], [edges, enrichedEdges]);

  // Filter
  const filtered = useMemo(() => {
    let result = allNodes;
    if (filter !== 'all') result = result.filter((n) => n.type === filter);
    if (milFilter) result = result.filter((n) => String(n.properties.militaryUse || '').includes(milFilter));
    if (search) {
      const q = search.toLowerCase();
      result = result.filter((n) =>
        [n.id, n.label, n.type, ...Object.values(n.properties).map(String)].join(' ').toLowerCase().includes(q),
      );
    }
    return result;
  }, [allNodes, filter, milFilter, search]);

  const filteredIds = useMemo(() => new Set(filtered.map((n) => n.id)), [filtered]);
  const filteredEdges = useMemo(
    () => allEdges.filter((e) => filteredIds.has(e.source) && filteredIds.has(e.target)),
    [allEdges, filteredIds],
  );

  const selectedNode = allNodes.find((n) => n.id === selectedId) || null;

  // Filter weapons from CSV
  const filteredWeapons = useMemo(() => {
    if (weapons.length === 0) return [];
    if (!search) return weapons;
    const q = search.toLowerCase();
    return weapons.filter((w) =>
      [w.weapon, w.developed, w.usedBy, w.militaryPurpose, w.source, w.sourceType]
        .join(' ').toLowerCase().includes(q),
    );
  }, [weapons, search]);

  // Nation counts
  const nationCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const e of edges) {
      if (e.label === 'developed' || e.label === 'employed') {
        const stakeholder = nodes.find((n) => n.id === e.source);
        if (stakeholder?.type === 'Stakeholder' && stakeholder.properties.subtype === 'Nation') {
          counts[stakeholder.label] = (counts[stakeholder.label] || 0) + 1;
        }
      }
    }
    return Object.entries(counts).sort((a, b) => b[1] - a[1]).slice(0, 8);
  }, [nodes, edges]);

  // Systems count
  const systemCount = nodes.filter((n) => n.type === 'System').length;

  // Sort
  const sorted = useMemo(() => {
    return [...filtered].sort((a, b) => {
      let va: string | number, vb: string | number;
      if (sortKey === 'label') { va = a.label; vb = b.label; }
      else if (sortKey === 'type') { va = a.type; vb = b.type; }
      else if (sortKey === 'year') { va = Number(a.properties.year) || 0; vb = Number(b.properties.year) || 0; }
      else { va = String(a.properties[sortKey] || ''); vb = String(b.properties[sortKey] || ''); }
      if (typeof va === 'number' && typeof vb === 'number') return (va - vb) * sortDir;
      return String(va).localeCompare(String(vb)) * sortDir;
    });
  }, [filtered, sortKey, sortDir]);

  const handleSort = (key: string) => {
    if (sortKey === key) setSortDir((d) => (d === 1 ? -1 : 1) as 1 | -1);
    else { setSortKey(key); setSortDir(1); }
  };

  // Graph rendering
  useEffect(() => {
    if (view !== 'graph' || !graphRef.current || filtered.length === 0) {
      if (networkRef.current) { networkRef.current.destroy(); networkRef.current = null; }
      return;
    }

    const isEnriched = new Set(enrichedNodes.map((n) => n.id));
    const visNodes = new DataSet(
      filtered.map((n) => {
        const typeColor = AIWAR_TYPE_COLORS[n.type] || '#00d4ff';
        const enriched = isEnriched.has(n.id);
        return {
          id: n.id,
          label: n.label,
          color: {
            background: enriched ? 'rgba(255, 171, 0, 0.15)' : 'rgba(10, 14, 23, 0.85)',
            border: enriched ? '#ffab00' : typeColor,
            highlight: { background: 'rgba(10, 14, 23, 0.95)', border: '#00d4ff' },
            hover: { background: 'rgba(10, 14, 23, 0.80)', border: typeColor },
          },
          size: n.type === 'System' ? 14 : n.type === 'Person' ? 10 : 12,
          title: `${n.label}\n${n.type}${n.properties.year ? ` (${n.properties.year})` : ''}${n.properties.militaryUse ? `\n${n.properties.militaryUse}` : ''}`,
        };
      }),
    );

    const visEdges = new DataSet(
      filteredEdges.map((e, i) => ({
        id: `e-${i}`,
        from: e.source,
        to: e.target,
        label: e.label,
      })),
    );

    const network = new Network(graphRef.current, { nodes: visNodes, edges: visEdges }, GRAPH_OPTIONS);
    network.on('click', (params) => {
      setSelectedId(params.nodes.length > 0 ? (params.nodes[0] as string) : null);
    });
    networkRef.current = network;

    return () => { network.destroy(); networkRef.current = null; };
  }, [view, filtered, filteredEdges, enrichedNodes]);

  // Sync selection in graph
  useEffect(() => {
    if (networkRef.current) {
      networkRef.current.selectNodes(selectedId ? [selectedId] : []);
    }
  }, [selectedId]);

  const handleFit = useCallback(() => { networkRef.current?.fit({ animation: true }); }, []);

  const handleEnrich = (result: ReasoningResult) => {
    const newNodes: GraphNode[] = result.enrichmentNodes.map((n) => ({
      id: n.id,
      label: n.label,
      type: n.type,
      properties: n.properties,
    }));
    const newEdges: GraphEdge[] = result.enrichmentEdges.map((e) => ({
      source: e.source,
      target: e.target,
      label: e.label,
    }));
    setEnrichedNodes((prev) => {
      const seen = new Set(prev.map((n) => n.id));
      return [...prev, ...newNodes.filter((n) => !seen.has(n.id))];
    });
    setEnrichedEdges((prev) => [...prev, ...newEdges]);
  };

  const columns = [
    { key: 'label', label: 'Name' },
    { key: 'type', label: 'Type' },
    { key: 'year', label: 'Year' },
    { key: 'militaryUse', label: 'Military Use' },
    { key: 'status', label: 'Status' },
  ];

  return (
    <div className="aiwar-explorer">
      {/* Top bar */}
      <div className="aiwar-topbar">
        <div className="aiwar-topbar-left">
          <button className="aiwar-back" onClick={onBack}>&larr; Back</button>
          <h2>Q2 &rsaquo; AIWAR &mdash; AI in Warfare Research</h2>
          <span className="badge">{systemCount} systems</span>
          {enrichedNodes.length > 0 && (
            <span className="badge warn">+{enrichedNodes.length} discovered</span>
          )}
        </div>
        <div className="aiwar-topbar-right">
          <button className={`pill ${view === 'table' ? 'active' : ''}`} onClick={() => setView('table')}>Table View</button>
          <button className={`pill ${view === 'graph' ? 'active' : ''}`} onClick={() => setView('graph')}>Graph View</button>
        </div>
      </div>

      <div className="aiwar-main">
        {/* Left sidebar */}
        <div className="aiwar-sidebar panel">
          <div className="panel-header">
            <div className="panel-title">
              <h2>Filters</h2>
            </div>
          </div>
          <div className="rail-body">
            <div className="rail-section">
              <div className="section-label">entity type</div>
              <div className="filter-pills">
                {FILTERS.map((f) => (
                  <button key={f.key} className={`pill ${filter === f.key ? 'active' : ''}`} onClick={() => setFilter(f.key)}>
                    {f.label} ({f.key === 'all' ? allNodes.length : allNodes.filter((n) => n.type === f.key).length})
                  </button>
                ))}
              </div>
            </div>

            <div className="rail-section">
              <div className="section-label">military use</div>
              <div className="filter-pills">
                <button className={`pill ${!milFilter ? 'active' : ''}`} onClick={() => setMilFilter(null)}>All</button>
                {MILITARY_FILTERS.map((m) => (
                  <button key={m} className={`pill ${milFilter === m ? 'active' : ''}`} onClick={() => setMilFilter(milFilter === m ? null : m)}>
                    {m}
                  </button>
                ))}
              </div>
            </div>

            <div className="rail-section">
              <div className="section-label">by nation</div>
              <div className="filter-pills">
                {nationCounts.map(([name, count]) => (
                  <button key={name} className="pill" onClick={() => setSearch(name)}>
                    {name} ({count})
                  </button>
                ))}
              </div>
            </div>
          </div>
        </div>

        {/* Center: table or graph */}
        <div className="aiwar-center">
          {view === 'table' ? (
            <section className="panel aiwar-table-panel">
              <div className="table-toolbar">
                <input
                  className="table-search"
                  placeholder="filter systems, nations, technology..."
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                />
                <span className="badge">
                  {weapons.length > 0 ? `${filteredWeapons.length} weapons` : `${filtered.length} rows`}
                </span>
              </div>
              <div className="table-scroll">
                {weapons.length > 0 ? (
                  /* Official CSV weapons table — the "innocent" view */
                  <table>
                    <thead>
                      <tr>
                        <th onClick={() => handleSort('weapon')}>
                          Weapon{sortKey === 'weapon' && <span style={{ marginLeft: 4, opacity: 0.7 }}>{sortDir === 1 ? '\u2191' : '\u2193'}</span>}
                        </th>
                        <th onClick={() => handleSort('developed')}>
                          Year{sortKey === 'developed' && <span style={{ marginLeft: 4, opacity: 0.7 }}>{sortDir === 1 ? '\u2191' : '\u2193'}</span>}
                        </th>
                        <th onClick={() => handleSort('usedBy')}>
                          Used By{sortKey === 'usedBy' && <span style={{ marginLeft: 4, opacity: 0.7 }}>{sortDir === 1 ? '\u2191' : '\u2193'}</span>}
                        </th>
                        <th>Military Purpose</th>
                        <th>Source</th>
                      </tr>
                    </thead>
                    <tbody>
                      {filteredWeapons.map((w, i) => {
                        // Find matching graph node for selection
                        const matchNode = nodes.find((n) =>
                          n.label.toLowerCase() === w.weapon.toLowerCase() ||
                          n.id.toLowerCase().replace(/\s/g, '') === w.weapon.toLowerCase().replace(/\s/g, ''),
                        );
                        return (
                          <tr
                            key={i}
                            className={matchNode && matchNode.id === selectedId ? 'active' : ''}
                            onClick={() => matchNode && setSelectedId(matchNode.id === selectedId ? null : matchNode.id)}
                          >
                            <td><strong>{w.weapon}</strong></td>
                            <td>{w.developed}</td>
                            <td>{w.usedBy}</td>
                            <td style={{ maxWidth: 300, whiteSpace: 'normal', lineHeight: 1.4 }}>
                              {w.militaryPurpose.length > 120 ? w.militaryPurpose.slice(0, 120) + '...' : w.militaryPurpose}
                            </td>
                            <td><span className="badge">{w.source}</span></td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                ) : (
                  /* Fallback: graph node table */
                  <table>
                    <thead>
                      <tr>
                        {columns.map((col) => (
                          <th key={col.key} onClick={() => handleSort(col.key)}>
                            {col.label}
                            {sortKey === col.key && <span style={{ marginLeft: 4, opacity: 0.7 }}>{sortDir === 1 ? '\u2191' : '\u2193'}</span>}
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {sorted.map((node) => (
                        <tr
                          key={node.id}
                          className={node.id === selectedId ? 'active' : ''}
                          onClick={() => setSelectedId(node.id === selectedId ? null : node.id)}
                        >
                          <td><strong>{node.label}</strong></td>
                          <td>
                            <span style={{ color: AIWAR_TYPE_COLORS[node.type] || '#00d4ff' }}>
                              {node.type}
                            </span>
                          </td>
                          <td>{String(node.properties.year || '')}</td>
                          <td>{String(node.properties.militaryUse || '')}</td>
                          <td>{String(node.properties.status || '')}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
              </div>
            </section>
          ) : (
            <section className="panel aiwar-graph-panel">
              <div className="graph-stage">
                {filtered.length === 0 ? (
                  <div className="graph-empty">
                    <div className="graph-empty-text">No nodes match current filters</div>
                  </div>
                ) : (
                  <div ref={graphRef} style={{ width: '100%', height: '100%' }} />
                )}
              </div>
              <div className="graph-footer">
                <div className="legend">
                  <span style={{ color: '#00d4ff' }}>system</span>
                  <span style={{ color: '#ff9800' }}>stakeholder</span>
                  <span style={{ color: '#4caf50' }}>civic</span>
                  <span style={{ color: '#e040fb' }}>person</span>
                  <span style={{ color: '#ffab00' }}>historical</span>
                  {enrichedNodes.length > 0 && (
                    <span style={{ color: '#ffab00' }}>discovered ({enrichedNodes.length})</span>
                  )}
                </div>
                <div className="mini-status">
                  <span className="badge">{filtered.length} nodes &middot; {filteredEdges.length} edges</span>
                  <button className="badge" onClick={handleFit} style={{ cursor: 'pointer' }}>fit view</button>
                </div>
              </div>
            </section>
          )}

          {/* Cypher console below */}
          <CypherConsole />
        </div>

        {/* Right: NARS panel */}
        <NarsPanel
          selectedNode={selectedNode}
          edges={allEdges}
          onEnrich={handleEnrich}
        />
      </div>

      {/* Footer hint */}
      <div className="aiwar-footer-hint">
        Select any weapon system and click <strong>Reason About This</strong> to discover
        connections the AI finds through NARS inference.
      </div>
    </div>
  );
}
