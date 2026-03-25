// NARS reasoning panel — shows inference chains, truth values, gates
// Surfaces enrichment data as "AI-discovered connections"

import { useState } from 'react';
import { TruthBadge } from './TruthBadge';

interface Inference {
  source: string;
  target: string;
  rel_type: string;
  truth: { frequency: number; confidence: number; expectation: number };
  inference_type: string;
  via: string[];
}

interface NarsPanelProps {
  selectedNode: string | null;
  onRunInference: (nodeId: string, depth: number) => Promise<Inference[]>;
  onSearchChina?: (query: string) => void;
}

function classifyGate(c: number): 'FLOW' | 'HOLD' | 'BLOCK' {
  if (c >= 0.6) return 'FLOW';
  if (c >= 0.35) return 'HOLD';
  return 'BLOCK';
}

export function NarsPanel({ selectedNode, onRunInference, onSearchChina }: NarsPanelProps) {
  const [inferences, setInferences] = useState<Inference[]>([]);
  const [loading, setLoading] = useState(false);
  const [depth, setDepth] = useState(3);

  const handleReason = async () => {
    if (!selectedNode) return;
    setLoading(true);
    try {
      const results = await onRunInference(selectedNode, depth);
      setInferences(results);
    } finally {
      setLoading(false);
    }
  };

  const flow = inferences.filter((i) => classifyGate(i.truth.confidence) === 'FLOW');
  const hold = inferences.filter((i) => classifyGate(i.truth.confidence) === 'HOLD');
  const block = inferences.filter((i) => classifyGate(i.truth.confidence) === 'BLOCK');

  return (
    <div className="nars-panel">
      <div className="nars-header">
        <h3>NARS Inference</h3>
        {selectedNode && (
          <div className="nars-controls">
            <select
              value={depth}
              onChange={(e) => setDepth(Number(e.target.value))}
              className="nars-select"
            >
              <option value={2}>2 hops</option>
              <option value={3}>3 hops</option>
              <option value={4}>4 hops</option>
            </select>
            <button className="button nars-reason-btn" onClick={handleReason} disabled={loading}>
              {loading ? 'reasoning...' : 'Reason'}
            </button>
          </div>
        )}
      </div>

      {!selectedNode && (
        <div className="nars-empty">Select a node and click Reason to discover connections</div>
      )}

      {inferences.length > 0 && (
        <div className="nars-results">
          <div className="nars-summary">
            <span className="badge good">{flow.length} FLOW</span>
            <span className="badge warn">{hold.length} HOLD</span>
            <span className="badge hot">{block.length} BLOCK</span>
            <span className="badge">{inferences.length} total</span>
          </div>

          {flow.length > 0 && (
            <div className="nars-group">
              <div className="section-label">AI Discovered Connections</div>
              {flow.map((inf, i) => (
                <div key={i} className="nars-inference-card">
                  <div className="nars-chain">
                    <span className="nars-node">{inf.source}</span>
                    <span className="nars-arrow">&rarr;[{inf.rel_type}]&rarr;</span>
                    <span className="nars-node">{inf.target}</span>
                  </div>
                  {inf.via.length > 0 && (
                    <div className="nars-via">via: {inf.via.join(' → ')}</div>
                  )}
                  <TruthBadge
                    f={inf.truth.frequency}
                    c={inf.truth.confidence}
                    gate={classifyGate(inf.truth.confidence)}
                  />
                </div>
              ))}
            </div>
          )}

          {hold.length > 0 && (
            <div className="nars-group">
              <div className="section-label">Needs Verification</div>
              {hold.map((inf, i) => (
                <div key={i} className="nars-inference-card nars-hold">
                  <div className="nars-chain">
                    <span className="nars-node">{inf.source}</span>
                    <span className="nars-arrow">&rarr;[{inf.rel_type}]&rarr;</span>
                    <span className="nars-node">{inf.target}</span>
                  </div>
                  <TruthBadge
                    f={inf.truth.frequency}
                    c={inf.truth.confidence}
                    gate="HOLD"
                  />
                </div>
              ))}
            </div>
          )}

          {onSearchChina && (
            <div className="nars-actions">
              <button
                className="button"
                onClick={() => onSearchChina('SenseTime military contracts China AI weapons')}
              >
                China Links
              </button>
              <button
                className="button"
                onClick={() => onSearchChina(selectedNode + ' funding network investors')}
              >
                Deeper
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
