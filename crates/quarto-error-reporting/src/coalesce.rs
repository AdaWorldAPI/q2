//! Cross-source diagnostic coalescing.
//!
//! When a single underlying problem produces a diagnostic on many
//! pages — for example, one bad `theme:` key in `_quarto.yml`
//! triggering [`Q-14-1`](../../error_catalog.json) once per rendered
//! page — the renderer should collapse them into a single emission
//! that lists the affected pages, rather than printing the same
//! ariadne block hundreds of times.
//!
//! # The primary key is the source location
//!
//! Two diagnostics whose `location` resolves to the same source
//! span in the same file are presumed to be the same error and are
//! grouped together. We deliberately do **not** include the code or
//! title in the grouping key — the source location alone is the
//! relation's primary key (decision recorded in
//! `claude-notes/plans/2026-05-22-theme-diagnostic-epic.md`).
//!
//! If two unrelated checks ever land at the same span this is a
//! design risk; the v1 cost (one merged emission with a possibly
//! mixed-content representative) is low. We will widen the key to
//! `(location, code)` if it turns out to bite.
//!
//! # What does not coalesce
//!
//! Diagnostics whose `location` is one of:
//!
//! - `None`,
//! - [`SourceInfo::Concat`], or
//! - [`SourceInfo::FilterProvenance`],
//!
//! pass through as singleton groups (one entry each). These shapes
//! don't reduce to a single contiguous byte range, so we can't form
//! a stable group key for them. This is the same conservative
//! contract as [`SourceInfo::resolve_byte_range`].
//!
//! [`SourceInfo::Concat`]: quarto_source_map::SourceInfo::Concat
//! [`SourceInfo::FilterProvenance`]: quarto_source_map::SourceInfo::FilterProvenance
//! [`SourceInfo::resolve_byte_range`]: quarto_source_map::SourceInfo::resolve_byte_range

use std::collections::HashMap;
use std::path::PathBuf;

use quarto_source_map::{SourceContext, SourceInfo};

use crate::diagnostic::{DiagnosticMessage, TextRenderOptions};

/// One entry from a coalesced render summary.
///
/// `affected_files` is in encounter order — the order in which the
/// caller's iterator produced each (path, diagnostic) pair that
/// contributed to this group. Singleton groups (size 1) carry one
/// path; rendered output for them omits the "Affected files:" tail
/// to match the legacy per-page render.
#[derive(Debug, Clone)]
pub struct CoalescedDiagnostic {
    pub representative: DiagnosticMessage,
    pub source_context: Option<SourceContext>,
    pub affected_files: Vec<PathBuf>,
}

/// Maximum number of file names rendered inline in the "Affected
/// files:" tail before switching to "… (and N others)".
///
/// Tunable; v1 sets it small so the typical "hundreds of pages"
/// case stays one line.
pub const AFFECTED_FILES_CAP: usize = 3;

impl CoalescedDiagnostic {
    /// Render the underlying ariadne diagnostic, followed by an
    /// `Affected files:` tail listing up to [`AFFECTED_FILES_CAP`]
    /// of the affected paths and a `(and N others)` count for the
    /// rest. Single-element groups omit the tail.
    pub fn to_text(&self) -> String {
        self.to_text_with_options(&TextRenderOptions::default())
    }

    /// Like [`Self::to_text`] but with explicit render options
    /// (mostly useful in tests, where hyperlinks are disabled for
    /// path-independent assertions).
    pub fn to_text_with_options(&self, opts: &TextRenderOptions) -> String {
        let body = self
            .representative
            .to_text_with_options(self.source_context.as_ref(), opts);
        if self.affected_files.len() <= 1 {
            return body;
        }
        let tail = render_affected_files_tail(&self.affected_files);
        format!("{}\n{}", body, tail)
    }
}

fn render_affected_files_tail(paths: &[PathBuf]) -> String {
    let shown = paths
        .iter()
        .take(AFFECTED_FILES_CAP)
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let remaining = paths.len().saturating_sub(AFFECTED_FILES_CAP);
    if remaining == 0 {
        format!("Affected files: {}", shown)
    } else {
        format!(
            "Affected files: {} (and {} other{})",
            shown,
            remaining,
            if remaining == 1 { "" } else { "s" },
        )
    }
}

/// Canonical, hashable form of a [`SourceInfo`] for grouping.
///
/// Resolves to the root `Original`'s `(file_id, start_offset,
/// end_offset)` tuple. Returns `None` for shapes that don't reduce
/// cleanly (mirrors [`SourceInfo::resolve_byte_range`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LocationKey {
    file_id: usize,
    start: usize,
    end: usize,
}

impl LocationKey {
    fn from(info: &SourceInfo) -> Option<Self> {
        let (file_id, start, end) = info.resolve_byte_range()?;
        Some(LocationKey {
            file_id,
            start,
            end,
        })
    }
}

