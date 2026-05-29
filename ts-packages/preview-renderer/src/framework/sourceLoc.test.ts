import { describe, it, expect } from 'vitest';
import { dataLocProps } from './sourceLoc';

describe('dataLocProps', () => {
    it('formats the node `l` field as fileId:startLine:startCol-endLine:endCol', () => {
        const node = {
            t: 'Para',
            c: [],
            l: {
                f: 0,
                b: { o: 12, l: 3, c: 1 },
                e: { o: 41, l: 5, c: 18 },
            },
        };
        expect(dataLocProps(node)).toEqual({ 'data-loc': '0:3:1-5:18' });
    });

    it('uses the non-zero file id when present', () => {
        const node = {
            l: { f: 7, b: { o: 0, l: 1, c: 1 }, e: { o: 4, l: 1, c: 5 } },
        };
        expect(dataLocProps(node)).toEqual({ 'data-loc': '7:1:1-1:5' });
    });

    it('returns an empty object when the node has no `l` field', () => {
        expect(dataLocProps({ t: 'Para', c: [] })).toEqual({});
    });

    it('returns an empty object for null / non-object input', () => {
        expect(dataLocProps(null)).toEqual({});
        expect(dataLocProps(undefined)).toEqual({});
    });

    it('returns an empty object when `l` is malformed (missing begin/end)', () => {
        expect(dataLocProps({ l: { f: 0 } })).toEqual({});
        expect(dataLocProps({ l: { f: 0, b: { o: 0, l: 1, c: 1 } } })).toEqual({});
    });
});
