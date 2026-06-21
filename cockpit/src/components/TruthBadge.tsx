// Reusable NARS truth value display: (f, c, gate)

interface TruthBadgeProps {
  f: number;
  c: number;
  gate: 'FLOW' | 'HOLD' | 'BLOCK';
  compact?: boolean;
}

const GATE_COLORS: Record<string, string> = {
  FLOW: '#35d07f',
  HOLD: '#ffb547',
  BLOCK: '#ff637d',
};

const GATE_ICONS: Record<string, string> = {
  FLOW: '\u2713',
  HOLD: '\u25CF',
  BLOCK: '\u2717',
};

export function TruthBadge({ f, c, gate, compact }: TruthBadgeProps) {
  const color = GATE_COLORS[gate] || '#93a9bf';

  if (compact) {
    return (
      <span className="truth-badge truth-badge--compact" style={{ color }}>
        {gate} {f.toFixed(2)}
      </span>
    );
  }

  return (
    <span className="truth-badge" style={{ borderColor: `${color}40` }}>
      <span className="truth-values">
        f={f.toFixed(2)}, c={c.toFixed(2)}
      </span>
      <span className="truth-gate" style={{ color, borderColor: `${color}40` }}>
        {GATE_ICONS[gate]} {gate}
      </span>
    </span>
  );
}
