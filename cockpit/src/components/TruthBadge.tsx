// Reusable NARS truth value display: frequency × confidence → gate

interface TruthBadgeProps {
  f: number;
  c: number;
  gate?: 'FLOW' | 'HOLD' | 'BLOCK';
  compact?: boolean;
}

const GATE_COLORS = {
  FLOW: '#35d07f',
  HOLD: '#ffb547',
  BLOCK: '#ff637d',
};

export function TruthBadge({ f, c, gate, compact }: TruthBadgeProps) {
  const expectation = c * (f - 0.5) + 0.5;
  const gateColor = gate ? GATE_COLORS[gate] : '#93a9bf';
  const barWidth = Math.round(expectation * 100);

  if (compact) {
    return (
      <span className="truth-badge-compact" style={{ color: gateColor }}>
        f={f.toFixed(2)} c={c.toFixed(2)}
        {gate && <span className="truth-gate-dot" style={{ background: gateColor }} />}
      </span>
    );
  }

  return (
    <div className="truth-badge">
      <div className="truth-values">
        <span>f={f.toFixed(2)}</span>
        <span>c={c.toFixed(2)}</span>
        <span>e={expectation.toFixed(2)}</span>
      </div>
      <div className="truth-bar-bg">
        <div className="truth-bar-fill" style={{ width: `${barWidth}%`, background: gateColor }} />
      </div>
      {gate && (
        <span className="truth-gate" style={{ color: gateColor, borderColor: `${gateColor}40` }}>
          {gate}
        </span>
      )}
    </div>
  );
}
