// Phase 1a (bd-sjb4pzx8) — one-time CSS for the rich-text editor.
//
// Goal: the editor should look like the rendered page. The heavy lifting is done
// by the theme stylesheet already loaded in the iframe (it styles the editor's
// <p>/<em>/<strong>/<a> for free). This sheet only (a) strips ProseMirror's
// default editor chrome so it doesn't fight the theme, (b) zeroes the inner
// block margin since the measured box already reproduces the block's spacing,
// and (c) gives chips a subtle, source-token pill look.

let injected = false;

const CSS = `
.q2-richtext-editor { position: relative; }

/* Faint "Editing…" label parked in the left margin, vertically aligned to the
   first line. pointer-events:none + user-select:none so it never interferes with
   text interaction. This is the first of the left-margin edit affordances. */
.q2-richtext-editing-label {
  position: absolute;
  right: calc(100% + 0.6rem);
  top: 0;
  font-size: 0.72rem;
  line-height: 1.7;
  font-style: italic;
  white-space: nowrap;
  color: rgba(59, 130, 246, 0.65);
  user-select: none;
  pointer-events: none;
}

.q2-richtext-editor .ProseMirror {
  outline: none;
  white-space: pre-wrap;
  word-wrap: break-word;
  /* "Edit mode" affordance: the WYSIWYG render is faithful enough that the user
     can't otherwise tell editing is live. A subtle tint + ring signals it. The
     padding is cancelled by an equal negative margin so the text does NOT shift
     (zero reflow) — only the tinted area extends slightly around the content. */
  background: rgba(59, 130, 246, 0.08);
  box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.35);
  border-radius: 3px;
  padding: 2px 5px;
  margin: -2px -5px;
}
.q2-richtext-editor .ProseMirror:focus { outline: none; }
/* The measured edit box reproduces the original block's margin/padding; the
   editor's own block must not add a second margin. */
.q2-richtext-editor .ProseMirror > * { margin-top: 0; margin-bottom: 0; }

/* Opaque source-token chips (v1: pills, not rendered). */
.q2-chip {
  font-family: var(--bs-font-monospace, ui-monospace, SFMono-Regular, Menlo, monospace);
  font-size: 0.85em;
  background: rgba(120, 120, 160, 0.14);
  border: 1px solid rgba(120, 120, 160, 0.30);
  border-radius: 3px;
  padding: 0 0.25em;
  white-space: nowrap;
  cursor: default;
  user-select: all;
}
.q2-chip-math { background: rgba(80, 160, 120, 0.14); border-color: rgba(80, 160, 120, 0.30); }
.q2-chip-cite, .q2-chip-shortcode { background: rgba(160, 120, 80, 0.14); border-color: rgba(160, 120, 80, 0.30); }
`;

/** Inject the rich-text editor stylesheet into the document head once. */
export function ensureRichTextStyles(): void {
  if (injected || typeof document === 'undefined') return;
  injected = true;
  const style = document.createElement('style');
  style.setAttribute('data-q2-richtext', '1');
  style.textContent = CSS;
  document.head.appendChild(style);
}
