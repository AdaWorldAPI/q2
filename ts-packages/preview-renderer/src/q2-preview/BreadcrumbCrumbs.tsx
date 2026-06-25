/**
 * BreadcrumbCrumbs.tsx — the position-agnostic crumb UI shared by the two
 * breadcrumb renderings (bd-9x3zbuj8 Task 2):
 *
 *   - STANDALONE: rendered inside the floating, absolutely-positioned
 *     `BreadcrumbChip` pill (for plain-text blocks: lists, code, divs, …, or a
 *     Para/Header the user toggled to plain mode). Uses the pivot-pinned
 *     left-spill geometry — the band is a fixed `bandWidth` the crumbs flex to
 *     fill, and the middle may ellipsize.
 *   - INLINE: rendered in the rich-text toolbar's flex row, to the RIGHT of the
 *     formatting buttons (for Para/Header in rich mode). No geometry: the crumbs
 *     size to their natural content width and flow after the toolbar buttons, so
 *     the toolbar's left edge stays fixed regardless of breadcrumb width.
 *
 * This component renders ONLY the inner trio (◀ · crumb band · ▶ · future
 * placeholder) as a fragment, so each caller supplies its own wrapper (the
 * positioned pill for standalone; the toolbar row for inline). It reads the
 * nesting handlers off `PreviewContext` and injects the shared stylesheet once.
 */

import React, { useContext } from 'react';
import { PreviewContext } from './PreviewContext';
import { detectPlatform } from './nestingNav';
import type { AncestorCrumb } from './nestingNav';

// ── Display item types ─────────────────────────────────────────────────────────

// A crumb to render, or an ellipsis standing in for the elided middle of a path
// too long to show at a legible width (standalone band only).
export type CrumbDisplayItem =
    | { kind: 'crumb'; crumb: AncestorCrumb }
    | { kind: 'ellipsis' };

// ── Shared stylesheet (inject-once) ─────────────────────────────────────────────

let stylesInjected = false;

/**
 * Inject the shared breadcrumb stylesheet once. Holds both the crumb-button
 * styling (shared by standalone + inline) and the standalone-only positioning
 * rules (`#quarto-content` offset-parent, `.q2-breadcrumb-chip` pill). The
 * positioning rules are class-scoped and harmless when only the inline crumbs
 * are present.
 */
export function ensureBreadcrumbStyles(): void {
    if (stylesInjected || typeof document === 'undefined') return;
    stylesInjected = true;
    const style = document.createElement('style');
    style.setAttribute('data-q2-breadcrumb', '1');
    style.textContent = CSS;
    document.head.appendChild(style);
}

const CSS = `
/* Phase 3: make #quarto-content the standalone chip's offset-parent. This
   creates a stacking context on the page-columns grid container. Risk is LOW:
   no position:fixed children inside #quarto-content; sidebar/TOC/overlays have
   their own stacking contexts. The chip's z-index:50 paints above
   sidebar(z≈1)/main(z=0). Only relevant to the standalone chip. */
#quarto-content { position: relative; }

.q2-breadcrumb-chip {
    display: flex;
    align-items: center;
    gap: 0;
    pointer-events: auto;
    /* No horizontal padding: chipLeft + ◀ width + band width must sum exactly to
       surfaceLeft so the crumb row's right edge meets the pivot
       (computeChipGeometry pins it there). Vertical breathing only. */
    padding: 1px 0;
    /* G13: opaque pill — a very faint cool blue-grey. */
    background: rgb(243, 247, 250);
    border-radius: 4px;
    box-shadow: 0 1px 3px rgba(0,0,0,0.12);
}
/* Fixed-width crumb band (standalone): [◀.right, surfaceLeft]. Crumbs flex to
   fill it; width is set inline. */
.q2-breadcrumb-crumbs {
    display: flex;
    align-items: center;
    gap: 0;
    overflow: hidden;
}
/* Inline band (in the rich-text toolbar): natural-width, no flex-fill, no
   ellipsize — the crumbs size to content and flow after the toolbar buttons. */
.q2-breadcrumb-crumbs.q2-bc-inline {
    overflow: visible;
    width: auto;
}
.q2-crumb {
    border: none;
    background: transparent;
    font-size: 12px;
    padding: 1px 3px;
    cursor: pointer;
    color: inherit;
    line-height: 1.4;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    text-align: center;
    /* Share the band equally; shrink below content width if needed (standalone). */
    flex: 1 1 0;
    min-width: 0;
}
/* Inline crumbs size to content rather than sharing a fixed band. */
.q2-bc-inline .q2-crumb {
    flex: 0 0 auto;
}
.q2-crumb-current {
    font-weight: bold;
    text-decoration: underline;
}
.q2-crumb:not(.q2-crumb-current):hover {
    text-decoration: underline;
}
.q2-breadcrumb-out,
.q2-breadcrumb-in {
    border: none;
    background: transparent;
    font-size: 11px;
    padding: 1px 4px;
    cursor: pointer;
    color: #555;
    line-height: 1.4;
    border-radius: 3px;
    flex-shrink: 0;
}
.q2-breadcrumb-out:hover,
.q2-breadcrumb-in:hover {
    background: rgba(0,0,0,0.08);
}
.q2-crumb-cat-container { color: #4f46e5; }
.q2-crumb-cat-list      { color: #15803d; }
.q2-crumb-cat-quote     { color: #b45309; }
.q2-crumb-cat-leaf-text { color: #0284c7; }
.q2-crumb-cat-embed     { color: #0f766e; }
.q2-breadcrumb-future   { opacity: 0.4; }
.q2-crumb-ellipsis {
    font-size: 12px;
    padding: 1px 3px;
    color: #888;
    line-height: 1.4;
    flex: 0 0 auto;
    user-select: none;
}
`;

