/**
 * PreviewRoot wires the iframe-safe code-copy handler (bd-wa2pgri8).
 *
 * The copy LOGIC is unit-tested in `utils/codeCopy.integration.test.ts`. This
 * test asserts the INTEGRATION contract: PreviewRoot installs the delegated
 * handler on its `previewHostRef` host (which wraps both the reveal and
 * plain-HTML branches), so a `.code-copy-button` click anywhere in the preview
 * content copies — and the handler is torn down on unmount.
 *
 * We inject the copy scaffold into the live host rather than depend on the
 * registry's CodeBlock/RawBlock rendering (a separate, already-shipped concern):
 * the contract under test is "the listener is bound to the right element."
 */

import { describe, it, expect, afterEach, beforeEach, vi } from 'vitest';
import { render, cleanup } from '@testing-library/react';
import { PreviewRoot } from './PreviewRoot';
import type { PreviewRootProps } from './PreviewRoot';

let writeText: ReturnType<typeof vi.fn>;

beforeEach(() => {
    writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
});

afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
});

function emptyAstJson(): string {
    return JSON.stringify({
        'pandoc-api-version': [1, 23, 0],
        meta: {},
        blocks: [{ t: 'Para', c: [{ t: 'Str', c: 'hi' }], s: 0 }],
        astContext: { p: [{ t: 0, r: [0, 3], d: 0 }] },
    });
}

function mountPreviewRoot() {
    const props: PreviewRootProps = {
        astJson: emptyAstJson(),
        untransformedAstJson: emptyAstJson(),
        renderedContent: 'hi\n',
        currentFilePath: '/test.qmd',
        assetManifest: {},
        setAst: () => {},
        onNavigateToDocument: () => {},
    };
    return render(<PreviewRoot {...props} />);
}

/** The `previewHostRef` host: the display:contents wrapper around preview content. */
function previewHost(container: HTMLElement): HTMLElement {
    const host = container.querySelector<HTMLElement>('div[style*="contents"]');
    if (!host) throw new Error('preview host (display:contents wrapper) not found');
    return host;
}

/** Append a code-copy scaffold (as the render pipeline emits it) into the host. */
function injectScaffold(host: HTMLElement): HTMLButtonElement {
    const div = document.createElement('div');
    div.innerHTML =
        '<div class="code-copy-outer-scaffold">' +
        '<div class="sourceCode"><pre><code>print("hi")</code></pre></div>' +
        '<button class="code-copy-button" aria-label="Copy code"><i class="bi"></i></button>' +
        '</div>';
    host.appendChild(div.firstElementChild!);
    return host.querySelector<HTMLButtonElement>('.code-copy-button')!;
}

describe('PreviewRoot code-copy wiring', () => {
    it('copies a code block when its copy button is clicked', async () => {
        const { container } = mountPreviewRoot();
        const button = injectScaffold(previewHost(container));

        button.click();
        await vi.waitFor(() => expect(writeText).toHaveBeenCalledWith('print("hi")'));
    });

    it('removes the handler on unmount (no leak)', () => {
        const { container, unmount } = mountPreviewRoot();
        const host = previewHost(container);
        const spy = vi.spyOn(host, 'removeEventListener');
        unmount();
        // The effect cleanup removes the capture-phase click listener.
        expect(spy).toHaveBeenCalledWith('click', expect.any(Function), true);
    });
});
