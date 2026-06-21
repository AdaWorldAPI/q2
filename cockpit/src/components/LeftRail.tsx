import { useMemo } from 'react';
import { useStore } from '../store';
import { executeQuery } from '../transport';

const FILTERS = [
  { key: 'all', label: 'all' },
  { key: 'Server', label: 'servers' },
  { key: 'Database', label: 'databases' },
  { key: 'Service', label: 'services' },
  { key: 'warning', label: 'at risk' },
];

const EXAMPLES = [
  {
    title: 'Infrastructure path',
    code: "g.V().hasLabel('server').outE().inV().path()",
    lang: 'gremlin',
  },
  {
    title: 'Cypher service fanout',
    code: "MATCH (s:Server)-[:CALLS]->(svc:Service) RETURN s, svc LIMIT 25",
    lang: 'cypher',
  },
  {
    title: 'R rollup',
    code: 'services %>% count(status) %>% arrange(desc(n))',
    lang: 'r',
  },
  {
    title: 'SPARQL dependency',
    code: 'PREFIX app: <http://q2.local/app#>\nSELECT ?service ?db WHERE { ?service app:dependsOn ?db . }',
    lang: 'sparql',
  },
];

export function LeftRail() {
  const nodes = useStore((s) => s.nodes);
  const edges = useStore((s) => s.edges);
  const filter = useStore((s) => s.filter);
  const setFilter = useStore((s) => s.setFilter);

  const metrics = useMemo(() => {
    const healthy = nodes.filter(
      (n) => n.properties.status === 'healthy',
    ).length;
    const alerts = nodes.length - healthy;
    return { nodes: nodes.length, edges: edges.length, healthy, alerts };
  }, [nodes, edges]);

  return (
    <section className="panel left-rail">
      <div className="panel-header">
        <div className="panel-title">
          <h2>Situation</h2>
          <span>overview &middot; filters &middot; quick jumps</span>
        </div>
        <div className="signal">healthy</div>
      </div>
      <div className="rail-body">
        {/* Metrics */}
        <div className="metrics-grid">
          <div className="metric-card">
            <div className="metric-label">Nodes</div>
            <div className="metric-value">{metrics.nodes}</div>
            <div className="metric-sub">live graph scope</div>
          </div>
          <div className="metric-card">
            <div className="metric-label">Edges</div>
            <div className="metric-value">{metrics.edges}</div>
            <div className="metric-sub">visible paths</div>
          </div>
          <div className="metric-card">
            <div className="metric-label">Healthy</div>
            <div className="metric-value">{metrics.healthy}</div>
            <div className="metric-sub">green signals</div>
          </div>
          <div className="metric-card">
            <div className="metric-label">Alerts</div>
            <div className="metric-value">{metrics.alerts}</div>
            <div className="metric-sub">amber + red</div>
          </div>
        </div>

        {/* Filters */}
        <div className="rail-section">
          <div className="section-label">filters</div>
          <div className="filter-pills">
            {FILTERS.map((f) => (
              <button
                key={f.key}
                className={`pill ${filter === f.key ? 'active' : ''}`}
                onClick={() => setFilter(f.key)}
              >
                {f.label}
              </button>
            ))}
          </div>
        </div>

        {/* Example queries */}
        <div className="rail-section">
          <div className="section-label">example cells</div>
          <div className="example-list">
            {EXAMPLES.map((ex) => (
              <button
                key={ex.title}
                className="example-card"
                onClick={() => executeQuery(ex.code, ex.lang)}
              >
                <strong>{ex.title}</strong>
                <code>{ex.code.length > 48 ? ex.code.slice(0, 48) + '\u2026' : ex.code}</code>
              </button>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}
