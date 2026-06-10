/**
 * Compute Automerge-compatible splice operations from a text diff.
 *
 * Moved here from hub-client's `utils/diffToMonacoEdits.ts` (bd-ov4gqk3m) so
 * both hub-client and the q2-preview SPA can turn an AST-rewrite result (a
 * whole new QMD string) into the `EditorContentChange[]` that
 * `applyEditorOperations` splices into the Automerge document. The
 * Monaco-coupled sibling (`diffToMonacoEdits`) stays in hub-client.
 */

import diff from 'fast-diff';
import type { EditorContentChange } from '@quarto/quarto-sync-client';

// fast-diff operation constants
const DIFF_DELETE = -1;
const DIFF_EQUAL = 0;
const DIFF_INSERT = 1;

/**
 * Compute Automerge-compatible splice operations to transform
 * `currentContent` into `targetContent`.
 *
 * Positions are byte offsets (`EditorContentChange.rangeOffset` /
 * `rangeLength`) expressed against the *evolving* document: each operation
 * assumes all previous operations in the array have already been applied,
 * matching how `applyEditorOperations` splices them in sequence.
 */
export function diffToEditorChanges(
  currentContent: string,
  targetContent: string,
): EditorContentChange[] {
  if (currentContent === targetContent) return [];

  const diffs = diff(currentContent, targetContent);
  const changes: EditorContentChange[] = [];
  let offset = 0;

  for (const [operation, text] of diffs) {
    if (operation === DIFF_EQUAL) {
      offset += text.length;
    } else if (operation === DIFF_DELETE) {
      changes.push({ rangeOffset: offset, rangeLength: text.length, text: '' });
      // offset stays the same: deleted chars are gone, next op starts at same position
    } else if (operation === DIFF_INSERT) {
      changes.push({ rangeOffset: offset, rangeLength: 0, text });
      offset += text.length; // advance past the newly inserted chars
    }
  }

  return changes;
}
