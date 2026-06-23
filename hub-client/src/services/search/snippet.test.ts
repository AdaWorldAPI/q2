import { describe, it, expect } from 'vitest';
import { buildSnippet } from './snippet';

/** Concatenate segment texts back into the rendered string. */
function rendered(segs: ReturnType<typeof buildSnippet>): string {
  return segs.map((s) => s.text).join('');
}
/** The substrings that would be wrapped in <mark>. */
function marks(segs: ReturnType<typeof buildSnippet>): string[] {
  return segs.filter((s) => s.match).map((s) => s.text);
}

describe('buildSnippet', () => {
  it('marks the matched term', () => {
    const segs = buildSnippet('the collaborative editor', ['collaborative']);
    expect(marks(segs)).toEqual(['collaborative']);
  });

  it('includes surrounding context around the match', () => {
    const content = 'lorem ipsum dolor sit amet target consectetur adipiscing elit done';
    const segs = buildSnippet(content, ['target'], { context: 15 });
    const out = rendered(segs);
    expect(out).toContain('target');
    expect(out).toContain('amet');         // preceding context
    expect(out).toContain('consectetur');  // following context
    expect(out.length).toBeLessThan(content.length);
  });

  it('matches case-insensitively but preserves original casing in output', () => {
    const segs = buildSnippet('The QUICK brown fox', ['quick']);
    expect(marks(segs)).toEqual(['QUICK']);
  });

  it('highlights multiple terms within the window', () => {
    const segs = buildSnippet('alpha beta gamma', ['alpha', 'gamma']);
    expect(marks(segs)).toEqual(['alpha', 'gamma']);
  });

  it('prepends an ellipsis when the window starts mid-content', () => {
    const content = 'a b c d e f g h i j target k l m';
    const segs = buildSnippet(content, ['target'], { context: 6 });
    expect(rendered(segs).startsWith('…')).toBe(true);
  });

  it('returns a leading slice (no marks) when no term matches', () => {
    const content = 'this content has none of the searched words at all here';
    const segs = buildSnippet(content, ['absent'], { maxLength: 20 });
    expect(marks(segs)).toEqual([]);
    expect(rendered(segs).length).toBeLessThanOrEqual(21); // +1 for possible ellipsis
  });

  it('returns an empty array for empty content', () => {
    expect(buildSnippet('', ['x'])).toEqual([]);
  });

  it('does not mark anything when terms is empty', () => {
    const segs = buildSnippet('some content here', []);
    expect(marks(segs)).toEqual([]);
  });
});
