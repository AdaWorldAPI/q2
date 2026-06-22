interface DataSourceCardProps {
  title: string;
  subtitle: string;
  nodeCount: string;
  edgeCount: string;
  description: string;
  accent: string;
  onLaunch: () => void;
  actionLabel?: string;
}

export function DataSourceCard({
  title,
  subtitle,
  nodeCount,
  edgeCount,
  description,
  accent,
  onLaunch,
  actionLabel = 'Launch',
}: DataSourceCardProps) {
  return (
    <div className="ds-card" style={{ '--ds-accent': accent } as React.CSSProperties}>
      <div className="ds-card-header">
        <h3>{title}</h3>
        <span className="ds-card-sub">{subtitle}</span>
      </div>
      <div className="ds-card-stats">
        <div className="ds-stat">
          <span className="ds-stat-value">{nodeCount}</span>
          <span className="ds-stat-label">nodes</span>
        </div>
        <div className="ds-stat">
          <span className="ds-stat-value">{edgeCount}</span>
          <span className="ds-stat-label">edges</span>
        </div>
      </div>
      <p className="ds-card-desc">{description}</p>
      <button className="ds-card-btn" onClick={onLaunch}>
        {actionLabel} &rarr;
      </button>
    </div>
  );
}
