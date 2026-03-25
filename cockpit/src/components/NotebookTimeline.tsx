interface NotebookTimelineProps {
  startYear: number;
  endYear: number;
  currentYear: number;
  nodeCount: number;
  onYearChange: (year: number) => void;
}

export function NotebookTimeline({ startYear, endYear, currentYear, nodeCount, onYearChange }: NotebookTimelineProps) {
  const years = Array.from({ length: endYear - startYear + 1 }, (_, i) => startYear + i);
  const progress = ((currentYear - startYear) / (endYear - startYear)) * 100;

  return (
    <div className="nb-timeline">
      <div className="nb-timeline-bar">
        <div className="nb-timeline-fill" style={{ width: `${progress}%` }} />
        {years.filter((_, i) => i % 3 === 0).map((y) => (
          <button
            key={y}
            className={`nb-timeline-tick ${y === currentYear ? 'active' : ''}`}
            style={{ left: `${((y - startYear) / (endYear - startYear)) * 100}%` }}
            onClick={() => onYearChange(y)}
          >
            {y}
          </button>
        ))}
      </div>
      <div className="nb-timeline-info">
        <span>{startYear} &mdash; {endYear}</span>
        <span>{nodeCount} nodes in graph</span>
      </div>
    </div>
  );
}
