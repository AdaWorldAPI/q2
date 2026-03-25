import { useState } from 'react';
import type { GraphNode, GraphEdge } from '../store';
import { TruthBadge } from './TruthBadge';
import { ENRICHMENT_INDEX, getDefaultReasoning, type ReasoningResult, type EnrichmentEdge } from '../data/aiwar-seed';

interface NarsPanelProps {
  selectedNode: GraphNode | null;
  edges: GraphEdge[];
  onEnrich: (result: ReasoningResult) => void;
}

export function NarsPanel({ selectedNode, edges, onEnrich }: NarsPanelProps) {
  const [reasoning, setReasoning] = useState<ReasoningResult | null>(null);
  const [running, setRunning] = useState(false);
  const [showChina, setShowChina] = useState(false);

  if (!selectedNode) {
    return (
      <section className="panel nars-panel">
        <div className="panel-header">
          <div className="panel-title">
            <h2>NARS Inference</h2>
            <span>select a system to reason</span>
          </div>
        </div>
        <div className="nars-empty">
          <div style={{ fontSize: '12px', color: 'var(--muted)', textAlign: 'center', padding: '40px 20px' }}>
            Select a weapon system and click<br /><strong>Reason About This</strong> to discover<br />connections through NARS inference.
          </div>
        </div>
      </section>
    );
  }

  const nodeEdges = edges.filter(
    (e) => e.source === selectedNode.id || e.target === selectedNode.id,
  );

  const handleReason = () => {
    setRunning(true);
    setShowChina(false);
    // Simulate inference delay
    setTimeout(() => {
      const result = ENRICHMENT_INDEX[selectedNode.id] || getDefaultReasoning(selectedNode.id, selectedNode.label);
      setReasoning(result);
      setRunning(false);
      onEnrich(result);
    }, 1200 + Math.random() * 800);
  };

  const handleChinaLinks = () => {
    setShowChina(true);
  };

  return (
    <section className="panel nars-panel">
      <div className="panel-header">
        <div className="panel-title">
          <h2>NARS Inference</h2>
          <span>{selectedNode.label}</span>
        </div>
        {reasoning && (
          <span className="badge good">
            {reasoning.discoveredConnections} discovered
          </span>
        )}
      </div>
      <div className="nars-body">
        {/* Official data */}
        <div className="nars-section">
          <div className="section-label">official data</div>
          <div className="nars-official">
            {nodeEdges.slice(0, 5).map((e, i) => (
              <div key={i} className="nars-edge-row">
                <span className="nars-edge-label">
                  {e.source === selectedNode.id ? selectedNode.label : e.source}
                  <span className="nars-arrow"> &rarr;[{e.label}]&rarr; </span>
                  {e.target === selectedNode.id ? selectedNode.label : e.target}
                </span>
              </div>
            ))}
            {nodeEdges.length === 0 && (
              <div style={{ color: 'var(--muted)', fontSize: '12px' }}>No direct edges in official data</div>
            )}
          </div>
          <div className="nars-props">
            {Object.entries(selectedNode.properties).map(([k, v]) => (
              v && String(v) !== 'NaN' && String(v) !== '' ? (
                <div key={k} className="prop-row">
                  <div className="k">{k}</div>
                  <div>{String(v)}</div>
                </div>
              ) : null
            ))}
          </div>
        </div>

        {/* Reason button */}
        {!reasoning && (
          <button
            className="nars-reason-btn"
            onClick={handleReason}
            disabled={running}
          >
            {running ? 'Reasoning...' : '\uD83E\uDDE0 Reason About This'}
          </button>
        )}

        {running && (
          <div className="nars-running">
            <div className="nars-spinner" />
            <span>NARS inference in progress...</span>
            <span className="nars-running-sub">Deduction &middot; Induction &middot; Abduction</span>
          </div>
        )}

        {/* Discovered connections */}
        {reasoning && !running && (
          <>
            <div className="nars-section">
              <div className="section-label">
                AI discovered connections &mdash; {reasoning.discoveredConnections} inferences
              </div>
              <div className="nars-discoveries">
                {reasoning.enrichmentEdges.map((e: EnrichmentEdge, i: number) => (
                  <div key={i} className="nars-discovery-card">
                    <div className="nars-discovery-edge">
                      {e.source} &rarr;[{e.label}]&rarr; {e.target}
                    </div>
                    <div className="nars-discovery-detail">{e.detail}</div>
                    <div className="nars-discovery-meta">
                      <TruthBadge f={e.truthValue.f} c={e.truthValue.c} gate={e.gate} />
                      <span className="nars-inference-type">{e.inference}</span>
                    </div>
                  </div>
                ))}
              </div>
            </div>

            {/* Patterns */}
            <div className="nars-section">
              <div className="section-label">structural patterns</div>
              <div className="nars-patterns">
                {reasoning.patterns.map((p, i) => (
                  <div key={i} className="nars-pattern">{p}</div>
                ))}
              </div>
            </div>

            {/* Confidence bar */}
            <div className="nars-confidence">
              <div className="section-label">confidence</div>
              <div className="nars-confidence-bar">
                <div
                  className="nars-confidence-fill"
                  style={{ width: `${reasoning.confidence}%` }}
                />
              </div>
              <span className="nars-confidence-value">{reasoning.confidence.toFixed(1)}%</span>
            </div>

            {/* Action buttons */}
            <div className="nars-actions">
              <button className="nars-action-btn" onClick={handleReason}>Deeper</button>
              <button className="nars-action-btn" onClick={handleChinaLinks}>China Links</button>
              <button className="nars-action-btn">Full Chain</button>
            </div>

            {/* China links panel */}
            {showChina && (
              <div className="nars-section nars-china">
                <div className="section-label">cross-domain inference &mdash; China containment</div>
                <div className="nars-china-pattern">
                  Pattern: Every system in the dataset serves resource denial against China.
                </div>
                <div className="nars-china-chains">
                  <div>US &rarr; Lattice &rarr; autonomous kill chain</div>
                  <div>US &rarr; Replicator &rarr; drone swarms</div>
                  <div>Israel &rarr; Lavender &rarr; target ranking</div>
                  <div>NATO &rarr; DIANA &rarr; defense innovation</div>
                </div>
                <div className="nars-china-search">
                  <div className="section-label">live search</div>
                  <div className="nars-search-item">\uD83D\uDD0D "SenseTime military contracts"</div>
                  <div className="nars-search-item">\uD83D\uDD0D "Hikvision Xinjiang surveillance"</div>
                  <div className="nars-search-item">\uD83D\uDD0D "PLA autonomous weapons 2025"</div>
                  <div className="nars-search-status">
                    <TruthBadge f={0.55} c={0.35} gate="HOLD" />
                    <span style={{ fontSize: '11px', color: 'var(--muted)' }}>
                      High complexity, cross-domain inference
                    </span>
                  </div>
                </div>
              </div>
            )}
          </>
        )}
      </div>
    </section>
  );
}
