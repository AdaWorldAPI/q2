import { useState } from 'react';
import { useNeuralDiagnosis } from './hooks/useNeuralDiagnosis';
import { BrainMriMode, type OrbitMode } from './components/debug/viz/BrainMriMode';
import { WaveformMode } from './components/debug/viz/WaveformMode';
import { useEffect } from 'react';

/**
 * /render — Creative visualization sandbox.
 *
 * Full-viewport Brain MRI with orbit selector + waveform overlay.
 * Demoscene-style pre-baked camera paths, real OSINT pipeline data.
 * Future: Unity WebGL embed, shader experiments, VR mode.
 */
export function RenderPage() {
  const { diagnosis, loading, load } = useNeuralDiagnosis();
  const [orbit, setOrbit] = useState<OrbitMode>('render');
  const [showWaveform, setShowWaveform] = useState(true);

  useEffect(() => { load(); }, [load]);

  if (loading || !diagnosis) {
    return (
      <div style={{ background: '#050810', color: '#4dd0e1', height: '100vh', display: 'flex', alignItems: 'center', justifyContent: 'center', fontFamily: 'monospace' }}>
        Loading brain...
      </div>
    );
  }

  return (
    <div style={{ width: '100vw', height: '100vh', background: '#050810', position: 'relative', overflow: 'hidden' }}>
      {/* Brain MRI — full viewport */}
      <div style={{ position: 'absolute', inset: 0 }}>
        <BrainMriMode diagnosis={diagnosis} orbitMode={orbit} />
      </div>

      {/* Waveform overlay — bottom strip */}
      {showWaveform && (
        <div style={{ position: 'absolute', bottom: 0, left: 0, right: 0, height: '120px', opacity: 0.7 }}>
          <WaveformMode diagnosis={diagnosis} />
        </div>
      )}

      {/* Controls — top right */}
      <div style={{
        position: 'absolute', top: 12, right: 12,
        display: 'flex', gap: 6, fontFamily: 'monospace', fontSize: 11,
      }}>
        {(['render', 'orbit', 'flight'] as OrbitMode[]).map((m) => (
          <button
            key={m}
            onClick={() => setOrbit(m)}
            style={{
              background: orbit === m ? '#4dd0e1' : 'rgba(10,14,23,0.8)',
              color: orbit === m ? '#050810' : '#93a9bf',
              border: '1px solid #333', borderRadius: 4,
              padding: '4px 10px', cursor: 'pointer',
              fontFamily: 'inherit',
            }}
          >
            {m}
          </button>
        ))}
        <button
          onClick={() => setShowWaveform(!showWaveform)}
          style={{
            background: showWaveform ? '#ffb547' : 'rgba(10,14,23,0.8)',
            color: showWaveform ? '#050810' : '#93a9bf',
            border: '1px solid #333', borderRadius: 4,
            padding: '4px 10px', cursor: 'pointer',
            fontFamily: 'inherit',
          }}
        >
          {showWaveform ? 'waves on' : 'waves off'}
        </button>
        <a href="/" style={{ color: '#4dd0e1', textDecoration: 'none', padding: '4px 10px' }}>
          ← cockpit
        </a>
      </div>

      {/* Title — top left */}
      <div style={{
        position: 'absolute', top: 12, left: 16,
        fontFamily: 'monospace', color: '#4dd0e140', fontSize: 10,
        letterSpacing: 2, textTransform: 'uppercase',
      }}>
        q2 render · brain mri · {orbit}
      </div>
    </div>
  );
}

/** /orbit — direct link to close orbit mode. */
export function OrbitPage() {
  const { diagnosis, loading, load } = useNeuralDiagnosis();
  useEffect(() => { load(); }, [load]);
  if (loading || !diagnosis) return <div style={{ background: '#050810', height: '100vh' }} />;
  return (
    <div style={{ width: '100vw', height: '100vh', position: 'relative' }}>
      <BrainMriMode diagnosis={diagnosis} orbitMode="orbit" />
      <a href="/render" style={{
        position: 'absolute', top: 12, right: 12,
        color: '#4dd0e1', fontFamily: 'monospace', fontSize: 11,
      }}>← render</a>
    </div>
  );
}

/** /flight — direct link to demoscene flythrough. */
export function FlightPage() {
  const { diagnosis, loading, load } = useNeuralDiagnosis();
  useEffect(() => { load(); }, [load]);
  if (loading || !diagnosis) return <div style={{ background: '#050810', height: '100vh' }} />;
  return (
    <div style={{ width: '100vw', height: '100vh', position: 'relative' }}>
      <BrainMriMode diagnosis={diagnosis} orbitMode="flight" />
      <a href="/render" style={{
        position: 'absolute', top: 12, right: 12,
        color: '#4dd0e1', fontFamily: 'monospace', fontSize: 11,
      }}>← render</a>
    </div>
  );
}
