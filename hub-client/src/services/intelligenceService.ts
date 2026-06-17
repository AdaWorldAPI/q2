/**
 * Intelligence Service
 *
 * Provides typed access to the wasm-quarto-hub-client LSP intelligence
 * functions for document analysis (symbols, folding ranges, diagnostics).
 *
 * This service reads from the VFS (populated by automerge sync) and uses
 * quarto-lsp-core's analyze_document() to parse documents once and extract
 * all intelligence data.
 */

import type {
  Symbol,
  Diagnostic,
  FoldingRange,
  DocumentAnalysis,
  SemanticToken,
  LspAnalyzeResponse,
  LspSymbolsResponse,
  LspFoldingRangesResponse,
  LspDiagnosticsResponse,
  LspSemanticTokensResponse,
} from '@quarto/preview-renderer/types/intelligence';
import { initWasm } from '@quarto/preview-runtime';
import { isQmdFile } from '@quarto/preview-renderer/types/project';

// Re-export types for convenience
export type { Symbol, Diagnostic, FoldingRange, DocumentAnalysis, SemanticToken } from '@quarto/preview-renderer/types/intelligence';

/**
 * The Monaco semantic-token legend: token-type names, indexed by a token's
 * `tokenType`. Theme rules (quarto-{light,dark}) target these names.
 *
 * **The Rust `quarto_lsp_core::QMD_TOKEN_LEGEND` is the source of truth.** This
 * is a checked-in mirror so the semantic-tokens provider can supply the legend
 * synchronously at registration (the WASM module is initialised lazily, after
 * the provider registers, so a sync WASM legend call there would throw). The
 * `semanticTokens.wasm.test.ts` drift guard asserts this deep-equals
 * `JSON.parse(lsp_get_token_legend())`; if it ever fails, copy the Rust array.
 */
export const QMD_TOKEN_LEGEND: readonly string[] = [
  // structural (qmd highlights.scm)
  'qmd.markup.heading',
  'qmd.markup.emphasis',
  'qmd.markup.strong',
  'qmd.markup.strikethrough',
  'qmd.markup.link.label',
  'qmd.markup.link.url',
  'qmd.markup.link.title',
  'qmd.markup.image.label',
  'qmd.markup.image.url',
  'qmd.markup.raw.inline',
  'qmd.markup.raw',
  'qmd.markup.raw.info',
  'qmd.markup.math',
  'qmd.markup.shortcode',
  'qmd.markup.list',
  'qmd.markup.quote',
  'qmd.markup.comment',
  'qmd.punctuation.special',
  'qmd.punctuation.special.image',
  'qmd.punctuation.bracket',
  'qmd.punctuation.delimiter.fence',
  'qmd.punctuation.delimiter.frontmatter',
  'qmd.attribute.specifier',
  // embedded code (the 24 hl-* roots in highlight.scss)
  'qmd.code.attribute',
  'qmd.code.boolean',
  'qmd.code.character',
  'qmd.code.comment',
  'qmd.code.constant',
  'qmd.code.constructor',
  'qmd.code.embedded',
  'qmd.code.error',
  'qmd.code.escape',
  'qmd.code.function',
  'qmd.code.keyword',
  'qmd.code.label',
  'qmd.code.markup',
  'qmd.code.module',
  'qmd.code.namespace',
  'qmd.code.number',
  'qmd.code.operator',
  'qmd.code.property',
  'qmd.code.punctuation',
  'qmd.code.special',
  'qmd.code.string',
  'qmd.code.tag',
  'qmd.code.type',
  'qmd.code.variable',
];

// ============================================================================
// Internal Helpers
// ============================================================================

/**
 * Get the WASM module, ensuring it's initialized.
 */
async function getWasm(): Promise<typeof import('wasm-quarto-hub-client')> {
  await initWasm();
  // Dynamic import to get the module
  const wasm = await import('wasm-quarto-hub-client');
  return wasm;
}

// ============================================================================
// Analysis Functions
// ============================================================================

