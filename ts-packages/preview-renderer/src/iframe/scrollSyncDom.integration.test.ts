/**
 * Shared scroll-sync DOM helpers (bd-9kzfi). jsdom environment so a
 * real Document/querySelectorAll backs `findElementForLine`.
 */
import { describe, it, expect } from 'vitest';
import { parseDataLoc, findElementForLine } from './scrollSyncDom';

describe('parseDataLoc', () => {
    it('parses a well-formed data-loc into 1-based fields', () => {
        expect(parseDataLoc('0:3:1-5:18')).toEqual({
            fileId: 0,
            startLine: 3,
            startCol: 1,
            endLine: 5,
            endCol: 18,
        });
    });

    it('returns null for malformed input', () => {
        expect(parseDataLoc('not-a-loc')).toBeNull();
        expect(parseDataLoc('0:3:1-5')).toBeNull();
    });
});

describe('findElementForLine', () => {
    function docWith(html: string): Document {
        const doc = document.implementation.createHTMLDocument('t');
        doc.body.innerHTML = html;
        return doc;
    }

    it('matches the element whose line range contains the line', () => {
        const doc = docWith(
            '<p data-loc="0:1:1-1:5" id="a">a</p>' +
                '<p data-loc="0:3:1-3:5" id="b">b</p>',
        );
        expect(findElementForLine(doc, 3)?.id).toBe('b');
        expect(findElementForLine(doc, 1)?.id).toBe('a');
    });

    it('prefers the most specific (smallest range) enclosing element', () => {
        const doc = docWith(
            '<div data-loc="0:1:1-10:1" id="outer">' +
                '<p data-loc="0:4:1-4:9" id="inner">x</p>' +
                '</div>',
        );
        // Line 4 is inside both; the tighter <p> wins.
        expect(findElementForLine(doc, 4)?.id).toBe('inner');
        // Line 2 is only inside the outer div.
        expect(findElementForLine(doc, 2)?.id).toBe('outer');
    });

    it('falls back to the nearest preceding block past the last range', () => {
        // Cursor past every range (e.g. a fresh blank line at the end of the
        // document). Snaps to the closest block that starts at or before the
        // line, so end-of-document edits still scroll the preview.
        const doc = docWith(
            '<p data-loc="0:1:1-1:5" id="a">a</p>' +
                '<p data-loc="0:3:1-3:5" id="b">b</p>',
        );
        expect(findElementForLine(doc, 99)?.id).toBe('b');
    });

    it('falls back to the nearest block in a gap between ranges', () => {
        // Line 5 is between the two paragraphs (covered by neither); the
        // preceding block wins over the following one.
        const doc = docWith(
            '<p data-loc="0:1:1-2:5" id="a">a</p>' +
                '<p data-loc="0:8:1-8:5" id="b">b</p>',
        );
        expect(findElementForLine(doc, 5)?.id).toBe('a');
    });

    it('falls back to the first block for a line before all ranges', () => {
        const doc = docWith(
            '<p data-loc="0:5:1-5:5" id="a">a</p>' +
                '<p data-loc="0:9:1-9:5" id="b">b</p>',
        );
        expect(findElementForLine(doc, 1)?.id).toBe('a');
    });

    it('returns null only when there are no located elements at all', () => {
        const doc = docWith('<p>no data-loc here</p>');
        expect(findElementForLine(doc, 99)).toBeNull();
    });

    it('ignores elements with an unparseable data-loc', () => {
        const doc = docWith('<p data-loc="garbage" id="g">g</p>');
        expect(findElementForLine(doc, 1)).toBeNull();
    });
});
