import { useRef, useEffect, useMemo, useCallback } from 'react';
import type { StackDiagnosis } from '../../../hooks/useNeuralDiagnosis';

interface WaveformModeProps {
  diagnosis: StackDiagnosis;
}

// Band definitions — each maps to a pipeline stage in the OSINT registry.
// The waveform shows REAL activity from /api/debug/osint counters.
const BANDS = [
  { name: 'Graph BFS',   color: '#1a3a5c', category: 'low',  key: 'graph_bfs' },
  { name: 'Spatial',     color: '#1a4a5c', category: 'low',  key: 'spatial_path' },
  { name: 'Episodic',    color: '#1a5a5c', category: 'low',  key: 'episodic_retrieve' },
  { name: 'Extraction',  color: '#00d4ff', category: 'mid',  key: 'extraction' },
  { name: 'xAI API',     color: '#4dd0e1', category: 'mid',  key: 'xai_api_call' },
  { name: 'NARS Deduct', color: '#ffb547', category: 'mid',  key: 'deduction' },
  { name: 'Contradict',  color: '#ff9800', category: 'mid',  key: 'contradiction' },
  { name: 'Revision',    color: '#e040fb', category: 'mid',  key: 'revision' },
  { name: 'Refinement',  color: '#ff4081', category: 'high', key: 'refinement' },
  { name: 'Planning',    color: '#ff637d', category: 'high', key: 'planning' },
  { name: 'Classify',    color: '#35d07f', category: 'high', key: 'classification' },
  { name: 'Ep. Store',   color: '#ffffff', category: 'high', key: 'episodic_store' },
];

// Exponential decay constant.
// At 60fps, decay of 0.92 means a spike drops to 50% in ~8 frames (~133ms).
// This gives a visible pulse that fades like neural afterglow.
const DECAY = 0.92;

// Spike sensitivity: how much one new call raises the signal.
// Tuned so a single call creates a visible blip, a burst saturates.
const SPIKE_GAIN = 0.15;

// Map module names from static diagnosis to band indices (fallback for cold start).
function moduleToBand(moduleName: string): number {
  const lower = moduleName.toLowerCase();
  if (lower.includes('adjacency') || lower.includes('csr')) return 0;
  if (lower.includes('lance') || lower.includes('arrow') || lower.includes('table')) return 1;
  if (lower.includes('storage') || lower.includes('io')) return 2;
  if (lower.includes('simd') || lower.includes('bitwise') || lower.includes('hpc')) return 3;
  if (lower.includes('cam') || lower.includes('pq') || lower.includes('clam')) return 4;
  if (lower.includes('nars') || lower.includes('truth') || lower.includes('inference')) return 5;
  if (lower.includes('semiring') || lower.includes('algebra')) return 6;
  if (lower.includes('strategy') || lower.includes('selector')) return 7;
  if (lower.includes('thinking') || lower.includes('style') || lower.includes('qualia')) return 8;
  if (lower.includes('collapse') || lower.includes('gate')) return 9;
  if (lower.includes('elevation') || lower.includes('budget')) return 10;
  if (lower.includes('mul') || lower.includes('compass') || lower.includes('homeostasis')) return 11;
  return Math.floor(Math.random() * 12);
}

