import { useState } from 'react';
import { TruthBadge } from '../TruthBadge';

interface ReviseResult {
  before: { f: number; c: number };
  after: { f: number; c: number };
  cascadeCount: number;
}

interface ReviseCellProps {
  onExecute: (edgeDesc: string, newF: number, newC: number, justification: string) => void;
  result: ReviseResult | null;
}

export function ReviseCell({ onExecute, result }: ReviseCellProps) {
  const [edgeDesc, setEdgeDesc] = useState('CNPC →[invested_in]→ Venezuela');
  const [newF, setNewF] = useState(0.30);
  const [newC, setNewC] = useState(0.70);
  const [justification, setJustification] = useState(
    'CNPC Venezuela investment was $4B in 2007 but operations suspended in 2019 due to sanctions.',
  );
  const [loading, setLoading] = useState(false);

  const handleApply = () => {
    setLoading(true);
    onExecute(edgeDesc, newF, newC, justification);
    setTimeout(() => setLoading(false), 600);
  };

  return (
    <div className="nb-cell nb-cell--revise">
      <div className="nb-cell-header">
        <span className="nb-cell-type nb-cell-type--revise">REVISE</span>
        <span className="nb-cell-desc">Human correction</span>
      </div>
      <div className="nb-cell-body">
        <div className="nb-cell-column">
          <div className="nb-cell-row">
            <label>Edge:</label>
            <input className="nb-input" value={edgeDesc} onChange={(e) => setEdgeDesc(e.target.value)} />
          </div>
          <div className="nb-cell-row">
            <label>New f:</label>
            <input className="nb-input nb-input--small" type="number" step={0.01} min={0} max={1}
              value={newF} onChange={(e) => setNewF(Number(e.target.value))} />
            <label>New c:</label>
            <input className="nb-input nb-input--small" type="number" step={0.01} min={0} max={1}
              value={newC} onChange={(e) => setNewC(Number(e.target.value))} />
          </div>
          <div className="nb-cell-row">
            <textarea
              className="nb-textarea"
              value={justification}
              onChange={(e) => setJustification(e.target.value)}
              placeholder="Justification for correction..."
              rows={3}
            />
          </div>
          <div className="nb-cell-row">
            <button className="nb-run-btn" onClick={handleApply} disabled={loading}>
              {loading ? 'Applying...' : '\u270F\uFE0F Apply'}
            </button>
          </div>
        </div>
        {result && (
          <div className="nb-cell-result">
            <div className="nb-revise-comparison">
              <div>Before: <TruthBadge f={result.before.f} c={result.before.c} gate="HOLD" compact /></div>
              <div>&rarr;</div>
              <div>After: <TruthBadge f={result.after.f} c={result.after.c} gate={result.after.c > 0.5 ? 'FLOW' : 'HOLD'} compact /></div>
            </div>
            <div className="nb-revise-cascade">
              Cascade: {result.cascadeCount} downstream projections updated
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
