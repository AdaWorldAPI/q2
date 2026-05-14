import React, { useCallback, useContext, useRef, useState } from 'react';
import {
    AttributionLookupContext,
    useNodeAttribution,
    type NodeAttributionIdentity,
} from './AttributionLookupContext';
// Shared with the CLI's `AttributionViewerTransform`, which `include_str!`s
// the same file at compile time. Editing `resources/attribution/viewer.css`
// updates both surfaces simultaneously. The virtual-module indirection
// is handled by `attributionViewerCssPlugin` in `vite.config.ts`: a
// plain `?raw` import of the out-of-tree path silently returns empty
// under Vitest. See `resources/attribution/README.md` for the contract.
import viewerCss from 'virtual:quarto-attribution-viewer-css';

/**
 * Format a Unix timestamp as a coarse relative-time string ("just
 * now", "3m ago", "2d ago"). Accepts both ms (Automerge) and seconds
 * (git blame) — the `< 1e12` heuristic catches anything before 2001
 * and treats it as seconds.
 */
export function formatRelativeTime(timestamp: number): string {
    const now = Date.now();
    const tsMs = timestamp < 1e12 ? timestamp * 1000 : timestamp;
    const diffSec = Math.floor((now - tsMs) / 1000);
    if (diffSec < 60) return 'just now';
    const diffMin = Math.floor(diffSec / 60);
    if (diffMin < 60) return `${diffMin}m ago`;
    const diffHr = Math.floor(diffMin / 60);
    if (diffHr < 24) return `${diffHr}h ago`;
    const diffDay = Math.floor(diffHr / 24);
    return `${diffDay}d ago`;
}

/** Floating tooltip rendered on hover, coloured to match the author. */
export function AttributionBadge({
    record,
    style,
}: {
    record: NodeAttributionIdentity;
    style?: React.CSSProperties;
}) {
    return (
        <span
            className="q2-attr-badge"
            style={{ ['--attr-color' as never]: record.color, ...style } as React.CSSProperties}
            data-attr-actor={record.actor}
        >
            <span className="q2-attr-badge-dot" style={{ backgroundColor: record.color }} />
            {record.name}{' '}
            <span className="q2-attr-badge-time">{formatRelativeTime(record.time)}</span>
        </span>
    );
}

/**
 * Style block injected once per attribution-bearing AST render by
 * q2-debug's `AstRenderer`. Off-path renders skip the injection
 * entirely so the iframe document tree stays byte-identical to
 * pre-attribution.
 *
 * Single source of truth is `resources/attribution/viewer.css` —
 * the CLI's `AttributionViewerTransform` reads the same file via
 * `include_str!`. Both surfaces re-pick up edits without manual sync.
 */
export const attributionStyles = viewerCss;

/**
 * Wrap `children` in a `.q2-attr-wrap` element when the AST node has
 * resolved attribution; pass through unchanged otherwise.
 *
 * `as` selects a `<div>` (block-level wrapper) or `<span>` (inline-level
 * wrapper) — the only structural divergence between Block/Inline
 * dispatchers in q2-debug and q2-preview. `data-sid` is the lookup key
 * the event-delegated badge in `useAttributionHover` uses to find the
 * record for the hovered wrap.
 *
 * The wrapper paints body text in the author's colour (descendants
 * inherit via the cascade). The CLI's auto-injected viewer JS
 * (`resources/attribution/viewer.js`) does the same thing for static
 * HTML output via a `querySelectorAll` + `style.color` pass, so both
 * surfaces look the same on attributed regions.
 *
 * `node` is typed `unknown` to absorb the structural mismatch between
 * the framework's loose `{ s?: number }` lookup key and the
 * dispatcher-side `BlockNode` / `InlineNode` / `CustomBlockNode` /
 * `CustomInlineNode` types — those types don't formally declare `s`
 * (the JSON serializer adds it). Off-path callers pass any node-like
 * value; the cast happens once, here.
 */
export function AttributionWrap({
    node,
    as,
    children,
}: {
    node: unknown;
    as: 'div' | 'span';
    children: React.ReactNode;
}) {
    const attribution = useNodeAttribution(node as { s?: number });
    if (!attribution) return <>{children}</>;
    const sid = (node as { s?: number }).s;
    const style = { color: attribution.color };
    if (as === 'div') {
        return (
            <div className="q2-attr-wrap" data-sid={sid} style={style}>
                {children}
            </div>
        );
    }
    return (
        <span className="q2-attr-wrap" data-sid={sid} style={style}>
            {children}
        </span>
    );
}

/**
 * Wire event-delegated hover state for `.q2-attr-wrap[data-sid]`
 * elements descended from the host element. Used by document-root
 * components (q2-debug `AstRenderer`, q2-preview `PreviewDocument`)
 * to mount the badge overlay and stylesheet once per render.
 *
 * Off-path (no `AttributionLookupContext` populated) `enabled` is
 * `false` and the returned `stylesheet` / `overlay` are `null`;
 * consumers skip the wiring and the DOM stays byte-identical to
 * pre-attribution. The `hostProps` object is `{}` when disabled, so
 * spreading it onto a host element is a no-op.
 */
export function useAttributionHover(): {
    enabled: boolean;
    hostProps: {
        onMouseOver?: React.MouseEventHandler;
        onMouseOut?: React.MouseEventHandler;
    };
    stylesheet: React.ReactNode;
    overlay: React.ReactNode;
} {
    const lookup = useContext(AttributionLookupContext);
    const lookupRef = useRef(lookup);
    lookupRef.current = lookup;

    const [hovered, setHovered] = useState<{
        record: NodeAttributionIdentity;
        rect: DOMRect;
    } | null>(null);

    const handleMouseOver = useCallback((e: React.MouseEvent) => {
        const ctx = lookupRef.current;
        if (!ctx) return;
        const target = e.target as HTMLElement;
        const wrap = target.closest('.q2-attr-wrap[data-sid]') as HTMLElement | null;
        if (!wrap) {
            setHovered(null);
            return;
        }
        const sid = Number(wrap.getAttribute('data-sid'));
        if (Number.isNaN(sid)) return;
        const record = ctx.get(sid);
        if (record) {
            setHovered({ record, rect: wrap.getBoundingClientRect() });
        }
    }, []);

    const handleMouseOut = useCallback((e: React.MouseEvent) => {
        const related = e.relatedTarget as HTMLElement | null;
        if (!related?.closest?.('.q2-attr-wrap[data-sid]')) {
            setHovered(null);
        }
    }, []);

    const enabled = lookup != null;
    const hostProps = enabled
        ? { onMouseOver: handleMouseOver, onMouseOut: handleMouseOut }
        : {};
    const stylesheet = enabled ? <style>{attributionStyles}</style> : null;
    const overlay = hovered ? (
        <AttributionBadge
            record={hovered.record}
            style={{
                position: 'fixed',
                top: hovered.rect.bottom + 2,
                left: hovered.rect.left,
            }}
        />
    ) : null;

    return { enabled, hostProps, stylesheet, overlay };
}