export function WaveformMode({ diagnosis }: WaveformModeProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const frameRef = useRef(0);
  const animRef = useRef<number>(0);

  // Decaying signal levels per band — persists across frames.
  const signalRef = useRef<Float64Array>(new Float64Array(BANDS.length));
  // Previous counter values — to detect deltas (new activity since last poll).
  const prevCountersRef = useRef<Map<string, number>>(new Map());
  // History buffer for scrolling spectrogram.
  const historyRef = useRef<number[][]>([]);

  // Static baseline from diagnosis (used for cold start before first OSINT poll).
  const staticBaseline = useMemo(() => {
    const amps = new Array(12).fill(0);
    const counts = new Array(12).fill(0);
    for (const repo of diagnosis.repos) {
      for (const mod of repo.modules) {
        const band = moduleToBand(mod.name);
        amps[band] += mod.alive_or_static;
        counts[band] += mod.total;
      }
    }
    return amps.map((a, i) => counts[i] > 0 ? a / counts[i] : 0);
  }, [diagnosis]);

  // Poll the OSINT pipeline counters. Returns delta calls per stage since last poll.
  const pollCounters = useCallback(async (): Promise<Map<string, number>> => {
    const deltas = new Map<string, number>();
    try {
      const resp = await fetch('/api/debug/osint');
      if (!resp.ok) return deltas;
      const data = await resp.json();
      const stages: [string, { calls: number }][] = data?.pipeline?.stages || [];
      for (const [name, snap] of stages) {
        const prev = prevCountersRef.current.get(name) || 0;
        const delta = Math.max(0, (snap?.calls || 0) - prev);
        deltas.set(name, delta);
        prevCountersRef.current.set(name, snap?.calls || 0);
      }
    } catch {
      // Network error — silent, waveform just flatlines.
    }
    return deltas;
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    let rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);
    let w = rect.width;
    let h = rect.height;

    const timeSteps = 200;
    const bandHeight = h / BANDS.length;
    const cellW = w / timeSteps;

    // Initialize history from static baseline.
    if (historyRef.current.length === 0) {
      historyRef.current = Array.from({ length: timeSteps }, () =>
        staticBaseline.map((a) => a * 0.3), // Start dim, let real data brighten it.
      );
    }
    // Initialize signals from static baseline.
    for (let i = 0; i < BANDS.length; i++) {
      if (signalRef.current[i] === 0) {
        signalRef.current[i] = staticBaseline[i] * 0.3;
      }
    }

    // Poll interval: fetch counters every 500ms (matches MRI pre-render rate).
    let pollTimer: ReturnType<typeof setInterval>;
    let pendingDeltas = new Map<string, number>();

    const startPolling = () => {
      pollTimer = setInterval(async () => {
        pendingDeltas = await pollCounters();
      }, 500);
    };
    startPolling();

    function draw() {
      if (!ctx) return;
      frameRef.current++;
      const signals = signalRef.current;

      // Apply pending deltas as spikes, then decay.
      for (let b = 0; b < BANDS.length; b++) {
        const key = BANDS[b].key;
        const delta = pendingDeltas.get(key) || 0;

        // Spike: each new call adds SPIKE_GAIN, capped at 1.0.
        if (delta > 0) {
          signals[b] = Math.min(1.0, signals[b] + delta * SPIKE_GAIN);
          // Clear the delta so we don't re-apply next frame.
          pendingDeltas.set(key, 0);
        }

        // Exponential decay: signal fades toward baseline.
        // baseline = static diagnosis health ratio (faint glow when idle).
        const baseline = staticBaseline[b] * 0.08;
        signals[b] = baseline + (signals[b] - baseline) * DECAY;
      }

      // Shift history, push current signals.
      const history = historyRef.current;
      if (history.length >= timeSteps) history.shift();
      history.push(Array.from(signals));

      // Clear.
      ctx.fillStyle = '#0a0e17';
      ctx.fillRect(0, 0, w, h);

      // Draw spectrogram (heat map of signal history).
      for (let t = 0; t < history.length; t++) {
        for (let b = 0; b < BANDS.length; b++) {
          const val = history[t]?.[b] || 0;
          const band = BANDS[b];
          const r = parseInt(band.color.slice(1, 3), 16);
          const g = parseInt(band.color.slice(3, 5), 16);
          const bl = parseInt(band.color.slice(5, 7), 16);
          ctx.fillStyle = `rgba(${r},${g},${bl},${val * 0.85 + 0.03})`;
          ctx.fillRect(t * cellW, b * bandHeight, cellW + 1, bandHeight);
        }
      }

      // Draw oscilloscope traces over spectrogram.
      ctx.lineWidth = 1.5;
      for (let b = 0; b < BANDS.length; b++) {
        const baseY = b * bandHeight + bandHeight / 2;
        ctx.strokeStyle = BANDS[b].color;
        ctx.globalAlpha = 0.85;
        ctx.beginPath();
        for (let t = 0; t < history.length; t++) {
          const val = history[t]?.[b] || 0;
          const y = baseY + (val - 0.5) * bandHeight * 0.8;
          if (t === 0) ctx.moveTo(t * cellW, y);
          else ctx.lineTo(t * cellW, y);
        }
        ctx.stroke();
        ctx.globalAlpha = 1;
      }

      // Band labels + current value.
      ctx.font = '10px monospace';
      ctx.textBaseline = 'middle';
      for (let b = 0; b < BANDS.length; b++) {
        const val = signals[b];
        ctx.fillStyle = BANDS[b].color;
        ctx.globalAlpha = 0.75;
        ctx.fillText(BANDS[b].name, 4, b * bandHeight + bandHeight / 2);
        // Current amplitude as percentage on the right.
        ctx.globalAlpha = val > 0.5 ? 1.0 : 0.4;
        ctx.fillText(`${(val * 100).toFixed(0)}%`, w - 36, b * bandHeight + bandHeight / 2);
        ctx.globalAlpha = 1;
      }

      // NaN flash indicators from static diagnosis (red pulse on right edge).
      for (const repo of diagnosis.repos) {
        for (const mod of repo.modules) {
          if (mod.nan_risk > 0 && frameRef.current % 60 < 3) {
            const band = moduleToBand(mod.name);
            ctx.fillStyle = 'rgba(255, 99, 125, 0.35)';
            ctx.fillRect(w - cellW * 4, band * bandHeight, cellW * 4, bandHeight);
          }
        }
      }

      animRef.current = requestAnimationFrame(draw);
    }

    draw();

    const handleResize = () => {
      rect = canvas.getBoundingClientRect();
      w = rect.width;
      h = rect.height;
      canvas.width = w * dpr;
      canvas.height = h * dpr;
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.scale(dpr, dpr);
    };
    window.addEventListener('resize', handleResize);

    return () => {
      cancelAnimationFrame(animRef.current);
      clearInterval(pollTimer);
      window.removeEventListener('resize', handleResize);
    };
  }, [staticBaseline, diagnosis, pollCounters]);

  return (
    <div className="viz-waveform">
      <canvas ref={canvasRef} style={{ width: '100%', height: '100%' }} />
    </div>
  );
}
