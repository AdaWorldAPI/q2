import { useRef, useEffect, useMemo } from 'react';
import type { StackDiagnosis } from '../../../hooks/useNeuralDiagnosis';

interface WaveformModeProps {
  diagnosis: StackDiagnosis;
}

// Band definitions matching the spec
const BANDS = [
  { name: 'Adjacency', color: '#1a3a5c', category: 'low' },
  { name: 'Lance I/O', color: '#1a4a5c', category: 'low' },
  { name: 'Storage', color: '#1a5a5c', category: 'low' },
  { name: 'SIMD', color: '#00d4ff', category: 'mid' },
  { name: 'CAM-PQ', color: '#4dd0e1', category: 'mid' },
  { name: 'NARS', color: '#ffb547', category: 'mid' },
  { name: 'Semiring', color: '#ff9800', category: 'mid' },
  { name: 'Strategy', color: '#e040fb', category: 'high' },
  { name: 'Thinking', color: '#ff4081', category: 'high' },
  { name: 'Collapse', color: '#ff637d', category: 'high' },
  { name: 'Elevation', color: '#35d07f', category: 'high' },
  { name: 'MUL', color: '#ffffff', category: 'high' },
];

// Map modules to bands
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

  // Pre-compute band amplitudes from diagnosis
  const bandAmplitudes = useMemo(() => {
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

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);
    const w = rect.width;
    const h = rect.height;

    const bandHeight = h / BANDS.length;
    const timeSteps = 200;
    // History buffer for scrolling spectrogram
    const history: number[][] = Array.from({ length: timeSteps }, () =>
      bandAmplitudes.map((a) => a + (Math.random() - 0.5) * 0.15),
    );

    function draw() {
      if (!ctx) return;
      frameRef.current++;

      // Shift history
      history.shift();
      history.push(
        bandAmplitudes.map((a) =>
          Math.max(0, Math.min(1, a + Math.sin(frameRef.current * 0.02 + Math.random() * 2) * 0.08 + (Math.random() - 0.5) * 0.05)),
        ),
      );

      // Clear
      ctx.fillStyle = '#0a0e17';
      ctx.fillRect(0, 0, w, h);

      // Draw spectrogram
      const cellW = w / timeSteps;
      for (let t = 0; t < timeSteps; t++) {
        for (let b = 0; b < BANDS.length; b++) {
          const val = history[t][b];
          const intensity = Math.floor(val * 255);
          const band = BANDS[b];
          // Parse hex color and apply intensity
          const r = parseInt(band.color.slice(1, 3), 16);
          const g = parseInt(band.color.slice(3, 5), 16);
          const bl = parseInt(band.color.slice(5, 7), 16);
          ctx.fillStyle = `rgba(${r},${g},${bl},${val * 0.8 + 0.05})`;
          ctx.fillRect(t * cellW, b * bandHeight, cellW + 1, bandHeight);
        }
      }

      // Draw oscilloscope lines over spectrogram
      ctx.lineWidth = 1.5;
      for (let b = 0; b < BANDS.length; b++) {
        const baseY = b * bandHeight + bandHeight / 2;
        ctx.strokeStyle = BANDS[b].color;
        ctx.globalAlpha = 0.8;
        ctx.beginPath();
        for (let t = 0; t < timeSteps; t++) {
          const val = history[t][b];
          const y = baseY + (val - 0.5) * bandHeight * 0.8;
          if (t === 0) ctx.moveTo(t * cellW, y);
          else ctx.lineTo(t * cellW, y);
        }
        ctx.stroke();
        ctx.globalAlpha = 1;
      }

      // Band labels
      ctx.font = '10px monospace';
      ctx.textBaseline = 'middle';
      for (let b = 0; b < BANDS.length; b++) {
        ctx.fillStyle = BANDS[b].color;
        ctx.globalAlpha = 0.7;
        ctx.fillText(BANDS[b].name, 4, b * bandHeight + bandHeight / 2);
        ctx.globalAlpha = 1;
      }

      // NaN flash indicators
      for (const repo of diagnosis.repos) {
        for (const mod of repo.modules) {
          if (mod.nan_risk > 0 && Math.random() < 0.02) {
            const band = moduleToBand(mod.name);
            ctx.fillStyle = 'rgba(255, 99, 125, 0.4)';
            ctx.fillRect(w - cellW * 3, band * bandHeight, cellW * 3, bandHeight);
          }
        }
      }

      animRef.current = requestAnimationFrame(draw);
    }

    draw();
    return () => cancelAnimationFrame(animRef.current);
  }, [bandAmplitudes, diagnosis]);

  return (
    <div className="viz-waveform">
      <canvas ref={canvasRef} style={{ width: '100%', height: '100%' }} />
    </div>
  );
}
