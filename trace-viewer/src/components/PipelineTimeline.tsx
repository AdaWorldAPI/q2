import type { TraceEntry } from '../types'

interface Props {
  entries: TraceEntry[]
  selectedIndex: number | null
  onSelect: (i: number) => void
}

/** Horizontal strip of stage buttons — one per pipeline entry. */
export function PipelineTimeline({ entries, selectedIndex, onSelect }: Props) {
  if (entries.length === 0) {
    return <div className="empty">No pipeline entries.</div>
  }
  return (
    <div className="timeline" role="tablist" aria-label="Pipeline stages">
      {entries.map((entry, i) => {
        const status = entry.status ?? 'ok'
        return (
          <button
            key={i}
            role="tab"
            aria-selected={selectedIndex === i}
            className={`stage status-${status}${
              selectedIndex === i ? ' active' : ''
            }`}
            onClick={() => onSelect(i)}
            title={entry.error?.message}
          >
            <span className="name">{entry.stage}</span>
            {entry.data_kind && (
              <span className="kind">{entry.data_kind}</span>
            )}
            {entry.duration_ms !== undefined && (
              <span className="duration">
                {entry.duration_ms.toFixed(1)} ms
              </span>
            )}
          </button>
        )
      })}
    </div>
  )
}
