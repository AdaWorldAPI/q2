interface DataSource {
  name: string;
  file: string;
  status: 'loading' | 'loaded' | 'error' | 'not_found' | 'found' | 'embedded' | 'empty' | 'static';
  detail: string;
  elapsed_ms?: number;
  count?: number;
}

interface DataStatusBarProps {
  sources: DataSource[];
}

const STATUS_ICONS: Record<string, { color: string; label: string }> = {
  loading:   { color: '#93a9bf', label: 'loading' },
  loaded:    { color: '#35d07f', label: 'loaded' },
  found:     { color: '#35d07f', label: 'found' },
  embedded:  { color: '#35d07f', label: 'embedded' },
  static:    { color: '#4dd0e1', label: 'static' },
  error:     { color: '#ff637d', label: 'error' },
  not_found: { color: '#ffb547', label: 'not found' },
  empty:     { color: '#ffb547', label: 'empty' },
};

function buildTooltip(src: DataSource): string {
  const elapsed = src.elapsed_ms ? ` (${src.elapsed_ms}ms)` : '';
  // Make fallback / live state explicit so the user is never misled
  // by a raw HTTP code or a generic "loaded" label.
  switch (src.status) {
    case 'error':
    case 'not_found':
    case 'empty':
      return `${src.name}: FALLBACK / using seed data — ${src.detail}${elapsed}`;
    case 'loaded':
      return `${src.name}: LIVE / loaded from live graph engine — ${src.detail}${elapsed}`;
    case 'found':
      return `${src.name}: LIVE / found on live graph engine — ${src.detail}${elapsed}`;
    case 'embedded':
      return `${src.name}: embedded seed bundle — ${src.detail}${elapsed}`;
    case 'static':
      return `${src.name}: static asset — ${src.detail}${elapsed}`;
    case 'loading':
      return `${src.name}: loading… — ${src.detail}${elapsed}`;
    default:
      return `${src.name}: ${src.detail}${elapsed}`;
  }
}

export function DataStatusBar({ sources }: DataStatusBarProps) {
  return (
    <span className="data-status-bar">
      {sources.map((src) => {
        const info = STATUS_ICONS[src.status] || STATUS_ICONS.loading;
        return (
          <span
            key={src.name}
            className="data-status-item"
            title={buildTooltip(src)}
          >
            <span className="data-status-dot" style={{ background: info.color }} />
            <span className="data-status-name">{src.file.split('/').pop()?.split('.')[0]}</span>
          </span>
        );
      })}
    </span>
  );
}
