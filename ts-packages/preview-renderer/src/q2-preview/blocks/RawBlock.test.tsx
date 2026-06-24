// @vitest-environment jsdom
import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/react';
import { RawBlock } from './RawBlock';
import type { NodeArgs, RawBlock as RawBlockType } from '../../framework';

/**
 * bd-xfw2omlt: a `{{< video >}}` on a reveal slide produces a RawBlock
 * `<iframe class="r-stretch">`. React injects raw HTML through a wrapper
 * `<div dangerouslySetInnerHTML>`, so the iframe ends up at
 * `section > div > iframe` — and reveal's stretch selector
 * (`section > .r-stretch`, direct children only) misses it. RawBlock mirrors a
 * root-level `r-stretch` onto its wrapper div so reveal stretches the wrapper
 * (the iframe then fills it via quarto-reveal.css).
 */

const raw = (content: string): RawBlockType => ({ t: 'RawBlock', c: ['html', content] });

function renderRaw(content: string) {
    const args = { node: raw(content), setLocalAst: () => {} } as NodeArgs<RawBlockType>;
    return render(<RawBlock {...args} />);
}

describe('RawBlock reveal stretch mirroring', () => {
    it('mirrors r-stretch from a reveal video iframe onto the wrapper div', () => {
        const { container } = renderRaw(
            '<iframe class="r-stretch" data-external="1" src="https://www.youtube.com/embed/x"></iframe>',
        );
        const div = container.firstElementChild as HTMLElement;
        expect(div.tagName).toBe('DIV');
        expect(div.classList.contains('r-stretch')).toBe(true);
        expect(div.querySelector('iframe')).not.toBeNull();
    });

    it('does not add r-stretch to the html (non-reveal) video wrapper', () => {
        const { container } = renderRaw(
            '<div class="quarto-video ratio ratio-16x9"><iframe src="https://www.youtube.com/embed/x"></iframe></div>',
        );
        const div = container.firstElementChild as HTMLElement;
        expect(div.classList.contains('r-stretch')).toBe(false);
    });

    it('does not add r-stretch to plain raw html', () => {
        const { container } = renderRaw('<p>hello</p>');
        const div = container.firstElementChild as HTMLElement;
        expect(div.classList.contains('r-stretch')).toBe(false);
    });
});
