/**
 * BreadcrumbChip.tsx — floating ancestor-path breadcrumb for the nesting cursor.
 *
 * P3.4 §3d: shows the AST ancestor path (e.g. "Section › Div › Paragraph")
 * with the current level highlighted. Rendered when unlockNestingCursor=true
 * AND an editor is open (editTarget != null). Self-gating: renders null
 * otherwise.
 *
 * Pointer-isolation note: stopPropagation/preventDefault are implemented here
 * for correct real-browser behaviour (prevent host click-switch; prevent blur-
 * commit on button press). jsdom's fireEvent.click does not simulate pointer
 * events or focus/blur, so these behaviours are NOT jsdom-tested here.
 * Pointer-isolation testing is deferred to P3.5 Playwright.
 *
 * ## Positioning model (Phase 3)
 *
 * Mount host: `#quarto-content` (a `page-columns` grid container spanning
 * screen-start → screen-end). The chip is `position:absolute` inside it.
 *
 * `#quarto-content` is given `position: relative` via the injected stylesheet
 * so it becomes the chip's offset-parent. Both the chip's containing block and
 * the active surface then scroll together with `#root { overflow:auto }` — a
 * once-computed offset remains scroll-stable with no listener required.
 *
 * Stacking-context note: `position: relative` on `#quarto-content` creates a
 * stacking context. Risk rated LOW (no `position:fixed` children inside
 * #quarto-content; sidebar and TOC overlays have their own contexts above the
 * page grid). The chip's `z-index:50` is intentional: paint above sidebar(z≈1)
 * and main(z=0) but below any modal/dialog. See §Positioning model in the plan.
 *
 * ## Layout model (pivot-pinned left-spill, Phase 3b/3c + G1)
 *
 * The crumb row's RIGHT edge is pinned at the pivot (the active surface's left
 * edge, `surfaceLeft`). The band is sized to the *comfortable* width the whole
 * path wants — `naturalWidth = crumbCount * CRUMB_W` — not the legibility floor.
 * When the indent gutter [colLeft, surfaceLeft] is wide enough to hold that, the
 * band fills the gutter exactly (no change from the old model). When it isn't —
 * a shallow nesting, or a container that contributes no indent of its own (code
 * block, non-indenting Div) — the excess band width pushes the chip's left edge
 * LEFT into the page margin. Crumbs DO enter the margin in that case; that is the
 * "where you came from" direction. ◀ rides the chip's left edge.
 *
 *   page edge          text-col margin (colLeft)      surface-left (pivot)
 *   | ◀  ❝  Cd |                                  [ editing surface text … ]
 *        └─ shallow path: band spilled left past colLeft into the margin ─┘
 *
 * Hard stop (left page edge): if the left-spill would cross `#quarto-content`'s
 * left edge (x < 0) the chip would leave `#root`'s content box and add a horizontal
 * scrollbar — so ◀ is pinned at x=0 and, rather than crunching, the band keeps its
 * comfortable width and spills RIGHT past the pivot (over the content, which has
 * room). The pure math lives in `computeChipGeometry`.
 */

import React, { useContext, useLayoutEffect, useRef, useState } from 'react';
import { PreviewContext } from './PreviewContext';
import { buildAncestorPath, currentSourceNodeType } from './nestingNav';
import type { AncestorCrumb } from './nestingNav';
import { BreadcrumbCrumbs, type CrumbDisplayItem } from './BreadcrumbCrumbs';
import { richEditorActiveForType } from './richTextSupport';
import { shouldPlaceChromeBelow } from './editChromeGeometry';

/** Gap (px) between the chip and the surface when flipped below it (bd-pvcnea83). */
const CHIP_FLIP_GAP = 4;

// ── Constants ──────────────────────────────────────────────────────────────────

/**
 * Minimum legible width (px) for the ◀ button and the LEGIBILITY FLOOR used to
 * count how many crumbs fit a band before the middle must ellipsize. Most crumb
 * glyphs are a single character (•, ¶, H2, Dv, 1.) at 12px font with ~3px padding
 * each side, so ~16px is the smallest that stays legible. NOTE: this is only the
 * floor for the slot count / page-edge ellipsize decision — the *comfortable*
 * per-crumb width used to size the band is `CRUMB_W`.
 */
export const MIN_GLYPH_W = 16;

