import { useState } from 'react';
import type { StackDiagnosis } from '../../../hooks/useNeuralDiagnosis';
import { WaveformMode } from './WaveformMode';
import { ConnectomeMode } from './ConnectomeMode';
import { DendriteMode } from './DendriteMode';

interface ResonanceVizProps {
  diagnosis: StackDiagnosis;
}

type VizMode = 'waveform' | 'connectome' | 'dendrite';

export function ResonanceViz({ diagnosis }: ResonanceVizProps) {
  const [mode, setMode] = useState<VizMode>('connectome');

  return (
    <div className="resonance-viz">
      <div className="resonance-viz-toolbar">
        <button className={`viz-mode-btn ${mode === 'waveform' ? 'active' : ''}`} onClick={() => setMode('waveform')}>
          Waveform
        </button>
        <button className={`viz-mode-btn ${mode === 'connectome' ? 'active' : ''}`} onClick={() => setMode('connectome')}>
          Connectome
        </button>
        <button className={`viz-mode-btn ${mode === 'dendrite' ? 'active' : ''}`} onClick={() => setMode('dendrite')}>
          Dendrite Forest
        </button>
      </div>
      <div className="resonance-viz-stage">
        {mode === 'waveform' && <WaveformMode diagnosis={diagnosis} />}
        {mode === 'connectome' && <ConnectomeMode diagnosis={diagnosis} />}
        {mode === 'dendrite' && <DendriteMode diagnosis={diagnosis} />}
      </div>
    </div>
  );
}
