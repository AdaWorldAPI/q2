interface CoverageBarProps {
  alive: number;
  dead: number;
  stub: number;
  nan: number;
  total: number;
}

export function CoverageBar({ alive, dead, stub, nan, total }: CoverageBarProps) {
  if (total === 0) return null;
  const alivePct = (alive / total) * 100;
  const deadPct = (dead / total) * 100;
  const stubPct = (stub / total) * 100;

  return (
    <div className="coverage-bar">
      <div className="coverage-bar-track">
        <div className="coverage-seg coverage-alive" style={{ width: `${alivePct}%` }} />
        <div className="coverage-seg coverage-stub" style={{ width: `${stubPct}%` }} />
        <div className="coverage-seg coverage-dead" style={{ width: `${deadPct}%` }} />
      </div>
    </div>
  );
}
