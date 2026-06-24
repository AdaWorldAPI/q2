/**
 * @vitest-environment jsdom
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react';
import type { FileEntry } from '@quarto/preview-renderer/types/project';
import FileSidebar from './FileSidebar';
import type { SearchResult } from '../services/search';

afterEach(cleanup);

function entry(path: string): FileEntry {
  return { path, docId: `doc-${path}` } as FileEntry;
}

const baseProps = {
  currentFile: null,
  onNewFile: () => {},
  onUploadFiles: () => {},
};

describe('FileSidebar full-text search', () => {
  it('does not render a search box when searchFiles is not provided', () => {
    render(
      <FileSidebar files={[entry('a.qmd')]} onSelectFile={() => {}} {...baseProps} />
    );
    expect(screen.queryByLabelText('Search files')).toBeNull();
  });

  it('runs a query and renders ranked results with snippets', async () => {
    const searchFiles = vi.fn(
      async (): Promise<SearchResult[]> => [
        { path: 'intro.qmd', score: 2, terms: ['search'] },
      ]
    );
    const fileContents = new Map([['intro.qmd', 'a document about search engines']]);

    render(
      <FileSidebar
        files={[entry('intro.qmd'), entry('other.qmd')]}
        onSelectFile={() => {}}
        searchFiles={searchFiles}
        fileContents={fileContents}
        {...baseProps}
      />
    );

    fireEvent.change(screen.getByLabelText('Search files'), {
      target: { value: 'search' },
    });

    await waitFor(() => expect(searchFiles).toHaveBeenCalledWith('search', expect.anything()));
    expect(await screen.findByText('intro.qmd')).toBeTruthy();
    // Snippet highlights the matched term in a <mark>.
    const mark = document.querySelector('.search-result-snippet mark');
    expect(mark?.textContent).toBe('search');
  });

  it('selects the right file when a result is clicked', async () => {
    const onSelectFile = vi.fn();
    const searchFiles = vi.fn(
      async (): Promise<SearchResult[]> => [{ path: 'intro.qmd', score: 1, terms: ['intro'] }]
    );

    render(
      <FileSidebar
        files={[entry('intro.qmd')]}
        onSelectFile={onSelectFile}
        searchFiles={searchFiles}
        {...baseProps}
      />
    );

    fireEvent.change(screen.getByLabelText('Search files'), {
      target: { value: 'intro' },
    });

    const result = await screen.findByText('intro.qmd');
    fireEvent.click(result);
    expect(onSelectFile).toHaveBeenCalledWith(expect.objectContaining({ path: 'intro.qmd' }));
  });

  it('shows a no-matches state when the query returns nothing', async () => {
    const searchFiles = vi.fn(async (): Promise<SearchResult[]> => []);
    render(
      <FileSidebar
        files={[entry('a.qmd')]}
        onSelectFile={() => {}}
        searchFiles={searchFiles}
        {...baseProps}
      />
    );

    fireEvent.change(screen.getByLabelText('Search files'), {
      target: { value: 'zzz' },
    });

    expect(await screen.findByText('No matches')).toBeTruthy();
  });

  it('clears the query with the clear button, restoring the file tree', async () => {
    const searchFiles = vi.fn(async (): Promise<SearchResult[]> => []);
    render(
      <FileSidebar
        files={[entry('tree-file.qmd')]}
        onSelectFile={() => {}}
        searchFiles={searchFiles}
        {...baseProps}
      />
    );

    const input = screen.getByLabelText('Search files') as HTMLInputElement;
    fireEvent.change(input, { target: { value: 'zzz' } });
    await screen.findByText('No matches');

    fireEvent.click(screen.getByLabelText('Clear search'));
    expect(input.value).toBe('');
    // Tree is back: the file name is shown again.
    expect(screen.getByText('tree-file.qmd')).toBeTruthy();
  });
});
