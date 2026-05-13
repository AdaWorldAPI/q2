import React from 'react';
import type { NodeAttributionIdentity } from '../framework';

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
 */
export const attributionStyles = `
    .q2-attr-wrap { position: relative; }
    .q2-attr-badge {
        display: inline-block;
        z-index: 10;
        font-size: 10px;
        line-height: 1;
        white-space: nowrap;
        padding: 2px 6px;
        border-radius: 3px;
        background: #fff;
        border: 1px solid var(--attr-color);
        color: var(--attr-color);
        font-weight: 600;
        pointer-events: none;
    }
    .q2-attr-badge-dot {
        display: inline-block;
        width: 6px;
        height: 6px;
        border-radius: 50%;
        margin-right: 3px;
        vertical-align: middle;
    }
    .q2-attr-badge-time {
        font-weight: 400;
        opacity: 0.7;
    }
`;
