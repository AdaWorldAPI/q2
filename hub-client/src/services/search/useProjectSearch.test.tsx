/**
 * @vitest-environment jsdom
 */

import { describe, it, expect } from 'vitest';
import { renderHook } from '@testing-library/react';
import type { FileEntry } from '@quarto/preview-renderer/types/project';
import { useProjectSearch } from './useProjectSearch';

function entry(path: string): FileEntry {
  return { path, docId: `doc-${path}` } as FileEntry;
}

function filesOf(...paths: string[]): FileEntry[] {
  return paths.map(entry);
}

describe('useProjectSearch', () => {
  it('indexes files present in both the file list and the contents map', async () => {
    const files = filesOf('a.qmd', 'b.qmd');
    const contents = new Map([
      ['a.qmd', 'alpha content'],
      ['b.qmd', 'beta content'],
    ]);
    const { result } = renderHook(() => useProjectSearch(files, contents));

    expect((await result.current('alpha')).map((r) => r.path)).toEqual(['a.qmd']);
    expect((await result.current('beta')).map((r) => r.path)).toEqual(['b.qmd']);
  });

  it('does not index content for a path absent from the file list (stale/binary guard)', async () => {
    const files = filesOf('a.qmd'); // b.qmd is NOT a listed file
    const contents = new Map([
      ['a.qmd', 'alpha content'],
      ['b.qmd', 'beta content'],
    ]);
    const { result } = renderHook(() => useProjectSearch(files, contents));

    expect(await result.current('beta')).toEqual([]);
    expect((await result.current('alpha')).map((r) => r.path)).toEqual(['a.qmd']);
  });

  it('re-indexes when a file’s content changes', async () => {
    const files = filesOf('a.qmd');
    const { result, rerender } = renderHook(
      ({ contents }: { contents: Map<string, string> }) => useProjectSearch(files, contents),
      { initialProps: { contents: new Map([['a.qmd', 'original alpha']]) } }
    );

    expect((await result.current('alpha')).map((r) => r.path)).toEqual(['a.qmd']);

    rerender({ contents: new Map([['a.qmd', 'revised omega']]) });

    expect(await result.current('alpha')).toEqual([]);
    expect((await result.current('omega')).map((r) => r.path)).toEqual(['a.qmd']);
  });

  it('removes a file from the index when it leaves the file list', async () => {
    const { result, rerender } = renderHook(
      ({ files }: { files: FileEntry[] }) =>
        useProjectSearch(
          files,
          new Map([
            ['a.qmd', 'alpha content'],
            ['b.qmd', 'beta content'],
          ])
        ),
      { initialProps: { files: filesOf('a.qmd', 'b.qmd') } }
    );

    expect((await result.current('beta')).map((r) => r.path)).toEqual(['b.qmd']);

    rerender({ files: filesOf('a.qmd') }); // b.qmd deleted

    expect(await result.current('beta')).toEqual([]);
    expect((await result.current('alpha')).map((r) => r.path)).toEqual(['a.qmd']);
  });

  it('clears the index when the project is switched (inputs reset to empty)', async () => {
    const { result, rerender } = renderHook(
      ({ files, contents }: { files: FileEntry[]; contents: Map<string, string> }) =>
        useProjectSearch(files, contents),
      {
        initialProps: {
          files: filesOf('a.qmd'),
          contents: new Map([['a.qmd', 'alpha content']]),
        },
      }
    );

    expect((await result.current('alpha')).map((r) => r.path)).toEqual(['a.qmd']);

    rerender({ files: [], contents: new Map() });

    expect(await result.current('alpha')).toEqual([]);
  });

  it('returns a stable search function across re-renders', () => {
    const files = filesOf('a.qmd');
    const { result, rerender } = renderHook(
      ({ contents }: { contents: Map<string, string> }) => useProjectSearch(files, contents),
      { initialProps: { contents: new Map([['a.qmd', 'alpha']]) } }
    );
    const first = result.current;
    rerender({ contents: new Map([['a.qmd', 'alpha revised']]) });
    expect(result.current).toBe(first);
  });
});
