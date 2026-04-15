import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { App } from './App'
import type { TraceSource } from './trace-source'
import type { TraceDocument, TraceListing } from './types'

function makeSource(
  listings: TraceListing[],
  traces: Record<string, TraceDocument>,
): TraceSource {
  return {
    list: async () => listings,
    load: async (doc) => {
      const t = traces[doc]
      if (!t) throw new Error(`not found: ${doc}`)
      return t
    },
  }
}

const sampleTrace: TraceDocument = {
  schema_version: 1,
  render: {
    input_path: 'doc.qmd',
    format_target: 'html',
    git_hash: 'abcd123',
    total_duration_ms: 42,
  },
  pipeline: [
    {
      stage: 'parse',
      index: 0,
      data_kind: 'DocumentAst',
      data: { ast: { blocks: [] } },
      duration_ms: 1.0,
      status: 'ok',
    },
    {
      stage: 'engine-execution',
      index: 1,
      status: 'error',
      error: { message: 'kernel died' },
    },
  ],
}

describe('App', () => {
  it('renders empty state when no traces', async () => {
    const src = makeSource([], {})
    render(<App source={src} />)
    await screen.findByText(/None found|Select a trace/i)
  })

  it('renders the first trace and defaults to the errored stage', async () => {
    const src = makeSource(
      [{ doc: 'hello' }],
      { hello: sampleTrace },
    )
    render(<App source={src} />)
    await screen.findByText('engine-execution', { selector: '.stage-name' })
    expect(screen.getByText('kernel died')).toBeInTheDocument()
  })

  it('shows the render meta', async () => {
    const src = makeSource([{ doc: 'hello' }], { hello: sampleTrace })
    render(<App source={src} />)
    await screen.findByText('doc.qmd')
    expect(screen.getByText('abcd123')).toBeInTheDocument()
  })
})
