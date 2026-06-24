// Phase 1 (bd-sjb4pzx8) — minimal Pandoc-AST shapes + source slicing for the
// rich-text bridge. The untransformed AST nodes carry `s` (a pool index) and the
// pool entries carry `r` (UTF-8 byte range); opaque constructs are reconstructed
// verbatim by slicing `content` at that range.

/** A source-info pool entry. Only `r` (byte range) is needed here. */
export interface PoolEntry {
  r: [number, number];
  t?: number;
  d?: unknown;
}

/** A node's literal source location (begin/end byte offset in `o`). */
export interface NodeLoc {
  b?: { o?: number };
  e?: { o?: number };
}

/** Loose annotated Pandoc node: `t` tag, `c` content, `s` pool index, `l` loc. */
export interface AstNode {
  t: string;
  c?: unknown;
  s?: number;
  l?: NodeLoc;
  [k: string]: unknown;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/** Slice `src` by UTF-8 byte offsets (pampa's range coordinate space). */
export function sliceBytes(src: string, start: number, end: number): string {
  return decoder.decode(encoder.encode(src).subarray(start, end));
}

/**
 * Verbatim source text for a node.
 *
 * Prefers the node's own literal location (`.l`) over the compact pool entry
 * (`.s`). The pool range is reliable for leaf inlines (Math/Cite) but is
 * mis-assigned for container inlines like shortcode spans — there the pool
 * points at an adjacent space while `.l` points at the actual token
 * (e.g. `{{< meta key >}}`). Falls back to the pool, then null.
 */
export function nodeSource(node: AstNode, pool: PoolEntry[], src: string): string | null {
  const b = node.l?.b?.o;
  const e = node.l?.e?.o;
  if (typeof b === 'number' && typeof e === 'number' && e >= b) {
    return sliceBytes(src, b, e);
  }
  if (node.s == null) return null;
  const entry = pool[node.s];
  if (!entry || !entry.r) return null;
  return sliceBytes(src, entry.r[0], entry.r[1]);
}
