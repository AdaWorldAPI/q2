import { useMemo, useState } from 'react'

interface Props {
  value: unknown
}

/**
 * Collapsible, searchable JSON tree. Used for AST entries where the payload
 * is a serialized Pandoc AST, but works for any JSON value.
 *
 * The search matches on stringified keys and primitive values; matching
 * nodes have their ancestor <details> elements forced open and a `.match`
 * class applied to the matched token.
 */
export function AstTree({ value }: Props) {
  const [query, setQuery] = useState('')
  const needle = query.trim().toLowerCase()

  const { hasMatch, openPaths } = useMemo(
    () => computeSearch(value, needle),
    [value, needle],
  )

  return (
    <div>
      <div style={{ marginBottom: 8, display: 'flex', gap: 8, alignItems: 'center' }}>
        <input
          type="search"
          placeholder="Search…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          style={{
            padding: '2px 6px',
            border: '1px solid var(--border)',
            borderRadius: 4,
            font: 'inherit',
            background: 'var(--bg)',
            color: 'inherit',
            flex: 1,
            maxWidth: 320,
          }}
        />
        {needle && (
          <span style={{ color: 'var(--fg-muted)', fontSize: 12 }}>
            {hasMatch ? 'match' : 'no match'}
          </span>
        )}
      </div>
      <div className="ast-tree">
        <Node path="" value={value} needle={needle} openPaths={openPaths} />
      </div>
    </div>
  )
}

interface NodeProps {
  path: string
  label?: string
  value: unknown
  needle: string
  openPaths: Set<string>
}

function Node({ path, label, value, needle, openPaths }: NodeProps) {
  if (value === null) {
    return <LeafLine label={label} text="null" needle={needle} />
  }
  if (Array.isArray(value)) {
    const summary = `${label !== undefined ? `${label}: ` : ''}Array(${value.length})`
    const forceOpen = openPaths.has(path)
    return (
      <details open={forceOpen || value.length <= 3}>
        <summary>{highlight(summary, needle)}</summary>
        <ul>
          {value.map((child, i) => (
            <li key={i}>
              <Node
                path={`${path}[${i}]`}
                label={String(i)}
                value={child}
                needle={needle}
                openPaths={openPaths}
              />
            </li>
          ))}
        </ul>
      </details>
    )
  }
  if (typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>)
    const summary = `${label !== undefined ? `${label}: ` : ''}Object(${entries.length})`
    const forceOpen = openPaths.has(path)
    return (
      <details open={forceOpen || entries.length <= 4}>
        <summary>{highlight(summary, needle)}</summary>
        <ul>
          {entries.map(([k, v]) => (
            <li key={k}>
              <Node
                path={`${path}.${k}`}
                label={k}
                value={v}
                needle={needle}
                openPaths={openPaths}
              />
            </li>
          ))}
        </ul>
      </details>
    )
  }
  // primitive
  const rendered =
    typeof value === 'string' ? JSON.stringify(value) : String(value)
  return <LeafLine label={label} text={rendered} needle={needle} />
}

function LeafLine({
  label,
  text,
  needle,
}: {
  label?: string
  text: string
  needle: string
}) {
  const display = label !== undefined ? `${label}: ${text}` : text
  return <span className="leaf">{highlight(display, needle)}</span>
}

function highlight(text: string, needle: string): React.ReactNode {
  if (!needle) return text
  const lower = text.toLowerCase()
  const idx = lower.indexOf(needle)
  if (idx < 0) return text
  return (
    <>
      {text.slice(0, idx)}
      <span className="match">{text.slice(idx, idx + needle.length)}</span>
      {text.slice(idx + needle.length)}
    </>
  )
}

/** Walk the tree; return all paths that contain a match, so their ancestors
 *  can be force-opened. */
function computeSearch(
  value: unknown,
  needle: string,
): { hasMatch: boolean; openPaths: Set<string> } {
  const openPaths = new Set<string>()
  if (!needle) return { hasMatch: false, openPaths }

  function visit(path: string, v: unknown): boolean {
    if (v === null) return matches('null', needle)
    if (Array.isArray(v)) {
      let any = false
      for (let i = 0; i < v.length; i++) {
        if (visit(`${path}[${i}]`, v[i])) {
          any = true
        }
      }
      if (any) openPaths.add(path)
      return any
    }
    if (typeof v === 'object') {
      let any = false
      for (const [k, child] of Object.entries(v as Record<string, unknown>)) {
        if (matches(k, needle)) any = true
        if (visit(`${path}.${k}`, child)) any = true
      }
      if (any) openPaths.add(path)
      return any
    }
    const text = typeof v === 'string' ? v : String(v)
    return matches(text, needle)
  }
  const hasMatch = visit('', value)
  return { hasMatch, openPaths }
}

function matches(haystack: string, needle: string): boolean {
  return haystack.toLowerCase().includes(needle)
}
