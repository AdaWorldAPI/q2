import { useCallback, useEffect, useRef } from 'react';
import type { FileEntry } from '@quarto/preview-renderer/types/project';
import { InMemorySearchProvider } from './inMemorySearchProvider';
import type { SearchOptions, SearchResult } from './types';

/** Async search function handed to the UI. */
export type SearchFiles = (query: string, opts?: SearchOptions) => Promise<SearchResult[]>;

/**
 * Owns a Phase 1 in-memory {@link InMemorySearchProvider} and keeps it
 * reconciled with the currently-open project.
 *
 * The index is maintained as a pure function of two pieces of React state the
 * hub already tracks:
 *
 * - `files` — the authoritative file list (membership; controls deletions and
 *   project switches), and
 * - `fileContents` — the live text of each file (only text files appear here;
 *   binaries are excluded upstream).
 *
 * A file is indexed iff it is in **both** (so a path lingering in
 * `fileContents` after deletion, or a binary with no text, is never indexed).
 * Driving from state — rather than from raw sync callbacks — means project
 * switches (both inputs reset to empty) and deletions are reflected for free,
 * with no risk of the index drifting from what the user sees.
 *
 * Returns a stable async `search` function. Swapping this hook for a
 * server-backed one is how Phase 2 (cross-project search) lands without
 * touching the UI.
 */
export function useProjectSearch(
  files: FileEntry[],
  fileContents: Map<string, string>
): SearchFiles {
  const providerRef = useRef<InMemorySearchProvider | null>(null);
  if (providerRef.current === null) {
    providerRef.current = new InMemorySearchProvider();
  }
  // Shadow of what is currently indexed (path -> indexed content), so we only
  // touch the index on real deltas.
  const indexedRef = useRef<Map<string, string>>(new Map());

  useEffect(() => {
    const provider = providerRef.current!;
    const indexed = indexedRef.current;
    const listed = new Set(files.map((f) => f.path));

    // Desired corpus: text content for paths that are also listed files.
    const desired = new Map<string, string>();
    for (const [path, content] of fileContents) {
      if (listed.has(path)) {
        desired.set(path, content);
      }
    }

    // Adds and updates.
    for (const [path, content] of desired) {
      if (indexed.get(path) !== content) {
        provider.addOrUpdate(path, content);
        indexed.set(path, content);
      }
    }

    // Removals (files deleted, unlisted, or whose content disappeared).
    for (const path of [...indexed.keys()]) {
      if (!desired.has(path)) {
        provider.remove(path);
        indexed.delete(path);
      }
    }
  }, [files, fileContents]);

  return useCallback<SearchFiles>(
    (query, opts) => providerRef.current!.search(query, opts),
    []
  );
}
