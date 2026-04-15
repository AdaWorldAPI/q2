/**
 * TraceSource abstraction — how the viewer discovers and loads trace
 * documents. Two implementations will exist:
 *
 *   - `HttpTraceSource` (this file): talks to the local `quarto trace view`
 *     server's `/api/traces` and `/api/trace/<doc>` routes.
 *   - `VfsTraceSource` (Phase 4.4): reads from hub-client's Automerge-backed
 *     virtual filesystem.
 *
 * UI components accept a `TraceSource` as a prop and stay identical across
 * targets.
 */

import type { TraceDocument, TraceListing } from './types'

export interface TraceSource {
  /** List all available traces. */
  list(): Promise<TraceListing[]>
  /** Load one trace by doc stem. */
  load(doc: string): Promise<TraceDocument>
}

/** HTTP-backed trace source for the native `quarto trace view` server. */
export class HttpTraceSource implements TraceSource {
  /** Base URL (defaults to same-origin). */
  readonly base: string

  constructor(base: string = '') {
    this.base = base.replace(/\/+$/, '')
  }

  async list(): Promise<TraceListing[]> {
    const res = await fetch(`${this.base}/api/traces`)
    if (!res.ok) {
      throw new Error(`GET /api/traces failed: ${res.status} ${res.statusText}`)
    }
    const body = (await res.json()) as { traces?: TraceListing[] }
    return body.traces ?? []
  }

  async load(doc: string): Promise<TraceDocument> {
    const res = await fetch(`${this.base}/api/trace/${encodeURIComponent(doc)}`)
    if (!res.ok) {
      throw new Error(
        `GET /api/trace/${doc} failed: ${res.status} ${res.statusText}`,
      )
    }
    return (await res.json()) as TraceDocument
  }
}
