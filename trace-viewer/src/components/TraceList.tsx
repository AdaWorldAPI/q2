import type { TraceListing } from '../types'

interface Props {
  listings: TraceListing[]
  selected: string | null
  onSelect: (doc: string) => void
}

export function TraceList({ listings, selected, onSelect }: Props) {
  if (listings.length === 0) {
    return (
      <>
        <h2>Traces</h2>
        <div className="empty" style={{ fontSize: 12, padding: 12 }}>
          None found.
        </div>
      </>
    )
  }
  return (
    <>
      <h2>Traces</h2>
      <ul style={{ listStyle: 'none', margin: 0, padding: 0 }}>
        {listings.map((l) => (
          <li
            key={l.doc}
            className={`trace-item${selected === l.doc ? ' active' : ''}`}
            onClick={() => onSelect(l.doc)}
          >
            {l.doc}
          </li>
        ))}
      </ul>
    </>
  )
}
