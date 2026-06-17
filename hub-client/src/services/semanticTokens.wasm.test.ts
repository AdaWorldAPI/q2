/**
 * WASM tests for the semantic-tokens path — Phase 4 of
 * `claude-notes/plans/2026-06-10-monaco-tree-sitter-highlighting.md`.
 *
 * Drives the real `lsp_get_semantic_tokens` / `lsp_get_token_legend` exports
 * over the VFS (the same round-trip Monaco's DocumentSemanticTokensProvider
 * makes), plus the legend drift guard that keeps the checked-in TS legend
 * honest to the Rust source of truth.
 *
 * Run with: `npm run test:wasm`
 */

import { describe, it, expect, beforeAll, beforeEach } from 'vitest';
import { readFile } from 'fs/promises';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { initWasm, vfsAddFile, vfsClear } from '@quarto/preview-runtime';
import { QMD_TOKEN_LEGEND } from './intelligenceService';

const __dirname = dirname(fileURLToPath(import.meta.url));

interface SemanticToken {
  line: number;
  character: number;
  length: number;
  tokenType: number;
  modifiers: number;
}
interface SemanticTokensResponse {
  success: boolean;
  error?: string;
  tokens?: SemanticToken[];
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
let wasm: any;

beforeAll(async () => {
  const wasmDir = join(__dirname, '../../wasm-quarto-hub-client');
  const wasmBytes = await readFile(join(wasmDir, 'wasm_quarto_hub_client_bg.wasm'));
  wasm = await import('wasm-quarto-hub-client');
  await wasm.default(wasmBytes);
  await initWasm();
});

beforeEach(() => {
  vfsClear();
});

const PATH = '/project/test.qmd';

function tokensFor(content: string): SemanticTokensResponse {
  vfsAddFile(PATH, content);
  return JSON.parse(wasm.lsp_get_semantic_tokens(PATH));
}

/** Resolve a token's legend name for readable assertions. */
function typeName(t: SemanticToken): string {
  return QMD_TOKEN_LEGEND[t.tokenType];
}

describe('lsp_get_semantic_tokens', () => {
  it('produces link tokens for a [label](url) snippet', () => {
    const result = tokensFor('[label](https://example.com)\n');
    expect(result.success, result.error).toBe(true);
    const names = (result.tokens ?? []).map(typeName);
    expect(names).toContain('qmd.markup.link.label');
    expect(names).toContain('qmd.markup.link.url');
  });

  it('produces code tokens inside an {r} cell', () => {
    const result = tokensFor('```{r}\nx <- 1\n```\n');
    expect(result.success, result.error).toBe(true);
    const tokens = result.tokens ?? [];
    // The cell body is on line 1; it should carry code-legend types.
    const codeOnBody = tokens.filter(
      (t) => t.line === 1 && typeName(t).startsWith('qmd.code.'),
    );
    expect(codeOnBody.length).toBeGreaterThan(0);
  });

  it('produces a comment token for an HTML comment', () => {
    const result = tokensFor('Text <!-- hidden --> more\n');
    expect(result.success, result.error).toBe(true);
    const names = (result.tokens ?? []).map(typeName);
    expect(names).toContain('qmd.markup.comment');
  });

  it('produces a comment token for an editorial `[>> ...]` comment', () => {
    const result = tokensFor('Para [>> editorial note]\n');
    expect(result.success, result.error).toBe(true);
    const names = (result.tokens ?? []).map(typeName);
    expect(names).toContain('qmd.markup.comment');
  });

  it('returns an empty token list for an empty document', () => {
    const result = tokensFor('');
    expect(result.success).toBe(true);
    expect(result.tokens).toEqual([]);
  });

  it('returns a failure envelope for a missing file', () => {
    vfsClear();
    const result: SemanticTokensResponse = JSON.parse(
      wasm.lsp_get_semantic_tokens('/project/does-not-exist.qmd'),
    );
    expect(result.success).toBe(false);
    expect(typeof result.error).toBe('string');
  });
});

describe('lsp_get_token_legend (drift guard)', () => {
  it('the checked-in TS legend deep-equals the Rust source of truth', () => {
    const rustLegend: string[] = JSON.parse(wasm.lsp_get_token_legend());
    expect([...QMD_TOKEN_LEGEND]).toEqual(rustLegend);
  });
});
