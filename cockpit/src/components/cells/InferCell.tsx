import { useState } from 'react';
import { TruthBadge } from '../TruthBadge';

interface Inference {
  premise: string;
  conclusion: string;
  truth: { f: number; c: number };
  gate: 'FLOW' | 'HOLD' | 'BLOCK';
}

interface InferCellProps {
  onExecute: (mode: string, depth: number) => void;
  result: { flow: Inference[]; hold: Inference[]; block: Inference[] } | null;
  onAddFlow: () => void;
}

export function InferCell({ onExecute, result, onAddFlow }: InferCellProps) {
  const [mode, setMode] = useState('Deduction');
  const [depth, setDepth] = useState(3);
  const [loading, setLoading] = useState(false);

  const handleRun = () => {
    setLoading(true);
    onExecute(mode, depth);
    setTimeout(() => setLoading(false), 1200);
  };

  return (
    <div className="nb-cell nb-cell--infer">
      <div className="nb-cell-header">
        <span className="nb-cell-type nb-cell-type--infer">INFER</span>
        <span className="nb-cell-desc">NARS inference on subgraph</span>
      </div>
      <div className="nb-cell-body">
        <div className="nb-cell-row">
          <label>Mode:</label>
          <select value={mode} onChange={(e) => setMode(e.target.value)} className="nb-select">
            <option>Deduction</option>
            <option>Induction</option>
            <option>Abduction</option>
            <option>All</option>
          </select>
          <label>Depth:</label>
          <select value={depth} onChange={(e) => setDepth(Number(e.target.value))} className="nb-select">
            <option value={1}>1 hop</option>
            <option value={2}>2 hops</option>
            <option value={3}>3 hops</option>
            <option value={5}>5 hops</option>
          </select>
          <button className="nb-run-btn" onClick={handleRun} disabled={loading}>
            {loading ? 'Inferring...' : '\u26A1 Infer'}
          </button>
        </div>
        {result && (
          <div className="nb-cell-result">
            <strong>{result.flow.length + result.hold.length + result.block.length} new inferences:</strong>

            {result.flow.length > 0 && (
              <div className="nb-infer-group">
                <div className="nb-infer-label" style={{ color: '#35d07f' }}>FLOW ({result.flow.length})</div>
                {result.flow.slice(0, 4).map((inf, i) => (
                  <div key={i} className="nb-infer-item">
                    <span>{inf.conclusion}</span>
                    <TruthBadge f={inf.truth.f} c={inf.truth.c} gate="FLOW" compact />
                  </div>
                ))}
              </div>
            )}

            {result.hold.length > 0 && (
              <div className="nb-infer-group">
                <div className="nb-infer-label" style={{ color: '#ffb547' }}>HOLD ({result.hold.length})</div>
                {result.hold.slice(0, 3).map((inf, i) => (
                  <div key={i} className="nb-infer-item">
                    <span>{inf.conclusion}</span>
                    <TruthBadge f={inf.truth.f} c={inf.truth.c} gate="HOLD" compact />
                  </div>
                ))}
              </div>
            )}

            {result.block.length > 0 && (
              <div className="nb-infer-group">
                <div className="nb-infer-label" style={{ color: '#ff637d' }}>BLOCK ({result.block.length}) — too uncertain to display</div>
              </div>
            )}

            <div className="nb-cell-actions">
              <button className="nb-action-btn" onClick={onAddFlow}>Add FLOW to graph</button>
              <button className="nb-action-btn">Investigate HOLD items</button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
