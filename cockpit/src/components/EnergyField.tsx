import { useEffect, useRef } from 'react';
import type { WireShaderResonance } from '../hooks/useShaderStream';
import { safeNum, clamp } from '../diagnostics/safe';

interface EnergyFieldProps {
  resonance: WireShaderResonance | null;
  width?: number;
  height?: number;
}

const COLS = 64;
const ROWS = 64;

export function EnergyField({ resonance, width = 256, height = 200 }: EnergyFieldProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const cellW = width / COLS;
    const cellH = height / ROWS;

    // Build a sparse energy map from top_k — defensively handle bad shape
    const energyMap = new Float32Array(COLS * ROWS);
    if (resonance && Array.isArray(resonance.top_k)) {
      for (const hit of resonance.top_k) {
        if (!hit || typeof hit !== 'object') continue;
        const idx = safeNum(hit.row, 0, 'resonance.top_k[].row');
        const energy = clamp(hit.resonance, 0, 1, 'resonance.top_k[].resonance');
        if (energy <= 0) continue;
        const cell = Math.floor(idx) % (COLS * ROWS);
        if (cell < 0 || cell >= COLS * ROWS) continue;
        energyMap[cell] = Math.max(energyMap[cell], energy);
        // Gaussian spread to neighbors
        const cx = cell % COLS;
        const cy = Math.floor(cell / COLS);
        for (let dy = -2; dy <= 2; dy++) {
          for (let dx = -2; dx <= 2; dx++) {
            const nx = cx + dx, ny = cy + dy;
            if (nx < 0 || nx >= COLS || ny < 0 || ny >= ROWS) continue;
            const dist = Math.sqrt(dx * dx + dy * dy);
            const spread = energy * Math.exp(-dist * 1.5);
            energyMap[ny * COLS + nx] = Math.max(energyMap[ny * COLS + nx], spread);
          }
        }
      }
    }

    // Draw cells
    ctx.clearRect(0, 0, width, height);
    for (let r = 0; r < ROWS; r++) {
      for (let c = 0; c < COLS; c++) {
        const e = energyMap[r * COLS + c];
        if (e < 0.01) continue;
        const alpha = Math.min(e, 1.0);
        const hue = 175 - e * 55;
        ctx.fillStyle = `hsla(${hue}, 100%, 55%, ${alpha.toFixed(2)})`;
        ctx.fillRect(c * cellW, r * cellH, cellW - 0.5, cellH - 0.5);
      }
    }

    // Draw "no data" placeholder
    if (!resonance || !Array.isArray(resonance.top_k) || resonance.top_k.length === 0) {
      ctx.font = '11px monospace';
      ctx.fillStyle = '#444';
      ctx.textAlign = 'center';
      ctx.fillText('Ψ no resonance events', width / 2, height / 2);
      ctx.fillStyle = '#333';
      ctx.font = '9px monospace';
      ctx.fillText('Shift+D for diagnostics', width / 2, height / 2 + 14);
      ctx.textAlign = 'left';
      return;
    }

    // Draw top-k labels — show row + cycle_index
    ctx.font = '8px monospace';
    ctx.fillStyle = '#ffffff99';
    for (const hit of resonance.top_k.slice(0, 3)) {
      if (!hit || typeof hit !== 'object') continue;
      const idx = safeNum(hit.row, 0, 'resonance.top_k_label.row');
      const cycleIdx = safeNum(hit.cycle_index, 0, 'resonance.top_k_label.cycle_index');
      const cell = Math.floor(idx) % (COLS * ROWS);
      const cx = (cell % COLS) * cellW + 2;
      const cy = Math.floor(cell / COLS) * cellH + 8;
      ctx.fillText(`${Math.floor(idx)}@${cycleIdx}`, cx, cy);
    }
  }, [resonance, width, height]);

  const cyclesUsed = resonance ? safeNum(resonance.cycles_used, 0, 'resonance.cycles_used') : 0;
  const hitCount = resonance ? safeNum(resonance.hit_count, 0, 'resonance.hit_count') : 0;
  const entropy = resonance ? safeNum(resonance.entropy, 0, 'resonance.entropy') : 0;
  const stdDev = resonance ? safeNum(resonance.std_dev, 0, 'resonance.std_dev') : 0;

  return (
    <div className="energy-field-wrap">
      <div className="panel-header" style={{ padding: '4px 8px' }}>
        <div className="panel-title">
          <span style={{ fontSize: '11px', color: 'var(--muted)' }}>Ψ resonance field</span>
          {resonance ? (
            <span style={{ fontSize: '10px', color: 'var(--muted)', marginLeft: 8 }}>
              cycles {cyclesUsed} · {hitCount} hits · H={entropy.toFixed(2)} · σ={stdDev.toFixed(2)}
            </span>
          ) : (
            <span style={{ fontSize: '10px', color: '#666', marginLeft: 8 }}>· awaiting</span>
          )}
        </div>
      </div>
      <canvas
        ref={canvasRef}
        width={width}
        height={height}
        style={{ display: 'block', width: '100%', height: `${height}px`, background: '#0a0e14' }}
      />
    </div>
  );
}
