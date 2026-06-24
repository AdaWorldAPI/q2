import MiniSearch from 'minisearch';
import type { SearchOptions, SearchProvider, SearchResult } from './types';

/**
 * Internal shape of an indexed document. `id` is the file path, which is also
 * indexed as a field so filename terms are searchable.
 */
interface IndexedDoc {
  id: string;
  path: string;
  text: string;
}

/**
 * Phase 1 {@link SearchProvider}: a MiniSearch-backed inverted index held
 * entirely in browser memory over the currently-open project's files.
 *
 * The corpus is maintained incrementally so it can be driven straight from the
 * hub's sync callbacks: {@link addOrUpdate} on `onFileContent`, {@link remove}
 * on a file deletion, {@link clear} on project switch.
 */
export class InMemorySearchProvider implements SearchProvider {
  private index: MiniSearch<IndexedDoc>;

  constructor() {
    this.index = InMemorySearchProvider.createIndex();
  }

  private static createIndex(): MiniSearch<IndexedDoc> {
    return new MiniSearch<IndexedDoc>({
      fields: ['text', 'path'],
      storeFields: ['path'],
      // Default tokenizer splits on whitespace and punctuation, so a path like
      // "chapters/methodology.qmd" yields ["chapters", "methodology", "qmd"].
    });
  }

  addOrUpdate(path: string, text: string): void {
    const doc: IndexedDoc = { id: path, path, text };
    if (this.index.has(path)) {
      this.index.replace(doc);
    } else {
      this.index.add(doc);
    }
  }

  remove(path: string): void {
    // discard() removes by id without needing the original document, and is a
    // no-op-safe only when the id exists — guard to keep remove() idempotent.
    if (this.index.has(path)) {
      this.index.discard(path);
    }
  }

  clear(): void {
    this.index.removeAll();
  }

  async search(query: string, opts?: SearchOptions): Promise<SearchResult[]> {
    if (query.trim() === '') {
      return [];
    }

    const raw = this.index.search(query, {
      prefix: true,
      fuzzy: 0.2,
      // Filename hits are a strong relevance signal.
      boost: { path: 2 },
    });

    const limited = opts?.limit != null ? raw.slice(0, opts.limit) : raw;

    return limited.map((r) => ({
      path: r.id as string,
      score: r.score,
      terms: r.terms,
      // title / projectId intentionally absent in Phase 1 (raw-text,
      // single-project index). Populated by later phases.
    }));
  }
}
