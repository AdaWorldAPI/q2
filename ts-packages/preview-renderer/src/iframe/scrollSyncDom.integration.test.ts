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

    it('returns null when no element covers the line', () => {
        const doc = docWith('<p data-loc="0:1:1-1:5">a</p>');
        expect(findElementForLine(doc, 99)).toBeNull();
    });

    it('ignores elements with an unparseable data-loc', () => {
        const doc = docWith('<p data-loc="garbage" id="g">g</p>');
        expect(findElementForLine(doc, 1)).toBeNull();
    });
});
