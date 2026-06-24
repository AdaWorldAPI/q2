/**
 * Source-location DOM attribute for scroll sync.
 *
 * The Rust JSON writer emits an `l` field on each node when
 * `include_inline_locations` is on (always true for q2-preview — see
 * `crates/quarto-core/src/pipeline.rs`). Its shape mirrors
 * `resolve_location` in `crates/pampa/src/writers/json.rs`:
 *
 *   l: { f: fileId, b: { o, l, c }, e: { o, l, c } }   // 1-based l/c
 *
 * `dataLocProps` turns that into the `data-loc` attribute the iframe
 * scroll-sync code matches on (`scrollSyncDom.ts`'s `parseDataLoc`),
 * using the same `fileId:startLine:startCol-endLine:endCol` format the
 * native HTML writer stamps. Returns `{}` for nodes without a
 * resolvable location, so spreading the result is always safe.
 */

interface ResolvedLoc {
    f: number;
    b: { o: number; l: number; c: number };
    e: { o: number; l: number; c: number };
}

export function dataLocProps(node: unknown): { 'data-loc'?: string } {
    const loc = (node as { l?: ResolvedLoc } | null | undefined)?.l;
    if (!loc || !loc.b || !loc.e) return {};
    return {
        'data-loc': `${loc.f}:${loc.b.l}:${loc.b.c}-${loc.e.l}:${loc.e.c}`,
    };
}
