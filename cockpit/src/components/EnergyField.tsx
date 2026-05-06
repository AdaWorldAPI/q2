import { useEffect, useRef } from 'react';
import type { WireResonanceDto } from '../hooks/useShaderStream';

interface EnergyFieldProps {
  resonance: WireResonanceDto | null;
  width?: number;
  height?: number;
}

const COLS = 64;
const ROWS = 64;

export function EnergyField({ resonance, width = 256, height = 256 }: EnergyFieldProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const cellW = width / COLS;
    const cellH = height / ROWS;

    // Build a sparse energy map from top_k
    const energyMap = new Float32Array(COLS * ROWS);
    if (resonance?.top_k) {
      for (const [idx, energy] of resonance.top_k) {
        const cell = idx % (COLS * ROWS);
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
        // Cyan → green gradient by energy
        const hue = 175 - e * 55; // 175 (cyan) → 120 (green)
        ctx.fillStyle = `hsla(${hue}, 100%, 55%, ${alpha.toFixed(2)})`;
        ctx.fillRect(c * cellW, r * cellH, cellW - 0.5, cellH - 0.5);
      }
    }

    // Draw top-k labels
    if (resonance?.top_k?.length) {
      ctx.font = '8px monospace';
      ctx.fillStyle = '#ffffff99';
      for (const [idx, energy] of resonance.top_k.slice(0, 3)) {
        const cell = idx % (COLS * ROWS);
        const cx = (cell % COLS) * cellW + 2;
        const cy = Math.floor(cell / COLS) * cellH + 8;
        ctx.fillText(`${idx}`, cx, cy);
      }
    }
  }, [resonance, width, height]);

  return (
    <div className="energy-field-wrap">
      <div className="panel-header" style={{ padding: '4px 8px' }}>
        <div className="panel-title">
          <span style={{ fontSize: '11px', color: 'var(--muted)' }}>Ψ resonance field</span>
          {resonance && (
            <span style={{ fontSize: '10px', color: 'var(--muted)', marginLeft: 8 }}>
              cycle {resonance.cycle_count} · {resonance.active_count} active · H={resonance.entropy.toFixed(2)}
              {resonance.converged && <span style={{ color: 'var(--green)', marginLeft: 4 }}>✓converged</span>}
            </span>
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
