import type { WireFreeEnergy } from '../hooks/useShaderStream';

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
  const fe = freeEnergy?.free_energy ?? 0.5;
  const likelihood = freeEnergy?.likelihood ?? 0.5;
  const kl = freeEnergy?.kl ?? 0.5;
  const belowHomeostasis = freeEnergy?.below_homeostasis ?? false;

  // Dial: 210° span, 0=low (good), 1=high (bad)
  const START = 210;
  const END = -30;
  const SPAN = 240;
  const needleDeg = START - fe * SPAN;

  const cx = 60, cy = 60, r = 48;
  const needleX = cx + 38 * Math.cos((needleDeg * Math.PI) / 180);
  const needleY = cy + 38 * Math.sin((needleDeg * Math.PI) / 180);

  const color = belowHomeostasis ? '#35d07f' : fe > 0.7 ? '#e53935' : fe > 0.4 ? '#ffb547' : '#35d07f';

  return (
    <div className="free-energy-dial">
      <div className="panel-header" style={{ padding: '4px 8px' }}>
        <div className="panel-title">
          <span style={{ fontSize: '11px', color: 'var(--muted)' }}>F free energy</span>
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
            {fe.toFixed(3)}
          </text>
          <text x="14" y="72" fontSize="7" fill="#35d07f" fontFamily="monospace">L</text>
          <text x="100" y="72" fontSize="7" fill="#ffb547" fontFamily="monospace">KL</text>
        </svg>
        <div className="dial-stats">
          <div><span style={{ color: '#35d07f' }}>L</span> {likelihood.toFixed(3)}</div>
          <div><span style={{ color: '#ffb547' }}>KL</span> {kl.toFixed(3)}</div>
          {belowHomeostasis && (
            <div style={{ color: '#35d07f', fontSize: '10px' }}>⊕ homeostasis</div>
          )}
        </div>
      </div>
    </div>
  );
}
