/**
 * The rich-text editor is the DEFAULT edit surface in q2 preview; only an
 * explicit `?richText=0` opts out (bd-q9lyghv2 follow-up). These assert that
 * default-on / opt-out contract.
 */

import { describe, it, expect } from 'vitest';
import { parseRichTextParam } from './PreviewApp';

describe('parseRichTextParam — rich text is the default', () => {
  it('defaults ON when no query string is present', () => {
    expect(parseRichTextParam('')).toBe(true);
  });

  it('defaults ON when the richText param is absent', () => {
    expect(parseRichTextParam('?page=index.qmd')).toBe(true);
  });

  it('stays ON for ?richText=1', () => {
    expect(parseRichTextParam('?richText=1')).toBe(true);
  });

  it('turns OFF only for the explicit ?richText=0 opt-out', () => {
    expect(parseRichTextParam('?richText=0')).toBe(false);
  });

  it('keeps ON for any other value (only "0" disables)', () => {
    expect(parseRichTextParam('?richText=true')).toBe(true);
    expect(parseRichTextParam('?richText=')).toBe(true);
  });

  it('works mixed with other params', () => {
    expect(parseRichTextParam('?page=a.qmd&richText=0&nestingCursor=1')).toBe(false);
    expect(parseRichTextParam('?page=a.qmd&nestingCursor=1')).toBe(true);
  });
});