// MIN_GLYPH_W is re-exported by BreadcrumbChip (geometry owner); the ◀/▶ buttons
// only need it for their fixed-width inline style in the standalone band.
const MIN_GLYPH_W = 16;

// ── Component ──────────────────────────────────────────────────────────────────

export interface BreadcrumbCrumbsProps {
    /** Items to render in the crumb band (already ellipsized for standalone; the
     *  full path for inline). */
    displayItems: CrumbDisplayItem[];
    /** 'standalone' (floating pill, fixed band) | 'inline' (toolbar row, natural). */
    layout: 'standalone' | 'inline';
    /** Standalone only: the fixed band width (px) the crumbs flex to fill. */
    bandWidth?: number;
}

/**
 * The crumb trio (◀ · band · ▶ · future placeholder), as a fragment. Reads the
 * nesting handlers off PreviewContext; injects the shared stylesheet.
 */
export function BreadcrumbCrumbs({
    displayItems,
    layout,
    bandWidth,
}: BreadcrumbCrumbsProps): React.ReactElement {
    ensureBreadcrumbStyles();
    const ctx = useContext(PreviewContext);
    const platform = detectPlatform();
    const outTip = platform === 'mac' ? 'Out (⌘⌃←)' : 'Out (Alt+Shift+←)';
    const inTip = platform === 'mac' ? 'In (⌘⌃→)' : 'In (Alt+Shift+→)';
    const inline = layout === 'inline';

    return (
        <>
            {/* ◀ out-arrow */}
            <button
                type="button"
                className="q2-breadcrumb-out"
                title={outTip}
                aria-label={outTip}
                style={inline ? undefined : { minWidth: `${MIN_GLYPH_W}px`, maxWidth: `${MIN_GLYPH_W}px`, flex: '0 0 auto' }}
                onPointerDown={(e) => e.preventDefault()}
                onClick={(e) => { e.stopPropagation(); ctx?.requestNestingMove?.('out'); }}
            >◀</button>
            {/* Crumb band */}
            <div
                className={`q2-breadcrumb-crumbs${inline ? ' q2-bc-inline' : ''}`}
                style={!inline && bandWidth != null ? { width: `${bandWidth}px`, flex: '0 0 auto' } : undefined}
            >
                {displayItems.map((item, idx) => {
                    if (item.kind === 'ellipsis') {
                        return (
                            <span
                                key={`ellipsis-${idx}`}
                                className="q2-crumb-ellipsis"
                                aria-hidden="true"
                            >…</span>
                        );
                    }
                    const c = item.crumb;
                    return (
                        <button
                            key={`${c.r0}-${c.r1}`}
                            type="button"
                            className={[
                                'q2-crumb',
                                `q2-crumb-cat-${c.category}`,
                                c.isCurrent ? 'q2-crumb-current' : '',
                            ].filter(Boolean).join(' ')}
                            title={c.label}
                            aria-label={c.label}
                            aria-current={c.isCurrent ? 'true' : undefined}
                            onPointerDown={(e) => e.preventDefault()}
                            onClick={(e) => { e.stopPropagation(); ctx?.requestNestingSelect?.(c.r0, c.r1); }}
                        >{c.abbrev}</button>
                    );
                })}
            </div>
            {/* ▶ in-arrow + future-crumb placeholder */}
            <button
                type="button"
                className="q2-breadcrumb-in"
                title={inTip}
                aria-label={inTip}
                style={{ flex: '0 0 auto' }}
                onPointerDown={(e) => e.preventDefault()}
                onClick={(e) => { e.stopPropagation(); ctx?.requestNestingMove?.('in'); }}
            >▶</button>
            <span className="q2-breadcrumb-future" />
        </>
    );
}
