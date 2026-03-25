import type { ModuleDiagnosis, FunctionMeta } from '../../hooks/useNeuralDiagnosis';

interface NeuronGridProps {
  module: ModuleDiagnosis;
  repoName: string;
}

const STATE_COLORS: Record<string, string> = {
  alive: '#35d07f',
  static: '#93a9bf',
  dead: '#ff637d',
  nan: '#ffb547',
  stub: '#666',
  wired_unused: '#4488ff',
};

const STATE_LABELS: Record<string, string> = {
  alive: 'ALIVE',
  static: 'STATIC',
  dead: 'DEAD',
  nan: 'NaN',
  stub: 'STUB',
  wired_unused: 'WIRED',
};

export function NeuronGrid({ module, repoName }: NeuronGridProps) {
  const deadFns = module.dead_functions || [];

  return (
    <div className="neuron-grid">
      <div className="neuron-grid-header">
        <h3>{repoName} :: {module.name}</h3>
        <div className="neuron-grid-stats">
          <span>{module.total} functions</span>
          <span style={{ color: '#35d07f' }}>{module.alive_or_static} alive</span>
          <span style={{ color: '#ff637d' }}>{module.dead} dead</span>
          <span style={{ color: '#666' }}>{module.stub} stub</span>
          <span style={{ color: '#ffb547' }}>{module.nan_risk} NaN</span>
          <span style={{ color: healthColor(module.health_pct) }}>
            {module.health_pct.toFixed(0)}% health
          </span>
        </div>
      </div>

      {/* Dead/stub/NaN function list */}
      {deadFns.length > 0 && (
        <div className="neuron-list">
          {deadFns.map((fn, i) => (
            <div key={i} className="neuron-item" style={{ borderLeftColor: STATE_COLORS[fn.state] || '#666' }}>
              <div className="neuron-item-header">
                <span className="neuron-state-badge" style={{
                  color: STATE_COLORS[fn.state],
                  borderColor: `${STATE_COLORS[fn.state]}40`,
                }}>
                  {STATE_LABELS[fn.state] || fn.state}
                </span>
                <span className="neuron-item-sig">{fn.signature}</span>
              </div>
              <div className="neuron-item-loc">
                {fn.file}:{fn.line}
              </div>
            </div>
          ))}
        </div>
      )}

      {deadFns.length === 0 && (
        <div className="neuron-grid-clean">
          All {module.total} functions are alive or static. No dead neurons.
        </div>
      )}
    </div>
  );
}

function healthColor(pct: number): string {
  if (pct >= 70) return '#35d07f';
  if (pct >= 30) return '#ffb547';
  return '#ff637d';
}
