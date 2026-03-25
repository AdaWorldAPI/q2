import { useState, useCallback } from 'react';
import { TruthBadge } from './TruthBadge';

interface AnalysisResult {
  bucket: string;
  label: string;
  description: string;
  thinking_style: string | null;
  elapsed_us: number;
  queries: {
    cypher: string;
    intent: string;
    status: string;
    row_count: number;
    error: string | null;
    edges_found: TruthEdge[];
  }[];
  causality_chains: {
    name: string;
    confidence: number;
    inference_type: string;
    narrative: string;
    edges: TruthEdge[];
  }[];
  summary: {
    total_nodes_involved: number;
    total_edges_found: number;
    total_inferred: number;
    avg_confidence: number;
    key_findings: string[];
    blind_spots: string[];
  };
}

interface TruthEdge {
  source: string;
  target: string;
  rel_type: string;
  truth: { frequency: number; confidence: number };
  inferred: boolean;
  inference_type: string | null;
}

const BUCKETS = [
  { id: 'economic_review', label: 'Economic Review', color: '#ffb547', icon: '\uD83D\uDCB0' },
  { id: 'civil_engineering', label: 'Civil Engineering', color: '#4caf50', icon: '\uD83C\uDFD7\uFE0F' },
  { id: 'political_dynamics', label: 'Political Dynamics', color: '#e040fb', icon: '\uD83C\uDFDB\uFE0F' },
  { id: 'ai_development_impact', label: 'AI Development', color: '#00d4ff', icon: '\uD83E\uDD16' },
  { id: 'kill_chain_analysis', label: 'Kill Chain', color: '#ff637d', icon: '\u26A0\uFE0F' },
  { id: 'surveillance_ecosystem', label: 'Surveillance', color: '#ff9800', icon: '\uD83D\uDC41\uFE0F' },
];

