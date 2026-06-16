//! Node-exact capture extraction and the shared innermost-wins flatten.
//!
//! This is the **one resolver both consumers share**: the render producer
//! (`Registry::highlight` / `UserGrammars::highlight`) and the editor's
//! semantic-token extractor (`quarto-lsp-core::tokens`) both extract spans
//! with [`captures_from_tree`] and collapse them with [`flatten_spans`], so
//! a code cell is coloured identically in the editor and the rendered HTML
//! by construction.
//!
//! It replaces the old `tree-sitter-highlight` `HighlightEvent`-stream walk
//! (`collect_spans`), which was lossy for same-start nested captures: it
//! dropped the inner capture's end boundary, stretching it to the outer
//! capture's end (bd-98k6). `tree_sitter::Query::captures()` returns
//! node-exact byte ranges, so the boundary survives.

use tree_sitter::{Node, Query, QueryCursor, StreamingIterator};

use crate::encoding::HighlightSpan;

/// Run a `QueryCursor` over `root` and return one [`HighlightSpan`] per
/// capture — node-exact `(start_byte, end_byte, capture_name)`, **unflattened**
/// (captures from one query over one tree nest; [`flatten_spans`] collapses
/// them).
///
/// `source` is the byte slice the tree was parsed from, used as the query's
/// text provider for predicate evaluation.
pub(crate) fn captures_from_tree(
    query: &Query,
    root: Node<'_>,
    source: &[u8],
) -> Vec<HighlightSpan> {
    let names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut spans = Vec::new();
    let mut it = cursor.captures(query, root, source);
    while let Some((m, capture_index)) = it.next() {
        let cap = m.captures[*capture_index];
        let name = names.get(cap.index as usize).copied().unwrap_or("unknown");
        let node = cap.node;
        spans.push(HighlightSpan {
            start: node.start_byte(),
            end: node.end_byte(),
            capture: name.to_string(),
        });
    }
    spans
}

