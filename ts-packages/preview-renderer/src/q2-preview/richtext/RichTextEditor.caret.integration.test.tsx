/**
 * RichTextEditor caret-at-click consume wiring (bd-q9lyghv2).
 *
 * At mount the editor must read-and-clear `pendingClickCoordsRef`. When coords
 * are present it places the caret via placeCaretFromClick (the helper, unit-
 * tested separately); when absent it leaves the autofocus:'end' default. Either
 * way the ref is consumed exactly once so a re-anchor remount cannot reuse a
 * stale click.
 *
 * Real posAtCoords geometry is browser-verified — here we mock the helper to
 * observe the wiring (was it called? with what? is the ref cleared?).
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/react';
import React, { useRef } from 'react';
import { PreviewContext } from '../PreviewContext';
import type { PreviewContextValue } from '../PreviewContext';
import type { ResolvedSource } from '../sourceIndex';
import { RichTextEditor } from './RichTextEditor';

// Observe the caret-placement call without needing real layout/geometry.
const placeCaretFromClick = vi.fn().mockReturnValue(true);
vi.mock('./caretFromClick', () => ({
    placeCaretFromClick: (...args: unknown[]) => placeCaretFromClick(...args),
}));

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

// A single "hello" paragraph: pool[0] spans the whole 5-byte source.
const CONTENT = 'hello';
const POOL = [{ t: 0, r: [0, 5] as [number, number], d: 0 }];
const SOURCE_NODE = { t: 'Para', c: [{ t: 'Str', c: 'hello' }], s: 0 } as unknown;
const RESOLVED: ResolvedSource = {
    sourceNode: SOURCE_NODE as ResolvedSource['sourceNode'],
    reachabilityClass: 'reachable' as ResolvedSource['reachabilityClass'],
    sourceEntry: { t: 0, r: [0, 5], d: 0 },
};

/** Mount RichTextEditor with a pre-seeded coords ref; expose it to the test. */
function mountEditor(initialCoords: { x: number; y: number } | null) {
    let coordsRef!: React.MutableRefObject<{ x: number; y: number } | null>;
    function Host() {
        coordsRef = useRef<{ x: number; y: number } | null>(initialCoords);
        const editDraftRef = useRef<string | null>(CONTENT);
        const ctx: PreviewContextValue = {
            currentFilePath: '/project/test.qmd',
            pool: POOL as PreviewContextValue['pool'],
            content: CONTENT,
            editDraftRef,
            pendingClickCoordsRef: coordsRef,
            setEditTarget: vi.fn(),
        };
        return (
            <PreviewContext.Provider value={ctx}>
                <RichTextEditor ctx={ctx} resolved={RESOLVED} />
            </PreviewContext.Provider>
        );
    }
    const utils = render(<Host />);
    return { ...utils, getCoords: () => coordsRef.current };
}

describe('RichTextEditor — caret-at-click consume', () => {
    it('consumes the coords ref and places the caret when coords are present', () => {
        const { getCoords } = mountEditor({ x: 42, y: 117 });

        // Helper invoked with the editor + the captured coords…
        expect(placeCaretFromClick).toHaveBeenCalledTimes(1);
        expect(placeCaretFromClick.mock.calls[0][1]).toEqual({ x: 42, y: 117 });
        // …and the ref is consumed (so a remount won't reuse a stale click).
        expect(getCoords()).toBeNull();
    });

    it('does not place the caret (and leaves end-of-block) when no coords present', () => {
        const { getCoords } = mountEditor(null);

        expect(placeCaretFromClick).not.toHaveBeenCalled();
        expect(getCoords()).toBeNull();
    });
});
