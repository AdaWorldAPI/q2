import { useState } from 'react';
import { TruthBadge } from '../TruthBadge';

interface Scenario {
  label: string;
  truth: { f: number; c: number };
  events: string[];
  signals: string[];
}

interface ProjectCellProps {
  onExecute: (horizon: string, count: number) => void;
  result: { scenarios: Scenario[] } | null;
}

export function ProjectCell({ onExecute, result }: ProjectCellProps) {
  const [horizon, setHorizon] = useState('2 weeks');
  const [count, setCount] = useState(3);
  const [loading, setLoading] = useState(false);

  const handleProject = () => {
    setLoading(true);
    onExecute(horizon, count);
    setTimeout(() => setLoading(false), 1500);
  };

  const SCENARIO_COLORS = ['#35d07f', '#ffb547', '#93a9bf'];

  return (
    <div className="nb-cell nb-cell--project">
      <div className="nb-cell-header">
        <span className="nb-cell-type nb-cell-type--project">PROJECT</span>
        <span className="nb-cell-desc">Temporal projection</span>
      </div>
      <div className="nb-cell-body">
        <div className="nb-cell-row">
          <label>Horizon:</label>
          <select value={horizon} onChange={(e) => setHorizon(e.target.value)} className="nb-select">
            <option>1 week</option>
            <option>2 weeks</option>
            <option>1 month</option>
            <option>3 months</option>
          </select>
          <label>Scenarios:</label>
          <select value={count} onChange={(e) => setCount(Number(e.target.value))} className="nb-select">
            <option value={2}>2</option>
            <option value={3}>3</option>
            <option value={5}>5</option>
          </select>
          <button className="nb-run-btn" onClick={handleProject} disabled={loading}>
            {loading ? 'Projecting...' : '\uD83D\uDCCA Project'}
          </button>
        </div>
        {result && (
          <div className="nb-cell-result">
            {result.scenarios.map((s, i) => (
              <div key={i} className="nb-scenario" style={{ borderLeftColor: SCENARIO_COLORS[i] || '#93a9bf' }}>
                <div className="nb-scenario-header">
                  <strong>Scenario {String.fromCharCode(65 + i)}: {s.label}</strong>
                  <TruthBadge f={s.truth.f} c={s.truth.c} gate={s.truth.c > 0.5 ? 'FLOW' : 'HOLD'} compact />
                </div>
                <ul className="nb-scenario-events">
                  {s.events.map((e, j) => <li key={j}>{e}</li>)}
                </ul>
                <div className="nb-scenario-signals">
                  Signals: {s.signals.join(', ')}
                </div>
              </div>
            ))}
            <div className="nb-disclaimer">
              These are NARS projections, not predictions. Truth values show confidence in structural patterns.
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
