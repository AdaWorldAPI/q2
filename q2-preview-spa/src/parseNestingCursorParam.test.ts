/**
 * The hierarchical block navigator (BreadcrumbChip) is now the DEFAULT in
 * q2 preview, matching hub-client (which defaults the `unlockNestingCursor`
 * preference on). Only an explicit `?nestingCursor=0` opts out (bd-9x3zbuj8).
 * These assert that default-on / opt-out contract.
 *
 * Note: the navigator self-gates on an active edit target, so it only becomes
 * visible when actually editing — i.e. under `q2 preview --allow-edit`.
 */

import { describe, it, expect } from 'vitest';
import { parseNestingCursorParam } from './PreviewApp';

describe('parseNestingCursorParam — the nesting navigator is the default', () => {
  it('defaults ON when no query string is present', () => {
    expect(parseNestingCursorParam('')).toBe(true);
  });

  it('defaults ON when the nestingCursor param is absent', () => {
    expect(parseNestingCursorParam('?page=index.qmd')).toBe(true);
  });

  it('stays ON for ?nestingCursor=1', () => {
    expect(parseNestingCursorParam('?nestingCursor=1')).toBe(true);
  });

  it('turns OFF only for the explicit ?nestingCursor=0 opt-out', () => {
    expect(parseNestingCursorParam('?nestingCursor=0')).toBe(false);
  });

  it('keeps ON for any other value (only "0" disables)', () => {
    expect(parseNestingCursorParam('?nestingCursor=true')).toBe(true);
    expect(parseNestingCursorParam('?nestingCursor=')).toBe(true);
  });

  it('works mixed with other params', () => {
    expect(parseNestingCursorParam('?page=a.qmd&nestingCursor=0&richText=1')).toBe(false);
    expect(parseNestingCursorParam('?page=a.qmd&richText=0')).toBe(true);
  });
});
