/**
 * bd-dg8x84bu regression guard.
 *
 * RevealDeck imports reveal's CSS as side-effecting modules; the bundler hoists
 * them into the q2-preview SPA's single GLOBAL stylesheet, so anything not scoped
 * under `.reveal` leaks onto every preview document — including `format: html`
 * pages with no deck. Reveal's upstream `reset.css` is a global Meyer page reset
 * (`html, body, …, em, i, cite { font: inherit; … }`) that zeroed `font-style` on
 * emphasis and broke italics. We import the `.reveal`-scoped derivative instead.
 *
 * This test asserts the invariant the three imported files must satisfy: every
 * selector is scoped under `.reveal`, so none can leak globally.
 */

import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const revealCssDir = resolve(here, '../../../../resources/revealjs');

/** Split a CSS string into top-level selector lists, skipping comments/at-rules. */
function selectorsOf(css: string): string[] {
    const noComments = css.replace(/\/\*[\s\S]*?\*\//g, '');
    const selectors: string[] = [];
    // Match "<selector-list> {" blocks at the top level (these stylesheets are flat).
    for (const m of noComments.matchAll(/([^{}]+)\{[^}]*\}/g)) {
        const head = m[1].trim();
        if (head.startsWith('@')) continue; // at-rules (e.g. @keyframes headers)
        for (const sel of head.split(',')) {
            const s = sel.trim();
            if (s) selectors.push(s);
        }
    }
    return selectors;
}

/** A selector is deck-scoped if it is `.reveal`, or targets `.reveal`/its descendants. */
function isRevealScoped(sel: string): boolean {
    return /(^|[\s>+~(,])\.reveal\b/.test(sel) || sel === '.reveal';
}

/**
 * Does this selector LEAK onto generic document content? The dangerous shape is a
 * bare element-type selector (`em`, `html`, `div`, …) with no class/id/attr/pseudo
 * qualifier and no `.reveal` scope — i.e. the Meyer-reset shape. Reveal-namespaced
 * globals (`.r-overlay`, `html.reveal-full-page`) carry a class so they only match
 * reveal's own DOM; `@keyframes` steps (`from`/`to`/`50%`) are not selectors.
 */
function leaksToDocument(sel: string): boolean {
    if (/^(from|to|\d+%)$/.test(sel)) return false; // keyframe step, not a selector
    if (isRevealScoped(sel)) return false;
    return !/[.#[:]/.test(sel); // bare type selector(s) only → matches plain document elements
}

describe('reveal CSS imported by RevealDeck must not leak onto document content (bd-dg8x84bu)', () => {
    // These are exactly the files RevealDeck.tsx pulls into the global SPA bundle.
    // None may carry a bare element-type rule (e.g. the Meyer reset's `em {…}`)
    // that would restyle plain `format: html` preview content.
    for (const file of ['reset-scoped.css', 'reveal.css', 'quarto-reveal.css']) {
        it(`${file} has no global bare-element selectors`, () => {
            const css = readFileSync(resolve(revealCssDir, file), 'utf8');
            const leaks = selectorsOf(css).filter(leaksToDocument);
            expect(leaks, `these bare-element selectors would restyle format:html content:\n${leaks.join('\n')}`).toEqual([]);
        });
    }

    it('the scoped reset still resets emphasis elements inside a deck', () => {
        const css = readFileSync(resolve(revealCssDir, 'reset-scoped.css'), 'utf8');
        expect(css).toContain('.reveal em');
        expect(css).toContain('.reveal i');
    });

    it('the upstream global reset is NOT what gets scoped away (it still exists, unscoped)', () => {
        // Sanity anchor: reset.css remains the verbatim upstream Meyer reset (kept
        // for the vendored↔npm byte-identity check). It is intentionally global —
        // which is exactly why RevealDeck must not import it directly.
        const upstream = readFileSync(resolve(revealCssDir, 'reset.css'), 'utf8');
        expect(selectorsOf(upstream).some((s) => !isRevealScoped(s))).toBe(true);
    });
});
