import type { RenderInfo } from '../types'

interface Props {
  render: RenderInfo
}

export function RenderMeta({ render }: Props) {
  const entries: [string, string][] = []
  if (render.input_path) entries.push(['input', render.input_path])
  if (render.output_path) entries.push(['output', render.output_path])
  if (render.format_target) entries.push(['format', render.format_target])
  if (render.git_hash) entries.push(['git', render.git_hash])
  if (render.started_at_unix_ms !== undefined) {
    entries.push(['started', new Date(render.started_at_unix_ms).toISOString()])
  }
  if (render.total_duration_ms !== undefined) {
    entries.push(['total', `${render.total_duration_ms.toFixed(1)} ms`])
  }

  if (entries.length === 0) return null
  return (
    <dl className="render-meta">
      {entries.map(([k, v]) => (
        <RenderRow key={k} label={k} value={v} />
      ))}
    </dl>
  )
}

function RenderRow({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt>{label}</dt>
      <dd style={{ margin: 0 }}>{value}</dd>
    </>
  )
}
