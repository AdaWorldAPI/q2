/**
 * Unit tests for placeCaretFromClick (bd-q9lyghv2).
 *
 * Pure logic over a fake editor: posAtCoords resolves the document position from
 * viewport coordinates; on a hit we move the selection there and focus; on a miss
 * we do nothing and report false so the caller can keep its end-of-block default.
 *
 * Real geometry (posAtCoords against laid-out DOM) is verified end-to-end in a
 * browser — jsdom returns null/0 — so here we mock the view.
 */

import { describe, it, expect, vi } from 'vitest';
import { placeCaretFromClick } from './caretFromClick';

/** Build a fake tiptap editor recording the chained caret commands. */
function fakeEditor(posAtCoordsReturn: { pos: number; inside: number } | null) {
    const calls = { setTextSelection: [] as number[], focus: 0, run: 0 };
    const chain: any = {
        focus() { calls.focus++; return chain; },
        setTextSelection(pos: number) { calls.setTextSelection.push(pos); return chain; },
        run() { calls.run++; return true; },
    };
    const editor = {
        view: { posAtCoords: vi.fn().mockReturnValue(posAtCoordsReturn) },
        chain: () => chain,
    };
    return { editor, calls };
}

describe('placeCaretFromClick', () => {
    it('maps coords with posAtCoords and moves the selection to the hit pos', () => {
        const { editor, calls } = fakeEditor({ pos: 7, inside: 0 });

        const placed = placeCaretFromClick(editor as any, { x: 42, y: 117 });

        expect(placed).toBe(true);
        // posAtCoords takes {left, top} (ProseMirror's coordinate shape).
        expect(editor.view.posAtCoords).toHaveBeenCalledWith({ left: 42, top: 117 });
        expect(calls.setTextSelection).toEqual([7]);
        expect(calls.focus).toBeGreaterThanOrEqual(1);
        expect(calls.run).toBe(1);
    });

    it('returns false and moves nothing when posAtCoords misses (null)', () => {
        const { editor, calls } = fakeEditor(null);

        const placed = placeCaretFromClick(editor as any, { x: 1, y: 1 });

        expect(placed).toBe(false);
        expect(calls.setTextSelection).toEqual([]);
        expect(calls.run).toBe(0);
    });
});
