// caretFromClick.ts (bd-q9lyghv2) — place the rich-text caret at the clicked spot.
//
// When a block is opened for rich-text editing by a MOUSE click, the original
// click event is already consumed by the time the tiptap editor mounts (the open
// goes through a React state update), so ProseMirror never gets to translate the
// click into a document position — the caret defaults to end-of-block, and only a
// SECOND click lands it correctly. We bridge that gap: the activation site stashes
// the click's viewport coordinates and the editor, at mount, replays them through
// `posAtCoords` to put the caret where the user actually clicked.
//
// This works because the editor renders the SAME visual text in the SAME measured
// box as the rendered block (same theme CSS), so the click's viewport coordinates
// land on the same glyph. Geometry correctness is browser-verified (jsdom returns
// null from posAtCoords); the unit tests here cover the hit/miss logic with a fake
// editor.

import type { Editor } from '@tiptap/core';

/**
 * Move the caret to the document position under the given viewport coordinates.
 *
 * @returns `true` if a position was resolved and the selection moved there;
 *   `false` if the point hit no content (caller keeps its end-of-block default).
 */
export function placeCaretFromClick(
    editor: Editor,
    coords: { x: number; y: number },
): boolean {
    // ProseMirror's posAtCoords takes {left, top} in viewport (client) space and
    // returns { pos, inside } or null when the point is outside any content.
    const hit = editor.view.posAtCoords({ left: coords.x, top: coords.y });
    if (!hit) return false;
    editor.chain().focus().setTextSelection(hit.pos).run();
    return true;
}
