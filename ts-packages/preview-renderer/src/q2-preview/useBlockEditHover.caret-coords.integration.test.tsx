/**
 * Caret-at-click capture wiring (bd-q9lyghv2).
 *
 * When a mouse click activates a block for rich-text editing, the activating
 * click's viewport coordinates must be stashed on `pendingClickCoordsRef` so the
 * editor can place the caret at the clicked position (instead of end-of-block) at
 * mount. Keyboard and touch activation must NOT set coords (they fall back to
 * end-of-block).
 *
 * jsdom note: PointerEvent does not honour pointerType/clientX/clientY from the
 * constructor init dict — force them via Object.defineProperty (see ptrEvent).
 */

import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, cleanup, fireEvent } from '@testing-library/react';
import React, { useRef } from 'react';
import { PreviewContext } from './PreviewContext';
import type { PreviewContextValue } from './PreviewContext';
import { useBlockEditHover } from './useBlockEditHover';

afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.restoreAllMocks();
});

function ptrEvent(
    type: string,
    opts: PointerEventInit & { clientX?: number; clientY?: number } = {},
): Event {
    const PE = (window as any).PointerEvent ?? Event;
    const evt = new PE(type, { bubbles: true, cancelable: true, ...opts });
    for (const [key, val] of Object.entries({
        ...(opts.pointerType !== undefined ? { pointerType: opts.pointerType } : {}),
        ...(opts.clientX !== undefined ? { clientX: opts.clientX } : {}),
        ...(opts.clientY !== undefined ? { clientY: opts.clientY } : {}),
    } as Record<string, unknown>)) {
        Object.defineProperty(evt, key, { value: val, configurable: true });
    }
    return evt;
}

const POOL_ENTRY_5 = { t: 0, r: [100, 200] as [number, number], d: 0 };
const POOL_WITH_5: unknown[] = [
    ...Array.from({ length: 5 }, () => null),
    POOL_ENTRY_5,
];

const MOCK_RECT: DOMRect = {
    width: 200, height: 40, top: 100, bottom: 140,
    left: 0, right: 200, x: 0, y: 100, toJSON: () => ({}),
};

function Inner() {
    const { hostProps, stylesheet } = useBlockEditHover();
    return (
        <div {...hostProps} data-testid="host">
            {stylesheet}
            <p data-block-pool-id="5" tabIndex={-1} data-testid="block5">block 5</p>
        </div>
    );
}

/** Mount the hover host, exposing the live pendingClickCoordsRef to the test. */
function mountHost() {
    const setEditTarget = vi.fn();
    let coordsRef!: React.MutableRefObject<{ x: number; y: number } | null>;
    function Host() {
        coordsRef = useRef<{ x: number; y: number } | null>(null);
        const ctx: PreviewContextValue = {
            currentFilePath: '/project/test.qmd',
            setEditTarget,
            pool: POOL_WITH_5,
            content: '',
            pendingClickCoordsRef: coordsRef,
        };
        return (
            <PreviewContext.Provider value={ctx}>
                <Inner />
            </PreviewContext.Provider>
        );
    }
    const utils = render(<Host />);
    return { ...utils, setEditTarget, getCoords: () => coordsRef.current };
}

describe('useBlockEditHover — caret-at-click coord capture', () => {
    it('stashes the click viewport coords on mouse pointerup activation', () => {
        const { getByTestId, setEditTarget, getCoords } = mountHost();
        const block = getByTestId('block5');
        vi.spyOn(block, 'getBoundingClientRect').mockReturnValue(MOCK_RECT);

        fireEvent(block, ptrEvent('pointerdown', { pointerType: 'mouse', clientX: 42, clientY: 117 }));
        fireEvent(block, ptrEvent('pointerup', { pointerType: 'mouse', clientX: 42, clientY: 117 }));

        // Activation happened…
        expect(setEditTarget).toHaveBeenCalledOnce();
        // …and the click coords were captured for the editor to consume.
        expect(getCoords()).toEqual({ x: 42, y: 117 });
    });

    it('does NOT stash coords on keyboard (Enter) activation', () => {
        const { getByTestId, setEditTarget, getCoords } = mountHost();
        const host = getByTestId('host');
        const block = getByTestId('block5');
        vi.spyOn(block, 'getBoundingClientRect').mockReturnValue(MOCK_RECT);

        // Hover sets hoveredRef; Enter activates via the keyboard path (no coords).
        fireEvent(block, ptrEvent('pointermove', { pointerType: 'mouse' }));
        fireEvent.keyDown(host, { key: 'Enter' });

        expect(setEditTarget).toHaveBeenCalledOnce();
        expect(getCoords()).toBeNull();
    });

    it('does NOT stash coords on touch hold activation', () => {
        vi.useFakeTimers();
        Object.defineProperty(HTMLElement.prototype, 'setPointerCapture', {
            value: vi.fn(), writable: true, configurable: true,
        });
        try {
            const { getByTestId, setEditTarget, getCoords } = mountHost();
            const block = getByTestId('block5');
            vi.spyOn(block, 'getBoundingClientRect').mockReturnValue(MOCK_RECT);

            fireEvent(block, ptrEvent('pointerdown', { pointerType: 'touch', clientX: 10, clientY: 10 }));
            vi.advanceTimersByTime(500);

            expect(setEditTarget).toHaveBeenCalledOnce();
            expect(getCoords()).toBeNull();
        } finally {
            delete (HTMLElement.prototype as any).setPointerCapture;
        }
    });
});
