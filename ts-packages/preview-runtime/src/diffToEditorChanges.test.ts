/**
 * Tests for diffToEditorChanges (bd-ov4gqk3m).
 *
 * The contract under test: the returned splice operations, applied **in
 * order against the evolving string** (exactly how
 * `applyEditorOperations` splices them into the Automerge text), must
 * transform `currentContent` into `targetContent`.
 */

import { describe, it, expect } from 'vitest';
import { diffToEditorChanges } from './diffToEditorChanges';
import type { EditorContentChange } from '@quarto/quarto-sync-client';

/** Mirror of the splice loop in quarto-sync-client's applyEditorOperations. */
function applyChanges(content: string, changes: EditorContentChange[]): string {
  let result = content;
  for (const change of changes) {
    result =
      result.slice(0, change.rangeOffset) +
      change.text +
      result.slice(change.rangeOffset + change.rangeLength);
  }
  return result;
}

describe('diffToEditorChanges', () => {
  it('returns empty array for identical content', () => {
    expect(diffToEditorChanges('hello', 'hello')).toEqual([]);
    expect(diffToEditorChanges('', '')).toEqual([]);
  });

  it('produces a single insertion with exact offsets', () => {
    const changes = diffToEditorChanges('hello', 'hello world');
    expect(changes).toEqual([{ rangeOffset: 5, rangeLength: 0, text: ' world' }]);
  });

  it('produces a single deletion with exact offsets', () => {
    const changes = diffToEditorChanges('hello world', 'hello');
    expect(changes).toEqual([{ rangeOffset: 5, rangeLength: 6, text: '' }]);
  });

  it('expresses later offsets against the evolving document', () => {
    // 'abc def ghi' → 'abcX def ghiY': after the first insert at offset 3,
    // the second insert lands at offset 12 in the *new* string (not 11).
    const current = 'abc def ghi';
    const target = 'abcX def ghiY';
    const changes = diffToEditorChanges(current, target);
    expect(applyChanges(current, changes)).toBe(target);
    expect(changes).toEqual([
      { rangeOffset: 3, rangeLength: 0, text: 'X' },
      { rangeOffset: 12, rangeLength: 0, text: 'Y' },
    ]);
  });

  it('round-trips a realistic QMD paragraph edit', () => {
    const current = '---\ntitle: Doc\n---\n\nFirst paragraph.\n\nSecond paragraph.\n';
    const target = '---\ntitle: Doc\n---\n\nFirst paragraph, *edited*.\n\nSecond paragraph.\n';
    const changes = diffToEditorChanges(current, target);
    expect(changes.length).toBeGreaterThan(0);
    expect(applyChanges(current, changes)).toBe(target);
  });

  it('round-trips mixed insert/delete/replace edits', () => {
    const cases: Array<[string, string]> = [
      ['hello world', 'world'],
      ['world', 'hello world'],
      ['hello', 'hallo'],
      ['abc def ghi', 'ABC def GHI'],
      ['completely different', 'totally new content'],
      ['', 'new content'],
      ['old content', ''],
      ['line1\nline2\nline3', 'line1\nlineX\nline3\nline4'],
      ['a\tb', 'a\t\tb'],
      ['line1\r\nline2', 'line1\nline2'],
    ];
    for (const [current, target] of cases) {
      const changes = diffToEditorChanges(current, target);
      expect(applyChanges(current, changes), `'${current}' → '${target}'`).toBe(target);
    }
  });

  it('round-trips unicode and emoji content (UTF-16 offsets)', () => {
    const cases: Array<[string, string]> = [
      ['hello 世界', 'hello 世界!'],
      ['hello 👋', 'hello 👋🌍'],
      ['naïve café', 'naïve, café'],
    ];
    for (const [current, target] of cases) {
      const changes = diffToEditorChanges(current, target);
      expect(applyChanges(current, changes), `'${current}' → '${target}'`).toBe(target);
    }
  });
});