/// Collapse nested/overlapping capture spans into a flat, non-overlapping,
/// start-sorted run where the **innermost (narrowest) capture wins each byte**.
///
/// Captures from one `Query` over one tree are nested-or-disjoint (CST node
/// ranges never partially overlap), so for any byte the covering captures form
/// a strict nesting chain and "narrowest" is unambiguous. The wider span is
/// split around the narrower one.
///
/// **Tie-break.** The one residual ambiguity is two captures at *equal extent*
/// (identical start *and* end — two patterns matching the same node). This
/// cannot arise from the structural nesting of a single tree, only from a query
/// that captures one node twice. It is resolved deterministically: the capture
/// appearing **later in the tree-sitter capture stream wins** (the stable sort
/// below preserves capture-stream order for equal-extent spans, and the paint
/// buffer lets the last-painted span win). No built-in-language corpus golden
/// contains a genuine equal-extent collision — the
/// `tests/fixtures/user-grammar-equal-extent` fixture exists to exercise this
/// path.
///
/// Zero-width spans (`start == end`) are dropped. The output is idempotent:
/// `flatten_spans(flatten_spans(x)) == flatten_spans(x)`.
///
/// This stays multi-line: a span crossing a newline is emitted as one span.
/// The editor's LSP conversion splits per-line separately; the render/HTML path
/// wants multi-line spans, so the split must **not** live here.
pub fn flatten_spans(mut spans: Vec<HighlightSpan>) -> Vec<HighlightSpan> {
    spans.retain(|s| s.end > s.start);
    if spans.is_empty() {
        return Vec::new();
    }

    let min = spans.iter().map(|s| s.start).min().expect("non-empty");
    let max = spans.iter().map(|s| s.end).max().expect("non-empty");

    // Stable sort: start ascending, then width descending so that at a given
    // start the wider span is painted first and the narrower overwrites it
    // (innermost-wins). Stability keeps equal-extent captures in capture-stream
    // order, so the last one painted (the later one in the stream) wins the tie.
    spans.sort_by(|a, b| {
        a.start
            .cmp(&b.start)
            .then_with(|| (b.end - b.start).cmp(&(a.end - a.start)))
    });

    // Per-byte owner = index (into the sorted `spans`) of the winning capture.
    let len = max - min;
    let mut owner: Vec<Option<usize>> = vec![None; len];
    for (idx, s) in spans.iter().enumerate() {
        for byte in &mut owner[(s.start - min)..(s.end - min)] {
            *byte = Some(idx);
        }
    }

    // Run-length encode contiguous same-owner runs, skipping uncovered gaps.
    // Keying runs on the owner *index* (not the capture string) keeps the
    // output idempotent: two adjacent flattened spans that happen to share a
    // capture name stay distinct, so re-flattening reproduces them exactly.
    let mut out: Vec<HighlightSpan> = Vec::new();
    let mut run_owner: Option<usize> = None;
    let mut run_start = 0usize;
    for (offset, &o) in owner.iter().enumerate() {
        if o != run_owner {
            if let Some(idx) = run_owner {
                out.push(HighlightSpan {
                    start: run_start + min,
                    end: offset + min,
                    capture: spans[idx].capture.clone(),
                });
            }
            run_owner = o;
            run_start = offset;
        }
    }
    if let Some(idx) = run_owner {
        out.push(HighlightSpan {
            start: run_start + min,
            end: len + min,
            capture: spans[idx].capture.clone(),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: usize, end: usize, capture: &str) -> HighlightSpan {
        HighlightSpan {
            start,
            end,
            capture: capture.to_string(),
        }
    }

    #[test]
    fn flatten_innermost_wins_over_nested() {
        // Outer `string` [0,14] with an inner `variable` [5,9]: the inner
        // splits the outer into [0,5] and [9,14].
        let out = flatten_spans(vec![span(0, 14, "string"), span(5, 9, "variable")]);
        assert_eq!(
            out,
            vec![
                span(0, 5, "string"),
                span(5, 9, "variable"),
                span(9, 14, "string"),
            ]
        );
    }

    #[test]
    fn flatten_drops_fully_covered_wrapper() {
        // `embedded` [3,9] is fully tiled by its children → it wins no byte
        // and disappears (the python f-string `{name}` case).
        let out = flatten_spans(vec![
            span(0, 12, "string"),
            span(3, 9, "embedded"),
            span(3, 4, "punctuation.special"),
            span(4, 8, "variable"),
            span(8, 9, "punctuation.special"),
        ]);
        assert_eq!(
            out,
            vec![
                span(0, 3, "string"),
                span(3, 4, "punctuation.special"),
                span(4, 8, "variable"),
                span(8, 9, "punctuation.special"),
                span(9, 12, "string"),
            ]
        );
    }

    #[test]
    fn flatten_keeps_disjoint_spans() {
        let out = flatten_spans(vec![span(0, 3, "keyword"), span(4, 7, "function")]);
        assert_eq!(out, vec![span(0, 3, "keyword"), span(4, 7, "function")]);
    }

    #[test]
    fn flatten_sorts_and_drops_zero_width() {
        let out = flatten_spans(vec![span(4, 7, "b"), span(2, 2, "empty"), span(0, 3, "a")]);
        assert_eq!(out, vec![span(0, 3, "a"), span(4, 7, "b")]);
    }

    #[test]
    fn flatten_is_idempotent() {
        let input = vec![
            span(0, 14, "property"),
            span(0, 4, "type"),
            span(5, 6, "operator"),
            span(7, 14, "string"),
        ];
        let once = flatten_spans(input);
        let twice = flatten_spans(once.clone());
        assert_eq!(once, twice);
    }

    #[test]
    fn flatten_handles_equal_extent_tie() {
        // Two captures at the identical range collapse to exactly one span.
        // Later-in-stream wins (here: `property`, appended second).
        let out = flatten_spans(vec![span(0, 4, "type"), span(0, 4, "property")]);
        assert_eq!(out, vec![span(0, 4, "property")]);
    }

    #[test]
    fn flatten_empty() {
        assert!(flatten_spans(vec![]).is_empty());
    }
}
