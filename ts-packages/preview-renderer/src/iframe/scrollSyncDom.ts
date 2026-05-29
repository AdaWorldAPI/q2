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
 */
export function findElementForLine(
    doc: Document,
    line: number,
): HTMLElement | null {
    const elements = doc.querySelectorAll('[data-loc]');
    let bestMatch: HTMLElement | null = null;
    let bestRangeSize = Infinity;

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
        }
    }

    return bestMatch;
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
