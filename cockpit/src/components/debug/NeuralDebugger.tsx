import { useEffect, useState } from 'react';
import { useNeuralDiagnosis } from '../../hooks/useNeuralDiagnosis';
import { NeuralMap } from './NeuralMap';
import { NeuronGrid } from './NeuronGrid';
import { ResonanceViz } from './viz/ResonanceViz';
import { WaveformMode } from './viz/WaveformMode';
import { BrainMriMode } from './viz/BrainMriMode';

type VizMode = 'resonance' | 'waveform' | 'brain-mri';

interface NeuralDebuggerProps {
  onBack: () => void;
}

export function NeuralDebugger({ onBack }: NeuralDebuggerProps) {
  const { diagnosis, loading, error, load } = useNeuralDiagnosis();
  const [selectedModule, setSelectedModule] = useState<string | null>(null);
  const [showViz, setShowViz] = useState(true);
  const [vizMode, setVizMode] = useState<VizMode>('brain-mri');

  useEffect(() => { load(); }, [load]);

  if (loading) {
    return (
      <div className="neural-loading">
        <div className="nars-spinner" />
        <div>Scanning neural topology...</div>
      </div>
    );
  }

  if (error || !diagnosis) {
    return (
      <div className="neural-loading">
        <div style={{ color: 'var(--danger)' }}>Failed to load diagnosis: {error}</div>
        <button className="nars-action-btn" onClick={load}>Retry</button>
      </div>
    );
  }

  // Find selected module data
  const selectedModuleData = selectedModule ? (() => {
    const [repoName, modName] = selectedModule.split('::');
    const repo = diagnosis.repos.find((r) => r.name === repoName);
    const mod = repo?.modules.find((m) => m.name === modName);
    return mod ? { mod, repoName } : null;
  })() : null;

  return (
    <div className="neural-debugger">
      {/* Top bar */}
      <div className="neural-topbar">
        <div className="neural-topbar-left">
          <button className="aiwar-back" onClick={onBack}>&larr; Back</button>
          <h2>Neural Debugger</h2>
          <span className="badge good">{diagnosis.total_functions.toLocaleString()} functions</span>
          <span className="badge" style={{ color: diagnosis.health_pct >= 70 ? '#35d07f' : '#ffb547' }}>
            {diagnosis.health_pct.toFixed(1)}% health
          </span>
          {diagnosis.total_dead > 0 && <span className="badge hot">{diagnosis.total_dead} dead</span>}
          {diagnosis.total_nan_risk > 0 && <span className="badge warn">{diagnosis.total_nan_risk} NaN</span>}
          <span className="badge">{diagnosis.scan_duration_ms}ms scan</span>
        </div>
        <div className="neural-topbar-right">
          <button className={`pill ${showViz && vizMode === 'brain-mri' ? 'active' : ''}`}
            onClick={() => { setShowViz(true); setVizMode('brain-mri'); }}>
            Brain MRI
          </button>
          <button className={`pill ${showViz && vizMode === 'waveform' ? 'active' : ''}`}
            onClick={() => { setShowViz(true); setVizMode('waveform'); }}>
            Waveform
          </button>
          <button className={`pill ${showViz && vizMode === 'resonance' ? 'active' : ''}`}
            onClick={() => { setShowViz(true); setVizMode('resonance'); }}>
            Resonance
          </button>
          <button className={`pill ${!showViz ? 'active' : ''}`}
            onClick={() => setShowViz(false)}>
            Module Grid
          </button>
        </div>
      </div>

      {/* Main content */}
      <div className="neural-content">
        {showViz ? (
          <div className="neural-viz-layout">
            <div className="neural-viz-main">
              {vizMode === 'brain-mri' && <BrainMriMode diagnosis={diagnosis} />}
              {vizMode === 'waveform' && <WaveformMode diagnosis={diagnosis} />}
              {vizMode === 'resonance' && <ResonanceViz diagnosis={diagnosis} />}
            </div>
            <div className="neural-viz-sidebar">
              <NeuralMap
                diagnosis={diagnosis}
                onSelectModule={(repo, mod) => setSelectedModule(`${repo}::${mod}`)}
                selectedModule={selectedModule}
              />
            </div>
          </div>
        ) : (
          <div className="neural-grid-layout">
            <div className="neural-grid-sidebar">
              <NeuralMap
                diagnosis={diagnosis}
                onSelectModule={(repo, mod) => setSelectedModule(`${repo}::${mod}`)}
                selectedModule={selectedModule}
              />
            </div>
            <div className="neural-grid-main">
              {selectedModuleData ? (
                <NeuronGrid module={selectedModuleData.mod} repoName={selectedModuleData.repoName} />
              ) : (
                <div className="neural-grid-hint">
                  Select a module to view its neurons
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
