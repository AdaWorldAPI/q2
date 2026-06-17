/**
 * RevealDeck deck-chrome parity (bd-n2w0sxgd, F3 of the
 * 2026-06-17 preview↔render parity audit).
 *
 * `q2 render` places the deck-level footer + logo as DIRECT children of
 * `.reveal`, OUTSIDE `.slides`, and adds `has-logo` to `.reveal`
 * (crates/quarto-core/src/revealjs/assemble.rs::render_revealjs_document +
 * footer_logo_html). `q2 preview`'s `RevealDeck` must mirror that. The markup
 * is pre-rendered by the `reveal-footer-logo` transform into the
 * `rendered.reveal.{footer,logo}` meta slots (which run in the q2-preview
 * pipeline), so the React side only reads + places it.
 *
 * `<Deck>` from `@revealjs/react` renders children only inside `.slides`, so the
 * chrome is injected into the `.reveal` element imperatively via `useReveal()` →
 * `getRevealElement()` (the `RevealChrome` component). These tests drive that
 * component directly with a fake `RevealContext`, so they assert the exact DOM
 * mutation without booting reveal.js.
 */

import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/react';
import { RevealContext } from '@revealjs/react';
import { RevealChrome, revealChromeFromMeta } from './RevealDeck';

const metaString = (c: string) => ({ t: 'MetaString', c });
const metaMap = (entries: Record<string, unknown>) => ({
    t: 'MetaMap',
    c: Object.entries(entries).map(([key, value]) => ({ key, value })),
});

/** Top-level meta carrying `rendered.reveal.{footer,logo}` slots. */
function metaWithReveal(slots: { footer?: string; logo?: string }) {
    const reveal: Record<string, unknown> = {};
    if (slots.footer !== undefined) reveal.footer = metaString(slots.footer);
    if (slots.logo !== undefined) reveal.logo = metaString(slots.logo);
    return { rendered: metaMap({ reveal: metaMap(reveal) }) };
}

const FOOTER_HTML =
    '<div class="footer footer-default">© 2026 Example Corp — <a href="https://example.com">example.com</a></div>';
const LOGO_HTML = '<img class="slide-logo" src="logo.svg">';

/** Render `RevealChrome` against a real `.reveal`/`.slides` element via a fake
 *  reveal API, returning that `.reveal` element for assertions. */
function renderChrome(props: { footerHtml?: string; logoHtml?: string }) {
    const reveal = document.createElement('div');
    reveal.className = 'reveal';
    const slides = document.createElement('div');
    slides.className = 'slides';
    reveal.appendChild(slides);
    document.body.appendChild(reveal);
    const fakeApi = { getRevealElement: () => reveal } as never;
    const utils = render(
        <RevealContext.Provider value={fakeApi}>
            <RevealChrome {...props} />
        </RevealContext.Provider>,
    );
    return { reveal, slides, ...utils };
}

describe('revealChromeFromMeta — reads rendered.reveal.{footer,logo}', () => {
    it('extracts both slots', () => {
        const chrome = revealChromeFromMeta(
            metaWithReveal({ footer: FOOTER_HTML, logo: LOGO_HTML }) as never,
        );
        expect(chrome.footerHtml).toBe(FOOTER_HTML);
        expect(chrome.logoHtml).toBe(LOGO_HTML);
    });

    it('returns undefined slots when absent', () => {
        const chrome = revealChromeFromMeta({} as never);
        expect(chrome.footerHtml).toBeUndefined();
        expect(chrome.logoHtml).toBeUndefined();
    });
});

describe('RevealChrome — places footer/logo outside .slides, adds has-logo', () => {
    it('injects logo + footer as direct children of .reveal (outside .slides)', () => {
        const { reveal, slides } = renderChrome({
            footerHtml: FOOTER_HTML,
            logoHtml: LOGO_HTML,
        });

        const logo = reveal.querySelector('img.slide-logo');
        const footer = reveal.querySelector('div.footer.footer-default');
        expect(logo).not.toBeNull();
        expect(footer).not.toBeNull();
        // Direct children of `.reveal`, NOT nested in `.slides`.
        expect(logo!.parentElement).toBe(reveal);
        expect(footer!.parentElement).toBe(reveal);
        expect(slides.contains(logo!)).toBe(false);
        expect(slides.contains(footer!)).toBe(false);
        // Footer preserves inline markup (link survives).
        expect(footer!.querySelector('a')?.getAttribute('href')).toBe(
            'https://example.com',
        );
    });

    it('orders logo before footer (matches assemble.rs footer_logo_html)', () => {
        const { reveal } = renderChrome({ footerHtml: FOOTER_HTML, logoHtml: LOGO_HTML });
        const logo = reveal.querySelector('img.slide-logo')!;
        const footer = reveal.querySelector('div.footer')!;
        // logo precedes footer in document order
        expect(
            logo.compareDocumentPosition(footer) &
                Node.DOCUMENT_POSITION_FOLLOWING,
        ).toBeTruthy();
    });

    it('adds has-logo to .reveal only when a logo is present', () => {
        const withLogo = renderChrome({ footerHtml: FOOTER_HTML, logoHtml: LOGO_HTML });
        expect(withLogo.reveal.classList.contains('has-logo')).toBe(true);

        const footerOnly = renderChrome({ footerHtml: FOOTER_HTML });
        expect(footerOnly.reveal.classList.contains('has-logo')).toBe(false);
        expect(footerOnly.reveal.querySelector('div.footer')).not.toBeNull();
        expect(footerOnly.reveal.querySelector('img.slide-logo')).toBeNull();
    });

    it('injects nothing when both slots are absent', () => {
        const { reveal, slides } = renderChrome({});
        expect(reveal.children.length).toBe(1); // only `.slides`
        expect(reveal.firstElementChild).toBe(slides);
        expect(reveal.classList.contains('has-logo')).toBe(false);
    });

    it('removes injected chrome + has-logo on unmount', () => {
        const { reveal, unmount } = renderChrome({
            footerHtml: FOOTER_HTML,
            logoHtml: LOGO_HTML,
        });
        expect(reveal.querySelector('.slide-logo')).not.toBeNull();
        unmount();
        expect(reveal.querySelector('.slide-logo')).toBeNull();
        expect(reveal.querySelector('.footer')).toBeNull();
        expect(reveal.classList.contains('has-logo')).toBe(false);
    });
});