/**
 * Comfortable per-crumb width (px) used to size the crumb band (`naturalWidth =
 * crumbCount * CRUMB_W`) so a shallow / zero-indent path spills LEFT into the
 * margin at a readable width rather than crunching to the `MIN_GLYPH_W` floor.
 *
 * LIVE-TUNED (G1 step 3): the value is a visual judgement, not a correctness
 * gate — `computeChipGeometry`'s tests derive their expectations from this
 * constant, so changing it cannot break them. Tune against a code-block-in-
 * blockquote (must show both crumbs without crunch) and a 3-level list (must not
 * over-spill past the page's left edge). Floor is `MIN_GLYPH_W` (16); start ~26.
 */
export const CRUMB_W = 22;

// ── Chip geometry (computed once per editTarget change) ────────────────────────
//
// The crumb row fills a fixed band [◀.right, surfaceLeft] via flexbox; the crumb
// UI itself (◀ · band · ▶ · future) is rendered by the shared `BreadcrumbCrumbs`
// component (also used inline in the rich-text toolbar — bd-9x3zbuj8 Task 2). This
// file owns only the standalone POSITIONING geometry.

interface ChipGeometry {
    /** chip top (px, relative to #quarto-content). */
    top: number;
    /** left edge of the chip (px, relative to #quarto-content), from
     *  `computeChipGeometry`. Equals `surfaceLeft − OUT_W − bandWidth` (right edge
     *  pinned at the pivot), so for a shallow / zero-indent path it sits LEFT of
     *  the text column (crumbs spill into the margin); clamped to 0 at the page
     *  edge. For a deep indent it reduces to `colLeft − OUT_W` (old behavior). */
    chipLeft: number;
    /** Width (px) of the crumb band, from `computeChipGeometry`:
     *  `max(gutter, crumbCount * CRUMB_W, MIN_GLYPH_W)` — the comfortable natural
     *  width unless the gutter is already wider. The crumbs flex to fill it; the
     *  band's right edge meets the pivot, EXCEPT when ◀ is pinned at the left page
     *  edge (x=0), where the band keeps this width and spills right past the pivot. */
    bandWidth: number;
    /** Items for the crumb row (may include a single ellipsis when the path is too
     *  long to show every crumb at a legible width). */
    displayItems: CrumbDisplayItem[];
}

/** Select which crumbs to show given how many MIN_GLYPH_W slots fit the band. */
function selectDisplayItems(crumbs: AncestorCrumb[], slots: number): CrumbDisplayItem[] {
    const n = crumbs.length;
    if (n === 0) return [];
    if (slots >= n) return crumbs.map((crumb) => ({ kind: 'crumb', crumb }));
    if (slots <= 1) return [{ kind: 'crumb', crumb: crumbs[n - 1] }]; // current only
    if (slots === 2) {
        // root + current
        return [{ kind: 'crumb', crumb: crumbs[0] }, { kind: 'crumb', crumb: crumbs[n - 1] }];
    }
    // root … current
    return [
        { kind: 'crumb', crumb: crumbs[0] },
        { kind: 'ellipsis' },
        { kind: 'crumb', crumb: crumbs[n - 1] },
    ];
}

/**
 * Pure chip geometry (G1 — pivot-pinned left-spill). Inputs are host-relative px:
 * `surfaceLeft` (the pivot = active surface's left edge), `colLeft` (text-column
 * left margin), and the crumb count. Returns the chip's left edge, the crumb-band
 * width, and the slot count for `selectDisplayItems`.
 *
 * The band's RIGHT edge is always pinned at the pivot (`surfaceLeft`); the band is
 * sized to the *comfortable* `naturalWidth = crumbCount * CRUMB_W` (not the
 * legibility floor), so when a container contributes little/no indent gutter the
 * excess width pushes the chip's left edge LEFT into the page margin — the "where
 * you came from" direction — instead of collapsing crumbs. Invariants:
 *   - Deep indent (`gutter ≥ naturalWidth`): `bandWidth == gutter`,
 *     `chipLeft == colLeft − OUT_W` — identical to the old gutter-only model.
 *   - Out of room on the left (chipLeft < 0): pin ◀ at the page edge (x=0) and keep
 *     the comfortable band width, letting it spill RIGHT past the pivot (over the
 *     content) rather than crunching/ellipsizing — the right side has room, so this
 *     adds no horizontal scrollbar. The right-edge-at-pivot invariant is relaxed
 *     here by design.
 *   - Unmeasured layout (`surfaceLeft ≤ 0`, e.g. jsdom zero-rects): keep the old
 *     anchor and report `slots == crumbCount` so the full path still renders.
 */
