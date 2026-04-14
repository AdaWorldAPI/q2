import { useEffect, useState } from 'react'
import { PipelineTimeline } from './components/PipelineTimeline'
import { RenderMeta } from './components/RenderMeta'
import { StageDetail } from './components/StageDetail'
import { TraceList } from './components/TraceList'
import type { TraceSource } from './trace-source'
import type { TraceDocument, TraceListing } from './types'

interface Props {
  source: TraceSource
}

export function App({ source }: Props) {
  const [listings, setListings] = useState<TraceListing[] | null>(null)
  const [listError, setListError] = useState<string | null>(null)
  const [selectedDoc, setSelectedDoc] = useState<string | null>(null)
  const [trace, setTrace] = useState<TraceDocument | null>(null)
  const [traceError, setTraceError] = useState<string | null>(null)
  const [selectedStageIdx, setSelectedStageIdx] = useState<number | null>(null)

  // Initial listing load.
  useEffect(() => {
    let cancelled = false
    source
      .list()
      .then((xs) => {
        if (cancelled) return
        setListings(xs)
        if (xs.length > 0 && selectedDoc === null) {
          setSelectedDoc(xs[0].doc)
        }
      })
      .catch((e) => {
        if (!cancelled) setListError(String(e))
      })
    return () => {
      cancelled = true
    }
    // Only runs once; we deliberately don't depend on selectedDoc.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [source])

  // Load the selected trace.
  useEffect(() => {
    if (!selectedDoc) {
      setTrace(null)
      return
    }
    let cancelled = false
    setTrace(null)
    setTraceError(null)
    setSelectedStageIdx(null)
    source
      .load(selectedDoc)
      .then((t) => {
        if (cancelled) return
        setTrace(t)
        // Default-select the first errored stage if any, else the first entry.
        const errIdx = t.pipeline.findIndex((e) => (e.status ?? 'ok') === 'error')
        setSelectedStageIdx(errIdx >= 0 ? errIdx : t.pipeline.length > 0 ? 0 : null)
      })
      .catch((e) => {
        if (!cancelled) setTraceError(String(e))
      })
    return () => {
      cancelled = true
    }
  }, [source, selectedDoc])

  const selectedEntry =
    trace && selectedStageIdx !== null ? trace.pipeline[selectedStageIdx] : null

  return (
    <div className="app-root">
      <header className="app-header">
        <h1>Quarto trace viewer</h1>
        {selectedDoc && <span className="muted">/ {selectedDoc}</span>}
        {trace?.schema_version !== undefined && (
          <span className="muted" style={{ marginLeft: 'auto' }}>
            schema v{trace.schema_version}
          </span>
        )}
      </header>
      <div className="app-body">
        <aside className="sidebar">
          {listError ? (
            <div className="error-box" style={{ margin: 12 }}>
              {listError}
            </div>
          ) : listings === null ? (
            <div className="empty">Loading…</div>
          ) : (
            <TraceList
              listings={listings}
              selected={selectedDoc}
              onSelect={setSelectedDoc}
            />
          )}
        </aside>
        <main className="main-panel">
          {!selectedDoc ? (
            <div className="empty">Select a trace from the sidebar.</div>
          ) : traceError ? (
            <div className="error-box">{traceError}</div>
          ) : !trace ? (
            <div className="empty">Loading trace…</div>
          ) : (
            <>
              <RenderMeta render={trace.render} />
              <PipelineTimeline
                entries={trace.pipeline}
                selectedIndex={selectedStageIdx}
                onSelect={setSelectedStageIdx}
              />
              {selectedEntry ? (
                <StageDetail entry={selectedEntry} />
              ) : (
                <div className="empty">Select a stage.</div>
              )}
            </>
          )}
        </main>
      </div>
    </div>
  )
}