export function AnalystPanel() {
  const [selectedBucket, setSelectedBucket] = useState<string | null>(null);
  const [result, setResult] = useState<AnalysisResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const runAnalysis = useCallback(async (bucketId: string) => {
    setSelectedBucket(bucketId);
    setLoading(true);
    setError(null);
    setResult(null);

    try {
      const res = await fetch(`/api/analyst/analyze/${bucketId}`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = await res.json();
      if (data.error) throw new Error(data.error);
      setResult(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Analysis failed');
    } finally {
      setLoading(false);
    }
  }, []);

  const selectedInfo = BUCKETS.find(b => b.id === selectedBucket);

  return (
    <div className="analyst-panel">
      {/* Bucket selector */}
      <div className="analyst-buckets">
        {BUCKETS.map((bucket) => (
          <button
            key={bucket.id}
            className={`analyst-bucket-btn ${selectedBucket === bucket.id ? 'active' : ''}`}
            style={{ borderColor: selectedBucket === bucket.id ? bucket.color : undefined }}
            onClick={() => runAnalysis(bucket.id)}
            disabled={loading}
          >
            <span className="analyst-bucket-icon">{bucket.icon}</span>
            <span className="analyst-bucket-label">{bucket.label}</span>
          </button>
        ))}
      </div>

      {/* Loading */}
      {loading && (
        <div className="analyst-loading">
          <div className="nars-spinner" />
          <span>Running {selectedInfo?.label} analysis...</span>
          <span style={{ fontSize: 11, color: 'var(--muted)' }}>
            Cypher queries &rarr; NARS inference &rarr; causality chains
          </span>
        </div>
      )}

      {/* Error */}
      {error && (
        <div className="analyst-error">
          <span style={{ color: 'var(--danger)' }}>Analysis error: {error}</span>
          <div style={{ fontSize: 11, color: 'var(--muted)', marginTop: 4 }}>
            The analyst runs Cypher queries through lance-graph. In demo mode (Node server),
            queries return stub data. Deploy with Dockerfile (Rust binary) for live analysis.
          </div>
        </div>
      )}

      {/* Results */}
      {result && (
        <div className="analyst-result">
          {/* Header */}
          <div className="analyst-result-header">
            <div>
              <h3 style={{ color: selectedInfo?.color }}>{result.label}</h3>
              <span className="analyst-desc">{result.description}</span>
            </div>
            <div className="analyst-meta">
              <span className="badge">{(result.elapsed_us / 1000).toFixed(0)}ms</span>
              {result.thinking_style && (
                <span className="badge" style={{ color: selectedInfo?.color }}>
                  {result.thinking_style}
                </span>
              )}
            </div>
          </div>

          {/* Summary */}
          <div className="analyst-summary">
            <div className="analyst-summary-stats">
              <div className="analyst-stat">
                <span className="analyst-stat-value">{result.summary.total_nodes_involved}</span>
                <span className="analyst-stat-label">nodes</span>
              </div>
              <div className="analyst-stat">
                <span className="analyst-stat-value">{result.summary.total_edges_found}</span>
                <span className="analyst-stat-label">edges</span>
              </div>
              <div className="analyst-stat">
                <span className="analyst-stat-value" style={{ color: '#ffab00' }}>
                  {result.summary.total_inferred}
                </span>
                <span className="analyst-stat-label">inferred</span>
              </div>
              <div className="analyst-stat">
                <span className="analyst-stat-value">
                  {(result.summary.avg_confidence * 100).toFixed(0)}%
                </span>
                <span className="analyst-stat-label">confidence</span>
              </div>
            </div>

            {/* Key findings */}
            <div className="analyst-findings">
              <div className="section-label" style={{ color: '#35d07f' }}>key findings</div>
              {result.summary.key_findings.map((f, i) => (
                <div key={i} className="analyst-finding">{f}</div>
              ))}
            </div>

            {/* Blind spots */}
            {result.summary.blind_spots.length > 0 && (
              <div className="analyst-findings">
                <div className="section-label" style={{ color: '#ff637d' }}>blind spots</div>
                {result.summary.blind_spots.map((f, i) => (
                  <div key={i} className="analyst-finding analyst-blind">{f}</div>
                ))}
              </div>
            )}
          </div>

          {/* Causality chains */}
          {result.causality_chains.length > 0 && (
            <div className="analyst-chains">
              <div className="section-label">causality chains</div>
              {result.causality_chains.map((chain, i) => (
                <div key={i} className="analyst-chain">
                  <div className="analyst-chain-header">
                    <strong>{chain.name}</strong>
                    <TruthBadge
                      f={chain.confidence}
                      c={chain.confidence}
                      gate={chain.confidence > 0.5 ? 'FLOW' : chain.confidence > 0.2 ? 'HOLD' : 'BLOCK'}
                    />
                  </div>
                  <div className="analyst-chain-narrative">{chain.narrative}</div>
                  <div className="analyst-chain-edges">
                    {chain.edges.map((e, j) => (
                      <span key={j} className="analyst-edge-tag">
                        {e.source} &rarr; {e.target}
                        {e.inferred && <span className="analyst-inferred-badge">inferred</span>}
                      </span>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          )}

          {/* Queries executed */}
          <div className="analyst-queries">
            <div className="section-label">queries executed ({result.queries.length})</div>
            {result.queries.map((q, i) => (
              <div key={i} className={`analyst-query ${q.status === 'error' ? 'analyst-query--error' : ''}`}>
                <div className="analyst-query-intent">{q.intent}</div>
                <pre className="analyst-query-cypher"><code>{q.cypher}</code></pre>
                <div className="analyst-query-meta">
                  <span className={`badge ${q.status === 'success' ? 'good' : q.status === 'error' ? 'hot' : ''}`}>
                    {q.status}
                  </span>
                  <span className="badge">{q.row_count} rows</span>
                  <span className="badge">{q.edges_found.length} edges</span>
                  {q.error && <span style={{ color: 'var(--danger)', fontSize: 11 }}>{q.error}</span>}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Empty state */}
      {!selectedBucket && !loading && (
        <div className="analyst-empty">
          Select an analysis bucket to create thinking through the aiwar graph.
          Each bucket runs Cypher queries, applies NARS inference, and builds causality chains.
        </div>
      )}
    </div>
  );
}
