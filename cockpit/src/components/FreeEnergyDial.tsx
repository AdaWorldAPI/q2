import type { WireFreeEnergy } from '../hooks/useShaderStream';
import { clamp, fmt, safeNum } from '../diagnostics/safe';

interface FreeEnergyDialProps {
  freeEnergy: WireFreeEnergy | null;
}

function arc(cx: number, cy: number, r: number, startDeg: number, endDeg: number): string {
  const toRad = (d: number) => (d * Math.PI) / 180;
  const x1 = cx + r * Math.cos(toRad(startDeg));
  const y1 = cy + r * Math.sin(toRad(startDeg));
  const x2 = cx + r * Math.cos(toRad(endDeg));
  const y2 = cy + r * Math.sin(toRad(endDeg));
  const large = Math.abs(endDeg - startDeg) > 180 ? 1 : 0;
  return `M ${x1} ${y1} A ${r} ${r} 0 ${large} 1 ${x2} ${y2}`;
}

export function FreeEnergyDial({ freeEnergy }: FreeEnergyDialProps) {
  // Safe coercion — every numeric field reports NaN/missing to diagnostics
  const fe = clamp(freeEnergy?.free_energy, 0, 2, 'freeEnergy.free_energy');
  const likelihood = clamp(freeEnergy?.likelihood, 0, 1, 'freeEnergy.likelihood');
  const kl = clamp(freeEnergy?.kl, 0, 1, 'freeEnergy.kl');
  const belowHomeostasis = freeEnergy?.below_homeostasis === true;
  const stale = freeEnergy === null;

  // Dial: 240° span, 0=low (good), 1=high (bad)
  const START = 210;
  const SPAN = 240;
  // fe is clamped to [0, 2] — the dial saturates above 1
  const dialPos = clamp(fe / 1.0, 0, 1, 'freeEnergy.dialPos');
  const needleDeg = START - dialPos * SPAN;

  const cx = 60, cy = 60, r = 48;
  const needleRad = (needleDeg * Math.PI) / 180;
  const needleX = safeNum(cx + 38 * Math.cos(needleRad), cx, 'dial.needleX');
  const needleY = safeNum(cy + 38 * Math.sin(needleRad), cy, 'dial.needleY');

  const color = stale ? '#666' : belowHomeostasis ? '#35d07f' : fe > 0.7 ? '#e53935' : fe > 0.4 ? '#ffb547' : '#35d07f';

  return (
    <div className="free-energy-dial">
      <div className="panel-header" style={{ padding: '4px 8px' }}>
        <div className="panel-title">
          <span style={{ fontSize: '11px', color: 'var(--muted)' }}>F free energy</span>
          {stale && (
            <span style={{ fontSize: '10px', color: '#666', marginLeft: 8 }} title="No health event received yet">
              · awaiting
            </span>
          )}
        </div>
      </div>
      <div className="dial-body">
        <svg viewBox="0 0 120 80" style={{ width: '100%', maxWidth: 160 }}>
          {/* Background arc */}
          <path d={arc(cx, cy, r, 210, -30)} fill="none" stroke="#1a2030" strokeWidth="8" />
          {/* Likelihood arc (green) */}
          <path
            d={arc(cx, cy, r, 210, 210 - likelihood * SPAN)}
            fill="none" stroke="#35d07f" strokeWidth="4"
          />
          {/* KL arc (yellow) */}
          <path
            d={arc(cx, cy, r - 8, 210, 210 - kl * SPAN)}
            fill="none" stroke="#ffb547" strokeWidth="3"
          />
          {/* Needle */}
          <line
            x1={cx} y1={cy}
            x2={needleX} y2={needleY}
            stroke={color} strokeWidth="2" strokeLinecap="round"
          />
          <circle cx={cx} cy={cy} r="3" fill={color} />
          {/* Labels */}
          <text x={cx} y={cy + 18} textAnchor="middle" fontSize="11" fill={color} fontFamily="monospace">
            {stale ? '—' : fmt(fe, 3, 'freeEnergy.free_energy')}
          </text>
          <text x="14" y="72" fontSize="7" fill="#35d07f" fontFamily="monospace">L</text>
          <text x="100" y="72" fontSize="7" fill="#ffb547" fontFamily="monospace">KL</text>
        </svg>
        <div className="dial-stats">
          <div><span style={{ color: '#35d07f' }}>L</span> {fmt(likelihood, 3, 'freeEnergy.likelihood')}</div>
          <div><span style={{ color: '#ffb547' }}>KL</span> {fmt(kl, 3, 'freeEnergy.kl')}</div>
          {belowHomeostasis && (
            <div style={{ color: '#35d07f', fontSize: '10px' }}>⊕ homeostasis</div>
          )}
          {stale && (
            <div style={{ color: '#666', fontSize: '10px' }}>no /v1/shader/stream events yet</div>
          )}
        </div>
      </div>
    </div>
  );
}
