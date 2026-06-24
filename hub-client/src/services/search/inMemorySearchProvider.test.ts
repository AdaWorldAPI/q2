import { describe, it, expect, beforeEach } from 'vitest';
import { InMemorySearchProvider } from './inMemorySearchProvider';

describe('InMemorySearchProvider', () => {
  let provider: InMemorySearchProvider;

  beforeEach(() => {
    provider = new InMemorySearchProvider();
  });

  describe('addOrUpdate + search', () => {
    it('returns a document whose content matches the query', async () => {
      provider.addOrUpdate('intro.qmd', 'The quick brown fox jumps');
      const results = await provider.search('fox');
      expect(results.map((r) => r.path)).toEqual(['intro.qmd']);
    });

    it('returns multiple matching documents', async () => {
      provider.addOrUpdate('a.qmd', 'apples and oranges');
      provider.addOrUpdate('b.qmd', 'oranges and lemons');
      provider.addOrUpdate('c.qmd', 'completely unrelated');
      const paths = (await provider.search('oranges')).map((r) => r.path).sort();
      expect(paths).toEqual(['a.qmd', 'b.qmd']);
    });

    it('ranks a document with more occurrences of the term higher', async () => {
      provider.addOrUpdate('dense.qmd', 'search search search search results');
      provider.addOrUpdate('sparse.qmd', 'a single search term among many other words here');
      const results = await provider.search('search');
      expect(results[0].path).toBe('dense.qmd');
    });

    it('matches on the file path as well as content', async () => {
      provider.addOrUpdate('chapters/methodology.qmd', 'body text without the term');
      const results = await provider.search('methodology');
      expect(results.map((r) => r.path)).toContain('chapters/methodology.qmd');
    });

    it('populates matched terms for highlighting', async () => {
      provider.addOrUpdate('intro.qmd', 'collaborative editing is collaborative');
      const results = await provider.search('collaborative');
      expect(results[0].terms).toContain('collaborative');
    });

    it('returns scores in non-increasing order', async () => {
      provider.addOrUpdate('a.qmd', 'token token token');
      provider.addOrUpdate('b.qmd', 'token once');
      const scores = (await provider.search('token')).map((r) => r.score);
      const sorted = [...scores].sort((x, y) => y - x);
      expect(scores).toEqual(sorted);
    });
  });

  describe('prefix and fuzzy matching', () => {
    it('matches on a term prefix', async () => {
      provider.addOrUpdate('doc.qmd', 'documentation generator');
      const results = await provider.search('docum');
      expect(results.map((r) => r.path)).toContain('doc.qmd');
    });

    it('tolerates a small typo (fuzzy)', async () => {
      provider.addOrUpdate('doc.qmd', 'collaborative editing');
      const results = await provider.search('colaborative');
      expect(results.map((r) => r.path)).toContain('doc.qmd');
    });
  });

  describe('update semantics', () => {
    it('replaces content on re-add without duplicating the document', async () => {
      provider.addOrUpdate('f.qmd', 'original content alpha');
      provider.addOrUpdate('f.qmd', 'revised content beta');

      // Old term no longer matches.
      expect(await provider.search('alpha')).toEqual([]);
      // New term matches, exactly once.
      const beta = await provider.search('beta');
      expect(beta.map((r) => r.path)).toEqual(['f.qmd']);
    });
  });

  describe('remove', () => {
    it('drops a document from results', async () => {
      provider.addOrUpdate('f.qmd', 'removable content');
      provider.remove('f.qmd');
      expect(await provider.search('removable')).toEqual([]);
    });

    it('is a no-op for an unknown path', async () => {
      provider.addOrUpdate('keep.qmd', 'keep this content');
      expect(() => provider.remove('never-added.qmd')).not.toThrow();
      expect((await provider.search('keep')).map((r) => r.path)).toEqual(['keep.qmd']);
    });
  });

  describe('clear', () => {
    it('empties the corpus', async () => {
      provider.addOrUpdate('a.qmd', 'alpha');
      provider.addOrUpdate('b.qmd', 'beta');
      provider.clear();
      expect(await provider.search('alpha')).toEqual([]);
      expect(await provider.search('beta')).toEqual([]);
    });

    it('allows re-indexing after clear', async () => {
      provider.addOrUpdate('a.qmd', 'alpha');
      provider.clear();
      provider.addOrUpdate('a.qmd', 'alpha again');
      expect((await provider.search('alpha')).map((r) => r.path)).toEqual(['a.qmd']);
    });
  });

  describe('empty queries', () => {
    it('returns nothing for an empty query', async () => {
      provider.addOrUpdate('a.qmd', 'some content');
      expect(await provider.search('')).toEqual([]);
    });

    it('returns nothing for a whitespace-only query', async () => {
      provider.addOrUpdate('a.qmd', 'some content');
      expect(await provider.search('   \t  ')).toEqual([]);
    });
  });

  describe('limit option', () => {
    it('caps the number of results', async () => {
      for (let i = 0; i < 10; i++) {
        provider.addOrUpdate(`f${i}.qmd`, 'shared term here');
      }
      const results = await provider.search('shared', { limit: 3 });
      expect(results).toHaveLength(3);
    });
  });

  describe('Phase 2 forward-compatibility', () => {
    it('leaves projectId undefined for the single-project (Phase 1) case', async () => {
      provider.addOrUpdate('a.qmd', 'content');
      const [result] = await provider.search('content');
      expect(result.projectId).toBeUndefined();
    });

    it('leaves title undefined for the raw-text (Phase 1) index', async () => {
      provider.addOrUpdate('a.qmd', 'content');
      const [result] = await provider.search('content');
      expect(result.title).toBeUndefined();
    });
  });
});
