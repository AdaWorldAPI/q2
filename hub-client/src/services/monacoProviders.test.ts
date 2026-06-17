/**
 * Phase 6 unit tests: the semantic-tokens delta-encoder and the provider's
 * cancel / stale / empty return-shape contract. Pure — no Monaco runtime, no
 * real WASM (the intelligence service is mocked).
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import type * as Monaco from 'monaco-editor';

// Mock the intelligence service so the provider's WASM call is controllable.
const getSemanticTokensForContentMock = vi.fn();
vi.mock('./intelligenceService', () => ({
  getSemanticTokensForContent: (...args: unknown[]) =>
    getSemanticTokensForContentMock(...args),
  getSymbols: vi.fn(async () => []),
  getFoldingRanges: vi.fn(async () => []),
  QMD_TOKEN_LEGEND: ['qmd.markup.heading', 'qmd.code.keyword'],
}));

import {
  encodeSemanticTokens,
  registerIntelligenceProviders,
  disposeIntelligenceProviders,
} from './monacoProviders';

describe('encodeSemanticTokens', () => {
  it('delta-encodes tokens into Monaco 5-tuples', () => {
    const data = encodeSemanticTokens([
      { line: 0, character: 0, length: 1, tokenType: 3, modifiers: 0 },
      { line: 0, character: 5, length: 4, tokenType: 7, modifiers: 0 },
      { line: 2, character: 2, length: 3, tokenType: 1, modifiers: 0 },
    ]);
    expect(Array.from(data)).toEqual([
      // deltaLine, deltaStartChar, length, type, mods
      0, 0, 1, 3, 0, // first token: absolute
      0, 5, 4, 7, 0, // same line: deltaChar = 5 - 0
      2, 2, 3, 1, 0, // line +2: deltaChar absolute again
    ]);
  });

  it('encodes an empty token list to an empty array', () => {
    expect(encodeSemanticTokens([]).length).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// Provider return-shape contract
// ---------------------------------------------------------------------------

type SemanticProvider = Monaco.languages.DocumentSemanticTokensProvider;

/** Capture the semantic-tokens provider registered with a stub Monaco. */
function captureProvider(currentPath: string | null): SemanticProvider {
  let captured: SemanticProvider | null = null;
  const disposable = { dispose: () => {} };
  const monaco = {
    languages: {
      registerDocumentSymbolProvider: () => disposable,
      registerFoldingRangeProvider: () => disposable,
      registerDocumentSemanticTokensProvider: (_lang: string, provider: SemanticProvider) => {
        captured = provider;
        return disposable;
      },
    },
  } as unknown as typeof Monaco;

  registerIntelligenceProviders(monaco, () => currentPath);
  if (!captured) throw new Error('semantic provider was not registered');
  return captured;
}

function stubToken(cancelled: boolean): Monaco.CancellationToken {
  return {
    isCancellationRequested: cancelled,
    onCancellationRequested: () => ({ dispose: () => {} }),
  } as unknown as Monaco.CancellationToken;
}

function stubModel(versionIds: number[], value = 'model text'): Monaco.editor.ITextModel {
  let i = 0;
  return {
    getVersionId: () => versionIds[Math.min(i++, versionIds.length - 1)],
    getValue: () => value,
  } as unknown as Monaco.editor.ITextModel;
}

describe('provideDocumentSemanticTokens return shape', () => {
  beforeEach(() => {
    getSemanticTokensForContentMock.mockReset();
    disposeIntelligenceProviders();
  });

  it('tokenises the model content, not the VFS image by path', async () => {
    // Regression: tokens were computed from the VFS image while Monaco renders
    // the model. When the two drift, every token shifts and smears colours onto
    // adjacent characters (e.g. an image opener token landing on a link's `[`).
    getSemanticTokensForContentMock.mockResolvedValue([]);
    const provider = captureProvider('/project/a.qmd');
    await provider.provideDocumentSemanticTokens(
      stubModel([1], '[label](url)'),
      null,
      stubToken(false),
    );
    expect(getSemanticTokensForContentMock).toHaveBeenCalledWith(
      '/project/a.qmd',
      '[label](url)',
    );
  });

  it('returns null when the request is already cancelled', async () => {
    getSemanticTokensForContentMock.mockResolvedValue([]);
    const provider = captureProvider('/project/a.qmd');
    const result = await provider.provideDocumentSemanticTokens(
      stubModel([1]),
      null,
      stubToken(true),
    );
    expect(result).toBeNull();
  });

  it('returns null when the model version changes across the await', async () => {
    getSemanticTokensForContentMock.mockResolvedValue([
      { line: 0, character: 0, length: 1, tokenType: 0, modifiers: 0 },
    ]);
    const provider = captureProvider('/project/a.qmd');
    // versionId 1 before the await, 2 after → stale.
    const result = await provider.provideDocumentSemanticTokens(
      stubModel([1, 2]),
      null,
      stubToken(false),
    );
    expect(result).toBeNull();
  });

  it('returns empty Uint32Array (not null) for a valid empty result', async () => {
    getSemanticTokensForContentMock.mockResolvedValue([]);
    const provider = captureProvider('/project/a.qmd');
    const result = await provider.provideDocumentSemanticTokens(
      stubModel([1]),
      null,
      stubToken(false),
    );
    expect(result).not.toBeNull();
    const tokens = result as Monaco.languages.SemanticTokens;
    expect(tokens.data).toBeInstanceOf(Uint32Array);
    expect(tokens.data.length).toBe(0);
    expect(tokens.resultId).toBeUndefined();
  });

  it('delta-encodes a non-empty result', async () => {
    getSemanticTokensForContentMock.mockResolvedValue([
      { line: 1, character: 2, length: 3, tokenType: 1, modifiers: 0 },
    ]);
    const provider = captureProvider('/project/a.qmd');
    const result = (await provider.provideDocumentSemanticTokens(
      stubModel([1]),
      null,
      stubToken(false),
    )) as Monaco.languages.SemanticTokens;
    expect(Array.from(result.data)).toEqual([1, 2, 3, 1, 0]);
  });

  it('returns null when there is no current file path', async () => {
    const provider = captureProvider(null);
    const result = await provider.provideDocumentSemanticTokens(
      stubModel([1]),
      null,
      stubToken(false),
    );
    expect(result).toBeNull();
    expect(getSemanticTokensForContentMock).not.toHaveBeenCalled();
  });
});
