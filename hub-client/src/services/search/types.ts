/**
 * Full-text search abstraction for the Quarto Hub client.
 *
 * The {@link SearchProvider} interface is the single seam that lets Phase 1
 * (an in-memory index over the currently-open project) evolve into Phase 2
 * (a server-backed index spanning a *set* of projects) without changing the
 * UI. The UI must never assume the corpus lives entirely in browser memory:
 * {@link SearchProvider.search} is therefore async, and {@link SearchResult}
 * already carries an optional {@link SearchResult.projectId} so cross-project
 * results render unchanged.
 *
 * See claude-notes/plans/2026-06-23-hub-client-full-text-search.md.
 */

/** Options refining a search query. */
export interface SearchOptions {
  /** Cap on the number of results returned (best-scoring first). */
  limit?: number;
}

/** A single match returned from {@link SearchProvider.search}. */
export interface SearchResult {
  /** File path within its project, e.g. "docs/intro.qmd". */
  path: string;
  /** Relevance score; higher is better. Comparable only within one result set. */
  score: number;
  /**
   * Query terms that actually matched this document (after prefix/fuzzy
   * expansion). Lets the UI highlight matches / build snippets without
   * re-running the matcher.
   */
  terms: string[];
  /**
   * Document title, when known (populated in Phase B from DocumentProfile).
   * Absent in Phase 1's raw-text index.
   */
  title?: string;
  /**
   * Identifies the owning project (the project's index-doc id). `undefined`
   * means "the single currently-open project" — the Phase 1 case. Phase 2's
   * cross-project provider populates this so results from different projects
   * are distinguishable.
   */
  projectId?: string;
}

/**
 * A full-text search index over a corpus of text files.
 *
 * Implementations maintain the corpus incrementally via
 * {@link addOrUpdate}/{@link remove}/{@link clear}, which are designed to be
 * driven directly from the hub's sync callbacks (`onFileContent`,
 * `onFilesChange`).
 */
export interface SearchProvider {
  /**
   * Insert a file, or replace it if a file with the same `path` is already
   * indexed. Idempotent for unchanged content.
   */
  addOrUpdate(path: string, text: string): void;

  /** Remove a file from the index. No-op if the path is not indexed. */
  remove(path: string): void;

  /** Drop the entire corpus (e.g. on project switch or disconnect). */
  clear(): void;

  /**
   * Search the corpus. Returns best-scoring results first. An empty or
   * whitespace-only query returns no results. Async to keep the interface
   * stable across the eventual server-backed (Phase 2) implementation.
   */
  search(query: string, opts?: SearchOptions): Promise<SearchResult[]>;
}