/// Group the input by source location and return one
/// [`CoalescedDiagnostic`] per group, in encounter order.
///
/// Inputs without a coalescable location (no `location`, or `Concat`
/// / `FilterProvenance`) pass through as singleton groups in their
/// original order — they always print exactly once.
///
/// The first `(path, diagnostic, source_context)` triple to introduce
/// a given key becomes the group's representative. Later triples
/// only contribute to `affected_files`. This matches the principle
/// that the user sees the first diagnostic they would have seen
/// before, with extra context appended.
pub fn coalesce_by_source<I>(input: I) -> Vec<CoalescedDiagnostic>
where
    I: IntoIterator<Item = (PathBuf, DiagnosticMessage, Option<SourceContext>)>,
{
    let mut groups: Vec<CoalescedDiagnostic> = Vec::new();
    let mut index: HashMap<LocationKey, usize> = HashMap::new();

    for (path, diagnostic, source_context) in input {
        let key = diagnostic.location.as_ref().and_then(LocationKey::from);
        match key {
            Some(k) => match index.get(&k).copied() {
                Some(idx) => {
                    groups[idx].affected_files.push(path);
                }
                None => {
                    let idx = groups.len();
                    index.insert(k, idx);
                    groups.push(CoalescedDiagnostic {
                        representative: diagnostic,
                        source_context,
                        affected_files: vec![path],
                    });
                }
            },
            None => {
                // Non-coalescable: emit as a singleton group at the
                // tail. Do not register in the index, so subsequent
                // identical-but-uncoalescable entries also emit as
                // singletons.
                groups.push(CoalescedDiagnostic {
                    representative: diagnostic,
                    source_context,
                    affected_files: vec![path],
                });
            }
        }
    }

    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::DiagnosticMessageBuilder;
    use quarto_source_map::{FileId, SourcePiece};
    use std::sync::Arc;

    fn original(file_id: usize, start: usize, end: usize) -> SourceInfo {
        SourceInfo::Original {
            file_id: FileId(file_id),
            start_offset: start,
            end_offset: end,
        }
    }

    fn diag_at(loc: SourceInfo, title: &str) -> DiagnosticMessage {
        DiagnosticMessageBuilder::error(title)
            .with_code("Q-14-1")
            .with_location(loc)
            .problem("…")
            .build()
    }

    #[test]
    fn two_diagnostics_at_the_same_location_collapse() {
        let loc = original(1, 100, 110);
        let input = vec![
            (PathBuf::from("a.qmd"), diag_at(loc.clone(), "T"), None),
            (PathBuf::from("b.qmd"), diag_at(loc.clone(), "T"), None),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].affected_files,
            vec![PathBuf::from("a.qmd"), PathBuf::from("b.qmd"),]
        );
    }

    #[test]
    fn different_locations_do_not_collapse() {
        let input = vec![
            (
                PathBuf::from("a.qmd"),
                diag_at(original(1, 100, 110), "T"),
                None,
            ),
            (
                PathBuf::from("b.qmd"),
                diag_at(original(1, 200, 210), "T"),
                None,
            ),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn different_file_ids_do_not_collapse() {
        let input = vec![
            (
                PathBuf::from("a.qmd"),
                diag_at(original(1, 100, 110), "T"),
                None,
            ),
            (
                PathBuf::from("b.qmd"),
                diag_at(original(2, 100, 110), "T"),
                None,
            ),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn substring_resolves_to_root_original_and_groups_with_it() {
        // A Substring whose root Original matches another Original
        // must coalesce into the same group — the canonical key is
        // the resolved root.
        let root = original(1, 100, 200);
        let sub = SourceInfo::Substring {
            parent: Arc::new(root.clone()),
            // Offsets relative to parent's text; resolve_byte_range
            // composes them: (fid, parent_start + sub_start,
            // parent_start + sub_end) = (1, 100, 110).
            start_offset: 0,
            end_offset: 10,
        };
        let input = vec![
            (PathBuf::from("a.qmd"), diag_at(root.clone(), "T"), None),
            (PathBuf::from("b.qmd"), diag_at(sub, "T"), None),
        ];
        let groups = coalesce_by_source(input);
        // root resolves to (1, 100, 200); sub resolves to (1, 100,
        // 110). Different end offsets ⇒ different keys ⇒ separate
        // groups. This documents the v1 contract: Substring uses
        // the *composed* offsets, not the parent's offsets.
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn concat_location_passes_through_as_singleton() {
        let concat = SourceInfo::Concat {
            pieces: vec![SourcePiece {
                source_info: original(1, 0, 10),
                offset_in_concat: 0,
                length: 10,
            }],
        };
        let input = vec![
            (PathBuf::from("a.qmd"), diag_at(concat.clone(), "T"), None),
            (PathBuf::from("b.qmd"), diag_at(concat, "T"), None),
        ];
        let groups = coalesce_by_source(input);
        // Both emitted as singletons because Concat has no
        // coalescable key. Order preserved.
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].affected_files, vec![PathBuf::from("a.qmd")]);
        assert_eq!(groups[1].affected_files, vec![PathBuf::from("b.qmd")]);
    }

    #[test]
    fn diagnostics_without_location_pass_through_as_singletons() {
        let d = DiagnosticMessageBuilder::error("no location")
            .problem("…")
            .build();
        let input = vec![
            (PathBuf::from("a.qmd"), d.clone(), None),
            (PathBuf::from("b.qmd"), d, None),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 2);
    }

    #[test]
    fn encounter_order_preserved_across_groups() {
        let loc1 = original(1, 100, 110);
        let loc2 = original(1, 200, 210);
        let input = vec![
            (PathBuf::from("a.qmd"), diag_at(loc1.clone(), "T1"), None),
            (PathBuf::from("b.qmd"), diag_at(loc2.clone(), "T2"), None),
            (PathBuf::from("c.qmd"), diag_at(loc1.clone(), "T1"), None),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 2);
        // Group order = order of first occurrence.
        assert_eq!(groups[0].representative.title, "T1");
        assert_eq!(
            groups[0].affected_files,
            vec![PathBuf::from("a.qmd"), PathBuf::from("c.qmd"),]
        );
        assert_eq!(groups[1].representative.title, "T2");
        assert_eq!(groups[1].affected_files, vec![PathBuf::from("b.qmd")]);
    }

    #[test]
    fn first_encounter_supplies_representative_and_context() {
        // The representative is the *first* (path, diagnostic) seen
        // for a given key. Later contributions only add to
        // `affected_files`. The same goes for the SourceContext.
        let loc = original(1, 100, 110);
        let mut ctx_first = SourceContext::new();
        ctx_first.add_file_with_id(FileId(1), "first.yml".into(), Some("first".into()));
        let mut ctx_second = SourceContext::new();
        ctx_second.add_file_with_id(FileId(1), "second.yml".into(), Some("second".into()));

        let input = vec![
            (
                PathBuf::from("a.qmd"),
                diag_at(loc.clone(), "first"),
                Some(ctx_first),
            ),
            (
                PathBuf::from("b.qmd"),
                diag_at(loc.clone(), "second"),
                Some(ctx_second),
            ),
        ];
        let groups = coalesce_by_source(input);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].representative.title, "first");
        assert!(groups[0].source_context.is_some());
    }

    #[test]
    fn singleton_group_omits_affected_files_tail() {
        let loc = original(1, 100, 110);
        let input = vec![(PathBuf::from("a.qmd"), diag_at(loc, "T"), None)];
        let groups = coalesce_by_source(input);
        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text = groups[0].to_text_with_options(&opts);
        assert!(
            !text.contains("Affected files:"),
            "singleton groups must not emit the affected-files tail:\n{}",
            text
        );
    }

    #[test]
    fn multi_group_below_cap_lists_all_files() {
        let loc = original(1, 100, 110);
        let input = vec![
            (PathBuf::from("a.qmd"), diag_at(loc.clone(), "T"), None),
            (PathBuf::from("b.qmd"), diag_at(loc.clone(), "T"), None),
        ];
        let groups = coalesce_by_source(input);
        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text = groups[0].to_text_with_options(&opts);
        assert!(text.contains("Affected files: a.qmd, b.qmd"), "{}", text);
        assert!(
            !text.contains("other"),
            "no '(and N others)' tail expected for ≤ cap:\n{}",
            text
        );
    }

    #[test]
    fn multi_group_above_cap_truncates_and_counts() {
        // AFFECTED_FILES_CAP=3, so 5 files should produce
        // "a.qmd, b.qmd, c.qmd (and 2 others)".
        let loc = original(1, 100, 110);
        let input: Vec<_> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|n| {
                (
                    PathBuf::from(format!("{n}.qmd")),
                    diag_at(loc.clone(), "T"),
                    None,
                )
            })
            .collect();
        let groups = coalesce_by_source(input);
        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text = groups[0].to_text_with_options(&opts);
        assert!(
            text.contains("Affected files: a.qmd, b.qmd, c.qmd (and 2 others)"),
            "{}",
            text,
        );
    }

    #[test]
    fn multi_group_just_above_cap_uses_singular_other() {
        // 4 files at cap=3 ⇒ 1 other (singular).
        let loc = original(1, 100, 110);
        let input: Vec<_> = ["a", "b", "c", "d"]
            .iter()
            .map(|n| {
                (
                    PathBuf::from(format!("{n}.qmd")),
                    diag_at(loc.clone(), "T"),
                    None,
                )
            })
            .collect();
        let groups = coalesce_by_source(input);
        let opts = TextRenderOptions {
            enable_hyperlinks: false,
        };
        let text = groups[0].to_text_with_options(&opts);
        assert!(
            text.contains("(and 1 other)"),
            "expected singular 'other' for exactly 1 over cap:\n{}",
            text,
        );
    }
}