export function computeChipGeometry(
    surfaceLeft: number,
    colLeft: number,
    crumbCount: number,
): { chipLeft: number; bandWidth: number; slots: number } {
    const OUT_W = MIN_GLYPH_W;
    const gutter = surfaceLeft - colLeft;
    const naturalWidth = crumbCount * CRUMB_W; // comfortable, not the floor
    const bandWidth = Math.max(gutter, naturalWidth, MIN_GLYPH_W);
    // Pin the band's right edge at the pivot; excess width pushes chipLeft left.
    let chipLeft = surfaceLeft - OUT_W - bandWidth;
    if (surfaceLeft > 0 && chipLeft < 0) {
        // Ran out of room on the LEFT: pin ◀ at the page edge (x=0) and keep the
        // comfortable band width, letting it extend RIGHT past the pivot (over the
        // content) instead of crunching/ellipsizing. The content column has room to
        // the right, so right-spill adds no horizontal scrollbar (unlike left-spill,
        // which would push past x=0). The band's right edge is no longer pinned at
        // the pivot in this case — by design.
        chipLeft = 0;
    } else if (surfaceLeft <= 0) {
        // Unmeasured layout (jsdom): keep the old anchor so the full path renders.
        chipLeft = Math.max(0, colLeft - OUT_W);
    }
    const slots = surfaceLeft > 0 ? Math.floor(bandWidth / MIN_GLYPH_W) : crumbCount;
    return { chipLeft, bandWidth, slots };
}

// ── Component ──────────────────────────────────────────────────────────────────

