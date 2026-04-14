import { afterEach, describe, expect, it, vi } from 'vitest'
import { HttpTraceSource } from './trace-source'

describe('HttpTraceSource', () => {
  afterEach(() => {
    vi.restoreAllMocks()
  })

  it('list() returns traces from /api/traces', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(
        JSON.stringify({
          trace_dir: '/tmp/.quarto/trace',
          traces: [{ doc: 'hello', path: '/tmp/.quarto/trace/hello/latest.json' }],
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      ),
    )
    vi.stubGlobal('fetch', fetchMock)

    const src = new HttpTraceSource('')
    const xs = await src.list()
    expect(xs).toHaveLength(1)
    expect(xs[0].doc).toBe('hello')
    expect(fetchMock).toHaveBeenCalledWith('/api/traces')
  })

  it('list() throws on non-OK response', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('nope', { status: 500 })),
    )
    const src = new HttpTraceSource('')
    await expect(src.list()).rejects.toThrow(/500/)
  })

  it('load() fetches /api/trace/<doc>', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(
        JSON.stringify({
          schema_version: 1,
          render: {},
          pipeline: [{ stage: 'parse', index: 0 }],
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      ),
    )
    vi.stubGlobal('fetch', fetchMock)
    const src = new HttpTraceSource('')
    const doc = await src.load('hello')
    expect(doc.schema_version).toBe(1)
    expect(doc.pipeline[0].stage).toBe('parse')
    expect(fetchMock).toHaveBeenCalledWith('/api/trace/hello')
  })

  it('load() URL-encodes doc stems with special characters', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(
        JSON.stringify({ schema_version: 1, render: {}, pipeline: [] }),
        { status: 200 },
      ),
    )
    vi.stubGlobal('fetch', fetchMock)
    const src = new HttpTraceSource('')
    await src.load('my doc/with slash')
    expect(fetchMock).toHaveBeenCalledWith(
      '/api/trace/my%20doc%2Fwith%20slash',
    )
  })

  it('base URL is applied to requests', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ traces: [] }), { status: 200 }),
    )
    vi.stubGlobal('fetch', fetchMock)
    const src = new HttpTraceSource('http://localhost:4180/')
    await src.list()
    expect(fetchMock).toHaveBeenCalledWith('http://localhost:4180/api/traces')
  })
})
