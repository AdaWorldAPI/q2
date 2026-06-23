/**
 * Unit tests for installCodeCopy — the iframe-safe code-copy handler that
 * makes `.code-copy-button` clicks actually copy in the q2-preview / hub-client
 * WASM iframe (bd-wa2pgri8). See codeCopy.ts for the delegation rationale.
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { installCodeCopy } from './codeCopy';

/** Build the scaffold the Rust CodeBlockRenderTransform emits, inside a host. */
function buildHost(codeHtml: string): { host: HTMLElement; button: HTMLElement } {
    const host = document.createElement('div');
    host.innerHTML = `
        <div class="code-copy-outer-scaffold">
            <div class="sourceCode"><pre><code>${codeHtml}</code></pre></div>
            <button class="code-copy-button" title="Copy to Clipboard" aria-label="Copy code"><i class="bi"></i></button>
        </div>`;
    document.body.appendChild(host);
    const button = host.querySelector<HTMLElement>('.code-copy-button')!;
    return { host, button };
}

let writeText: ReturnType<typeof vi.fn>;

beforeEach(() => {
    vi.useFakeTimers();
    writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
        value: { writeText },
        configurable: true,
    });
});

afterEach(() => {
    vi.useRealTimers();
    document.body.innerHTML = '';
});

describe('installCodeCopy', () => {
    it('copies the code text to the clipboard when a copy button is clicked', async () => {
        const { host, button } = buildHost('print("hi")');
        installCodeCopy(host);

        button.click();
        await vi.waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
        expect(writeText).toHaveBeenCalledWith('print("hi")');
    });

    it('flashes the checked class on success, reverting after 1s', async () => {
        const { host, button } = buildHost('x = 1');
        installCodeCopy(host);

        button.click();
        // writeText resolves on a microtask; flush it before asserting.
        await Promise.resolve();
        await Promise.resolve();
        expect(button.classList.contains('code-copy-button-checked')).toBe(true);

        vi.advanceTimersByTime(1000);
        expect(button.classList.contains('code-copy-button-checked')).toBe(false);
    });

    it('strips .code-annotation-* children from the copied text', async () => {
        const { host, button } = buildHost(
            'let y = 2 <span class="code-annotation-anchor">1</span>',
        );
        installCodeCopy(host);

        button.click();
        await vi.waitFor(() => expect(writeText).toHaveBeenCalled());
        // The annotation marker text must not be in the copied string.
        const copied = writeText.mock.calls[0][0] as string;
        expect(copied).toContain('let y = 2');
        expect(copied).not.toContain('1');
    });

    it('ignores clicks that are not on a copy button', () => {
        const { host } = buildHost('noop');
        installCodeCopy(host);

        host.querySelector('code')!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
        expect(writeText).not.toHaveBeenCalled();
    });

    it('stops pointer events on the copy button from reaching block handlers', () => {
        const { host, button } = buildHost('z');
        // A bubble-phase pointerup listener on an element BETWEEN host and the
        // button stands in for PreviewRoot's onPointerUp block-activation.
        const blockHandler = vi.fn();
        const scaffold = host.querySelector<HTMLElement>('.code-copy-outer-scaffold')!;
        scaffold.addEventListener('pointerup', blockHandler);
        installCodeCopy(host);

        button.dispatchEvent(new Event('pointerup', { bubbles: true }));
        expect(blockHandler).not.toHaveBeenCalled();

        // Control: a pointerup NOT on a copy button reaches the block handler.
        scaffold.dispatchEvent(new Event('pointerup', { bubbles: true }));
        expect(blockHandler).toHaveBeenCalledTimes(1);
    });

    it('cleanup removes the listener', () => {
        const { host, button } = buildHost('gone');
        const cleanup = installCodeCopy(host);
        cleanup();

        button.click();
        expect(writeText).not.toHaveBeenCalled();
    });

    it('does not throw when navigator.clipboard is unavailable', () => {
        Object.defineProperty(navigator, 'clipboard', { value: undefined, configurable: true });
        const { host, button } = buildHost('safe');
        installCodeCopy(host);
        expect(() => button.click()).not.toThrow();
    });
});
