/**
 * TypeScript mirror of the `quarto-trace` schema (`quarto-trace/src/lib.rs`).
 *
 * The canonical definition is the Rust crate; keep these types in sync when
 * the schema evolves. Readers are lenient about unknown fields for forward
 * compatibility.
 */

export const SCHEMA_VERSION = 1

export type StageStatus = 'ok' | 'error' | 'skipped' | 'unknown' | string

export interface RenderInfo {
  input_path?: string
  output_path?: string
  format_target?: string
  started_at_unix_ms?: number
  git_hash?: string
  total_duration_ms?: number
}

export interface StageErrorInfo {
  message: string
}

export interface TraceEntry {
  stage: string
  index: number
  data_kind?: string
  data?: unknown
  duration_ms?: number
  status?: StageStatus
  error?: StageErrorInfo
}

export interface TraceDocument {
  schema_version: number
  render: RenderInfo
  pipeline: TraceEntry[]
}

/** One item returned by `TraceSource.list()`. */
export interface TraceListing {
  doc: string
  path?: string
}

/**
 * Known kinds of pipeline data. The trace JSON may carry any string here;
 * this type captures only the known values for exhaustive switching.
 */
export type KnownDataKind =
  | 'LoadedSource'
  | 'DocumentSource'
  | 'DocumentAst'
  | 'ExecutedDocument'
  | 'RenderedOutput'
  | 'FinalOutput'
