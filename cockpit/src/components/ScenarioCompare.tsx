import { TruthBadge } from './TruthBadge';

interface Scenario {
  label: string;
  truth: { f: number; c: number };
  events: string[];
  signals: string[];
}

interface ScenarioCompareProps {
  scenarios: Scenario[];
}

const COLORS = ['#35d07f', '#ffb547', '#93a9bf', '#00d4ff', '#e040fb'];

export function ScenarioCompare({ scenarios }: ScenarioCompareProps) {
  if (scenarios.length === 0) {
    return (
      <div className="scenario-compare-empty">
        Run a PROJECT cell to generate scenarios for comparison.
      </div>
    );
  }

  return (
    <div className="scenario-compare">
      <div className="section-label">scenario comparison</div>
      <div className="scenario-grid" style={{ gridTemplateColumns: `repeat(${scenarios.length}, 1fr)` }}>
        {scenarios.map((s, i) => (
          <div key={i} className="scenario-col" style={{ borderTopColor: COLORS[i] }}>
            <div className="scenario-col-header">
              <strong>{String.fromCharCode(65 + i)}: {s.label}</strong>
              <TruthBadge f={s.truth.f} c={s.truth.c} gate={s.truth.c > 0.5 ? 'FLOW' : 'HOLD'} compact />
            </div>
            <ul>
              {s.events.map((e, j) => <li key={j}>{e}</li>)}
            </ul>
            <div className="scenario-col-signals">
              {s.signals.join(', ')}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
