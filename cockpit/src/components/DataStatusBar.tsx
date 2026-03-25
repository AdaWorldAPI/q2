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

export function DataStatusBar({ sources }: DataStatusBarProps) {
  return (
    <span className="data-status-bar">
      {sources.map((src) => {
        const info = STATUS_ICONS[src.status] || STATUS_ICONS.loading;
        return (
          <span
            key={src.name}
            className="data-status-item"
            title={`${src.name}: ${src.detail}${src.elapsed_ms ? ` (${src.elapsed_ms}ms)` : ''}`}
          >
            <span className="data-status-dot" style={{ background: info.color }} />
            <span className="data-status-name">{src.file.split('/').pop()?.split('.')[0]}</span>
          </span>
        );
      })}
    </span>
  );
}
