import type { TraceEntry } from '../types'
import { AstTree } from './AstTree'
import { CopyJsonButton } from './CopyJsonButton'
import { HtmlSourceView } from './HtmlSourceView'
import { TextView } from './TextView'

interface Props {
  entry: TraceEntry
}

export function StageDetail({ entry }: Props) {
  const status = entry.status ?? 'ok'
  return (
    <div>
      <div className="detail-header">
        <span className="stage-name">{entry.stage}</span>
        <span className="badge">index {entry.index}</span>
        {entry.data_kind && <span className="badge">{entry.data_kind}</span>}
        {entry.duration_ms !== undefined && (
          <span className="badge">{entry.duration_ms.toFixed(1)} ms</span>
        )}
        <span
          className={
            status === 'error' ? 'badge status-error' : 'badge'
          }
        >
          {status}
        </span>
        <div style={{ marginLeft: 'auto' }}>
          <CopyJsonButton value={entry} label="Copy entry JSON" />
        </div>
      </div>

      {status === 'error' && entry.error ? (
        <div className="error-box">{entry.error.message}</div>
      ) : status === 'skipped' ? (
        <div className="empty">Stage skipped.</div>
      ) : (
        <Payload entry={entry} />
      )}
    </div>
  )
}

function Payload({ entry }: { entry: TraceEntry }) {
  if (entry.data === undefined || entry.data === null) {
    return <div className="empty">No data captured for this entry.</div>
  }
  switch (entry.data_kind) {
    case 'LoadedSource':
      return <LoadedSourceMeta data={entry.data as Record<string, unknown>} />
    case 'DocumentSource':
    case 'ExecutedDocument':
      return <DocumentText data={entry.data as Record<string, unknown>} />
    case 'DocumentAst':
      return <AstTree value={(entry.data as { ast?: unknown }).ast ?? entry.data} />
    case 'RenderedOutput':
    case 'FinalOutput':
      return <RenderedOutputView data={entry.data as Record<string, unknown>} />
    default:
      return <AstTree value={entry.data} />
  }
}

function LoadedSourceMeta({ data }: { data: Record<string, unknown> }) {
  return (
    <dl className="render-meta">
      {Object.entries(data).map(([k, v]) => (
        <Row key={k} label={k} value={v} />
      ))}
    </dl>
  )
}

function DocumentText({ data }: { data: Record<string, unknown> }) {
  const markdown = typeof data.markdown === 'string' ? data.markdown : null
  return (
    <>
      <dl className="render-meta">
        {Object.entries(data)
          .filter(([k]) => k !== 'markdown')
          .map(([k, v]) => (
            <Row key={k} label={k} value={v} />
          ))}
      </dl>
      {markdown !== null ? (
        <TextView text={markdown} />
      ) : (
        <div className="empty">No markdown payload.</div>
      )}
    </>
  )
}

function RenderedOutputView({ data }: { data: Record<string, unknown> }) {
  const content = typeof data.content === 'string' ? data.content : null
  const meta = Object.entries(data).filter(([k]) => k !== 'content')
  return (
    <>
      <dl className="render-meta">
        {meta.map(([k, v]) => (
          <Row key={k} label={k} value={v} />
        ))}
      </dl>
      {content !== null ? (
        <HtmlSourceView html={content} />
      ) : (
        <div className="empty">No content payload.</div>
      )}
    </>
  )
}

function Row({ label, value }: { label: string; value: unknown }) {
  return (
    <>
      <dt>{label}</dt>
      <dd style={{ margin: 0 }}>{renderScalar(value)}</dd>
    </>
  )
}

function renderScalar(v: unknown): string {
  if (v === null || v === undefined) return '—'
  if (typeof v === 'string') return v
  if (typeof v === 'number' || typeof v === 'boolean') return String(v)
  if (Array.isArray(v)) {
    if (v.length === 0) return '[]'
    return v.map((x) => renderScalar(x)).join(', ')
  }
  return JSON.stringify(v)
}