export function BreadcrumbChip(): React.ReactElement | null {
    const ctx = useContext(PreviewContext);
    const chipRef = useRef<HTMLDivElement | null>(null);
    const [geom, setGeom] = useState<ChipGeometry | null>(null);

    const et = ctx?.editTarget;
    // Suppress the standalone floating chip when the rich-text editor is showing
    // for this target — there the breadcrumb renders INLINE in the toolbar row
    // instead (bd-9x3zbuj8 Task 2), so a standalone chip would double it up.
    // currentSourceNodeType resolves the active node's type from the source index;
    // richEditorActiveForType applies the same predicate the dispatcher uses to
    // pick the rich surface (so this stays in lock-step with editorMode/richText).
    const nodeType = et ? currentSourceNodeType(ctx?.sourceIndex, et.anchorR0, et.anchorR1) : null;
    const richInlineActive = !!ctx && nodeType != null && richEditorActiveForType(ctx, nodeType);
    const active = !!ctx?.unlockNestingCursor && !!et && !richInlineActive;

    // ── Geometry effect (Phase 3: content-plane anchor, no scroll listener) ───
    //
    // Fires on editTarget change only. Both the chip's offset-parent
    // (#quarto-content, position:relative) and the surface element live inside
    // #root's scroll container, so a once-computed offset stays correct under
    // scroll — no recompute, no lag.
    useLayoutEffect(() => {
        if (!active) { setGeom(null); return; }

        // --- surface ---
        // Anchor to the actual editing surface (the <textarea>), not the
        // #q2-active-edit-region wrapper: the wrapper spans the full text column
        // (left = colLeft) for every block, so anchoring to it loses the block's
        // indent. The textarea sits at the block's real content left (indented for
        // list/blockquote items), which is the "surface left edge" the crumb row
        // must meet. Fall back to the wrapper when no textarea is mounted.
        const editRegion = ctx?.activeEditRegionRef?.current;
        if (!editRegion) { setGeom(null); return; }
        const surface = editRegion.querySelector('textarea') ?? editRegion;

        // --- host (#quarto-content, the chip's offset-parent) ---
        // Look up directly by id — NOT via surface.offsetParent.
        // Using surface.offsetParent was the prior defect: when #quarto-content
        // had no `position`, the chip's containing block resolved to the
        // viewport ICB (which doesn't move when #root scrolls internally),
        // causing the chip to detach from the surface on scroll.
        const host = document.getElementById('quarto-content');
        if (!host) { setGeom(null); return; }

        const hostRect = host.getBoundingClientRect();
        const sRect = surface.getBoundingClientRect();

        // Coords relative to host (scroll-stable because both share #root scroll)
        const surfaceLeft = sRect.left - hostRect.left;
        const surfaceTop = sRect.top - hostRect.top;

        // Text-column left margin (colLeft): ◀'s right edge pins here, and the crumb
        // band starts here — so only ◀ is in the outer margin, never the crumbs.
        const mainEl = document.querySelector('main#quarto-document-content');
        const colLeft = mainEl
            ? mainEl.getBoundingClientRect().left - hostRect.left
            : surfaceLeft;

        // --- Source crumbs (outermost-first, current = last) ---
        const crumbs = et
            ? buildAncestorPath(ctx?.sourceIndex, et.anchorR0, et.anchorR1)
            : [];

        // --- Layout: pivot-pinned left-spill (G1) ---
        // The band's right edge is pinned at the pivot (surfaceLeft); it is sized to
        // the comfortable naturalWidth (crumbs * CRUMB_W), so a shallow / zero-indent
        // path spills the chip LEFT into the page margin rather than collapsing. ◀
        // rides the chip's left edge; ▶ + the future placeholder sit just right of the
        // pivot (over the content). See computeChipGeometry for the full contract.
        const { chipLeft, bandWidth, slots } = computeChipGeometry(
            surfaceLeft,
            colLeft,
            crumbs.length,
        );
        const displayItems = selectDisplayItems(crumbs, slots);

        // --- Chip top: above the surface by default; flip below when clipped ---
        // Default: bottom edge flush at the surface top (top = surfaceTop − chipH).
        // But for a surface flush against the viewport top (e.g. the first block of
        // a title-less document) that lands above the scroll area and is cropped, so
        // flip BELOW the surface instead (bd-pvcnea83). Decide from the surface's
        // viewport-relative top (sRect.top), guarded on real geometry so jsdom's
        // zero-rects keep the default 'above' placement.
        const chipH = chipRef.current?.getBoundingClientRect().height ?? 0;
        const haveRealGeometry = sRect.height > 0;
        const flipBelow = haveRealGeometry
            && shouldPlaceChromeBelow(sRect.top, chipH, CHIP_FLIP_GAP);
        const top = flipBelow
            ? (sRect.bottom - hostRect.top) + CHIP_FLIP_GAP // below the surface
            : surfaceTop - chipH;                           // above (default)

        setGeom({ top, chipLeft, bandWidth, displayItems });
    }, [active, et?.anchorR0, et?.anchorR1, ctx?.activeEditRegionRef, ctx?.sourceIndex]);

    if (!active || !et) return null;

    const crumbs = buildAncestorPath(ctx?.sourceIndex, et.anchorR0, et.anchorR1);

    // stopPropagation: the host (#quarto-content) carries delegated pointer
    // handlers (useBlockEditHover); the standalone chip floats OUTSIDE the edit
    // region, so it must fully intercept its own pointer events — a chip click
    // must never be read as a leaf-reset/click-switch, and pointerdown must not
    // blur-commit the textarea. (The inline rendering sits INSIDE the edit region,
    // where the host's active-region guard already ignores it.)
    // [Real focus/blur + pointer-ordering: verified in P3.5.]
    const eat = (e: React.PointerEvent) => { e.stopPropagation(); e.preventDefault(); };

    // Determine display items for the crumb row.
    // When geom is available, use geom.displayItems (may be ellipsized).
    // When geom is null (first render / not yet measured), fall back to full crumbs
    // at natural widths (unpositioned chip renders for height measurement).
    const displayItems: CrumbDisplayItem[] = geom
        ? geom.displayItems
        : crumbs.map((crumb) => ({ kind: 'crumb' as const, crumb }));

    // The crumb styles (incl. `#quarto-content { position: relative }` that the
    // geometry effect's offset-parent lookup relies on) are injected by
    // BreadcrumbCrumbs via ensureBreadcrumbStyles(), rendered as our child below.
    return (
        // Phase 3d: position:absolute — never reflows; paints into outer margin
        // (no overflow:hidden on #quarto-content or page-columns ancestors).
        <div
            ref={chipRef}
            className="q2-breadcrumb-chip"
            data-testid="q2-breadcrumb-chip"
            role="toolbar"
            aria-label="Nesting breadcrumb"
            onPointerDown={eat}
            onPointerUp={(e) => e.stopPropagation()}
            style={{
                position: 'absolute',
                // #quarto-content is a CSS grid (.page-columns). An abspos child
                // of a grid is contained by its GRID AREA, not the grid box — so
                // without this it auto-places into the body content column and
                // `left:0` resolves to the column edge (colLeft), unable to reach
                // the outer page margin. Spanning screen-start→screen-end makes the
                // grid area the full page width, so computed left/top (measured vs
                // #quarto-content) resolve against the full box and margin-spill works.
                gridColumn: 'screen-start / screen-end',
                gridRow: '1 / -1',
                top: geom ? `${geom.top}px` : undefined,
                // Left edge from computeChipGeometry (pivot-pinned left-spill):
                // surfaceLeft − ◀ − band, clamped to ≥ 0 at the page edge.
                left: geom ? `${geom.chipLeft}px` : undefined,
                zIndex: 50,
            }}
        >
            <BreadcrumbCrumbs
                layout="standalone"
                displayItems={displayItems}
                bandWidth={geom?.bandWidth}
            />
        </div>
    );
}
