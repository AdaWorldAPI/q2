import { useState } from 'react';

interface ObserveCellProps {
  onExecute: (timeWindow: string) => void;
  result: { events: string[]; nodeCount: number } | null;
}

const TIME_WINDOWS = [
  'March 2026',
  'February 2026',
  'January 2026',
  'Q4 2025',
  'Q3 2025',
  'Full Timeline',
];

export function ObserveCell({ onExecute, result }: ObserveCellProps) {
  const [timeWindow, setTimeWindow] = useState('March 2026');
  const [loading, setLoading] = useState(false);

  const handleLoad = () => {
    setLoading(true);
    onExecute(timeWindow);
    setTimeout(() => setLoading(false), 800);
  };

  return (
    <div className="nb-cell nb-cell--observe">
      <div className="nb-cell-header">
        <span className="nb-cell-type nb-cell-type--observe">OBSERVE</span>
        <span className="nb-cell-desc">Load time window</span>
      </div>
      <div className="nb-cell-body">
        <div className="nb-cell-row">
          <label>Time window:</label>
          <select value={timeWindow} onChange={(e) => setTimeWindow(e.target.value)} className="nb-select">
            {TIME_WINDOWS.map((w) => <option key={w} value={w}>{w}</option>)}
          </select>
          <button className="nb-run-btn" onClick={handleLoad} disabled={loading}>
            {loading ? 'Loading...' : 'Load'}
          </button>
        </div>
        {result && (
          <div className="nb-cell-result">
            <strong>Loaded {result.events.length} events ({result.nodeCount} nodes):</strong>
            <ul className="nb-event-list">
              {result.events.slice(0, 8).map((e, i) => (
                <li key={i}>{e}</li>
              ))}
              {result.events.length > 8 && (
                <li className="nb-more">+{result.events.length - 8} more...</li>
              )}
            </ul>
          </div>
        )}
      </div>
    </div>
  );
}
