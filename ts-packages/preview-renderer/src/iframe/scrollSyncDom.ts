/**
 * Shared DOM helpers for editor↔preview scroll sync. Both the HTML
 * preview (`MorphIframe`) and the q2-preview iframe (`Q2PreviewIframe`)
 * map an editor line number onto a preview element via `data-loc`
 * attributes of the form `fileId:startLine:startCol-endLine:endCol`
 * (1-based), the same format the native HTML writer and q2-preview's
 * `dataLocProps` emit.
 */

/**
 * Parsed source location from a `data-loc` attribute.
 * Format: `fileId:startLine:startCol-endLine:endCol` (1-based).
 */
export interface SourceLocation {
    fileId: number;
    startLine: number;
    startCol: number;
    endLine: number;
    endCol: number;
}

/**
 * Parse a `data-loc` attribute string into a `SourceLocation`.
 * Returns null if the format is invalid.
 */
export function parseDataLoc(dataLoc: string): SourceLocation | null {
    const match = dataLoc.match(/^(\d+):(\d+):(\d+)-(\d+):(\d+)$/);
    if (!match) return null;
    return {
        fileId: parseInt(match[1], 10),
        startLine: parseInt(match[2], 10),
        startCol: parseInt(match[3], 10),
        endLine: parseInt(match[4], 10),
        endCol: parseInt(match[5], 10),
    };
}

/**
 * Find the best matching element for a given line number, preferring
 * the most specific (smallest line range) match that contains the line.
 *
 * When no element's range contains the line (a blank line between
 * blocks, or — most commonly — a fresh line past the last block at the
 * end of the document), falls back to the nearest located block: the
 * one starting closest at-or-before the line, or failing that the first
 * block after it. This keeps end-of-document edits scrolling the
 * preview instead of silently no-op'ing. Returns null only when the
 * document has no located elements at all.
 */
export function findElementForLine(
    doc: Document,
    line: number,
): HTMLElement | null {
    const elements = doc.querySelectorAll('[data-loc]');
    let bestMatch: HTMLElement | null = null;
    let bestRangeSize = Infinity;

    // Fallbacks for a line no range contains: nearest block at-or-before
    // (largest startLine ≤ line) preferred, else nearest block after
    // (smallest startLine > line).
    let precedingMatch: HTMLElement | null = null;
    let precedingStart = -Infinity;
    let followingMatch: HTMLElement | null = null;
    let followingStart = Infinity;

    for (const element of elements) {
        const dataLoc = element.getAttribute('data-loc');
        if (!dataLoc) continue;

        const loc = parseDataLoc(dataLoc);
        if (!loc) continue;

        if (line >= loc.startLine && line <= loc.endLine) {
            const rangeSize = loc.endLine - loc.startLine;
            if (rangeSize < bestRangeSize) {
                bestMatch = element as HTMLElement;
                bestRangeSize = rangeSize;
            }
        } else if (loc.startLine <= line && loc.startLine > precedingStart) {
            precedingMatch = element as HTMLElement;
            precedingStart = loc.startLine;
        } else if (loc.startLine > line && loc.startLine < followingStart) {
            followingMatch = element as HTMLElement;
            followingStart = loc.startLine;
        }
    }

    return bestMatch ?? precedingMatch ?? followingMatch;
}

/**
 * Check if an element is fully visible within the iframe viewport.
 * `win` is the iframe's own `contentWindow` (so `innerHeight` is the
 * preview viewport, not the host page's).
 */
export function isElementVisible(element: HTMLElement, win: Window): boolean {
    const rect = element.getBoundingClientRect();
    return rect.top >= 0 && rect.bottom <= win.innerHeight;
}

/**
 * Scroll ratio of an iframe document: 0 at the top, 1 at the bottom, and 0
 * for a document too short to scroll. `win`/`doc` are the iframe's own
 * contentWindow/contentDocument. Shared by MorphIframe and Q2PreviewIframe.
 */
export function computeScrollRatio(win: Window, doc: Document): number {
    const maxScroll = doc.documentElement.scrollHeight - win.innerHeight;
    if (maxScroll <= 0) return 0;
    return win.scrollY / maxScroll;
}
