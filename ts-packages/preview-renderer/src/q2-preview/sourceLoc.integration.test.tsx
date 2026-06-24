/**
 * q2-preview emits `data-loc` source-location attributes on block leaf
 * elements (bd-9kzfi). These power editor↔preview scroll sync the same
 * way the HTML preview (`MorphIframe`) does — `findElementForLine`
 * matches the `data-loc="fileId:startLine:startCol-endLine:endCol"`
 * format, so q2-preview must stamp the same shape from each node's `l`
 * field.
 */

import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/react';
import { Ast } from '../framework';
import type { PandocAST } from '../framework';
import { previewRegistry } from './registry';

function mount(blocks: unknown[]) {
    const ast: PandocAST = {
        'pandoc-api-version': [1, 23, 0],
        meta: {},
        blocks: blocks as never,
    };
    return render(
        <Ast
            astJson={JSON.stringify(ast)}
            currentFilePath="/project/test.qmd"
            onNavigateToDocument={() => {}}
            setAst={() => {}}
            registry={previewRegistry}
        />,
    );
}

const STR = (c: string) => ({ t: 'Str', c });
const loc = (sl: number, sc: number, el: number, ec: number) => ({
    f: 0,
    b: { o: 0, l: sl, c: sc },
    e: { o: 0, l: el, c: ec },
});

describe('q2-preview data-loc emission on block leaves', () => {
    it('stamps data-loc on a Para → <p>', () => {
        const { container } = mount([
            { t: 'Para', c: [STR('hello')], l: loc(3, 1, 3, 6) },
        ]);
        const p = container.querySelector('p');
        expect(p?.getAttribute('data-loc')).toBe('0:3:1-3:6');
    });

    it('stamps data-loc on a Header alongside id/class props', () => {
        const { container } = mount([
            {
                t: 'Header',
                c: [2, ['intro', ['unnumbered'], []], [STR('Intro')]],
                l: loc(1, 1, 1, 8),
            },
        ]);
        const h2 = container.querySelector('h2');
        expect(h2?.getAttribute('id')).toBe('intro');
        expect(h2?.getAttribute('class')).toBe('unnumbered');
        expect(h2?.getAttribute('data-loc')).toBe('0:1:1-1:8');
    });

    it('stamps data-loc on an un-highlighted CodeBlock → <pre>', () => {
        const { container } = mount([
            {
                t: 'CodeBlock',
                c: [['', [], []], 'x <- 1'],
                l: loc(5, 1, 7, 4),
            },
        ]);
        const pre = container.querySelector('pre');
        expect(pre?.getAttribute('data-loc')).toBe('0:5:1-7:4');
    });

    it('stamps data-loc on a Div → <div>', () => {
        const { container } = mount([
            {
                t: 'Div',
                c: [['', [], []], [{ t: 'Para', c: [STR('x')] }]],
                l: loc(10, 1, 12, 4),
            },
        ]);
        const div = container.querySelector('div[data-loc]');
        expect(div?.getAttribute('data-loc')).toBe('0:10:1-12:4');
    });

    it('stamps data-loc on a BlockQuote → <blockquote>', () => {
        const { container } = mount([
            {
                t: 'BlockQuote',
                c: [{ t: 'Para', c: [STR('q')] }],
                l: loc(2, 1, 2, 5),
            },
        ]);
        expect(
            container.querySelector('blockquote')?.getAttribute('data-loc'),
        ).toBe('0:2:1-2:5');
    });

    it('omits data-loc when the node carries no `l` field', () => {
        const { container } = mount([{ t: 'Para', c: [STR('hi')] }]);
        expect(container.querySelector('p')?.hasAttribute('data-loc')).toBe(
            false,
        );
    });
});