/**
 * Analyze a document, returning all intelligence data in one call.
 *
 * This is the most efficient way to get multiple pieces of data,
 * as it performs only one parse.
 *
 * @param path - File path in VFS (e.g., "index.qmd")
 * @returns Complete analysis (symbols, folding ranges, diagnostics)
 */
export async function analyzeDocument(path: string): Promise<DocumentAnalysis> {
  // Only QMD files have intelligence data
  if (!isQmdFile(path)) {
    return { symbols: [], foldingRanges: [], diagnostics: [] };
  }

  const wasm = await getWasm();
  const result: LspAnalyzeResponse = JSON.parse(wasm.lsp_analyze_document(path));

  if (result.success) {
    return {
      symbols: result.symbols ?? [],
      foldingRanges: result.foldingRanges ?? [],
      diagnostics: result.diagnostics ?? [],
    };
  }

  console.warn('Failed to analyze document:', result.error);
  return { symbols: [], foldingRanges: [], diagnostics: [] };
}

/**
 * Get document symbols (outline) for a file.
 *
 * Convenience function - prefer analyzeDocument() when you need
 * multiple pieces of data.
 *
 * @param path - File path in VFS
 * @returns Hierarchical symbols for document outline
 */
export async function getSymbols(path: string): Promise<Symbol[]> {
  // Only QMD files have symbols
  if (!isQmdFile(path)) {
    return [];
  }

  const wasm = await getWasm();
  const result: LspSymbolsResponse = JSON.parse(wasm.lsp_get_symbols(path));

  if (result.success) {
    return result.symbols ?? [];
  }

  console.warn('Failed to get symbols:', result.error);
  return [];
}

/**
 * Get folding ranges for a file.
 *
 * Folding ranges include:
 * - YAML frontmatter (--- to ---)
 * - Code cells (```{lang} to ```)
 * - Sections (header to next same-level-or-higher header)
 *
 * @param path - File path in VFS
 * @returns Folding ranges for code folding
 */
export async function getFoldingRanges(path: string): Promise<FoldingRange[]> {
  // Only QMD files have folding ranges
  if (!isQmdFile(path)) {
    return [];
  }

  const wasm = await getWasm();
  const result: LspFoldingRangesResponse = JSON.parse(wasm.lsp_get_folding_ranges(path));

  if (result.success) {
    return result.foldingRanges ?? [];
  }

  console.warn('Failed to get folding ranges:', result.error);
  return [];
}

/**
 * Get diagnostics for a file.
 *
 * Returns rich diagnostics matching quarto-error-reporting structure,
 * including title, problem, hints, and details.
 *
 * @param path - File path in VFS
 * @returns Rich diagnostics array
 */
export async function getDiagnostics(path: string): Promise<Diagnostic[]> {
  // Only QMD files have diagnostics
  if (!isQmdFile(path)) {
    return [];
  }

  const wasm = await getWasm();
  const result: LspDiagnosticsResponse = JSON.parse(wasm.lsp_get_diagnostics(path));

  if (result.success) {
    return result.diagnostics ?? [];
  }

  console.warn('Failed to get diagnostics:', result.error);
  return [];
}

/**
 * Get semantic tokens for a file — drives Monaco syntax highlighting for
 * `.qmd` (qmd structure + frontmatter YAML + code-cell interiors).
 *
 * Graceful by design: a non-qmd path, a `{ success: false }` envelope, or a
 * JSON-decode failure all resolve to `[]` (never a rejection), so the provider
 * needs no try/catch on the normal path and treats "no tokens" uniformly as
 * "fall back to the Monarch base layer".
 *
 * @param path - File path in VFS
 * @returns Semantic tokens, sorted and non-overlapping
 */
export async function getSemanticTokens(path: string): Promise<SemanticToken[]> {
  if (!isQmdFile(path)) {
    return [];
  }

  try {
    const wasm = await getWasm();
    const result: LspSemanticTokensResponse = JSON.parse(wasm.lsp_get_semantic_tokens(path));

    if (result.success) {
      return result.tokens ?? [];
    }

    console.warn('Failed to get semantic tokens:', result.error);
    return [];
  } catch (e) {
    console.warn('Failed to get semantic tokens:', e);
    return [];
  }
}
