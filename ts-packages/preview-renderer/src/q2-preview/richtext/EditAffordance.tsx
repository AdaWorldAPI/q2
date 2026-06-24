// Phase 1a (bd-sjb4pzx8) — left-margin edit affordance.
//
// Parked in the LEFT MARGIN of the active edit box (absolute-positioned, off the
// text) so it never hijacks clicking/selecting. Shows "Editing…" plus — when the
// block supports the rich editor — a rich/plain surface toggle (the escape hatch
// to the monospaced textarea for syntax the rich editor can't express).
//
// Rendered by `renderMeasuredEdit` for BOTH surfaces, so the toggle is reachable
// from rich AND plain mode. Only shown when `ctx.richText` is on.

import type { MouseEvent } from 'react';
import type { PreviewContextValue } from './../PreviewContext';
import { ensureRichTextStyles } from './styles';

export function EditAffordance({
  ctx,
  richSupported,
}: {
  ctx: PreviewContextValue;
  richSupported: boolean;
}) {
  // Inject the shared stylesheet here too — the affordance renders in plain mode
  // (textarea), where RichTextEditor (the other injection site) isn't mounted.
  ensureRichTextStyles();
  const mode = ctx.editorMode ?? 'rich';

  // mousedown + preventDefault: keep focus in the editor (a real blur would
  // commit/close the surface before the mode switch lands).
  const choose = (m: 'rich' | 'plain') => (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (m === mode) return;
    // Mark the swap so the outgoing surface's unmount-blur doesn't commit/close.
    // Cleared on the next tick, after the synchronous unmount-blur has passed.
    if (ctx.editorModeSwitchRef) ctx.editorModeSwitchRef.current = true;
    ctx.setEditorMode?.(m);
    setTimeout(() => {
      if (ctx.editorModeSwitchRef) ctx.editorModeSwitchRef.current = false;
    }, 0);
  };

  return (
    <div className="q2-edit-affordance" contentEditable={false}>
      <div className="q2-edit-affordance-label">Editing…</div>
      {richSupported && (
        <div className="q2-edit-mode-toggle" role="group" aria-label="Editor mode">
          <button
            type="button"
            className={mode === 'rich' ? 'q2-edit-mode-active' : ''}
            aria-pressed={mode === 'rich'}
            onMouseDown={choose('rich')}
          >
            rich text
          </button>
          <button
            type="button"
            className={mode === 'plain' ? 'q2-edit-mode-active' : ''}
            aria-pressed={mode === 'plain'}
            onMouseDown={choose('plain')}
          >
            plain text
          </button>
        </div>
      )}
    </div>
  );
}
