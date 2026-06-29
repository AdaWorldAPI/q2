// Phase 1b (bd-sjb4pzx8) — a small formatting toolbar anchored to the top-left
// of the rich-text edit box.
//
// Mark buttons (bold/italic/strike/sub/sup) are second triggers for the same
// commands Cmd-B/I fire — `toggleMark` over the current selection (ProseMirror
// applies/removes the mark across the range; an empty selection sets a stored
// mark so the next typed text gets it). The link button opens a small URL input
// and uses `extendMarkRange('link')` so an existing link can be edited/removed by
// placing the cursor anywhere inside it.
//
// All buttons use mousedown-preventDefault so clicking them never blurs the
// editor (which would collapse the selection before the command runs). The link
// input DOES take focus; the editor's commit is scoped to "focus left the whole
// edit box" (see RichTextEditor), so focusing the input keeps the session open.

import { useEffect, useLayoutEffect, useRef, useState, type MouseEvent, type ReactNode } from 'react';
import type { Editor } from '@tiptap/core';
import { shouldPlaceChromeBelow } from '../editChromeGeometry';

/** Gap (px) between the toolbar and the edit box, matching the CSS margin. */
const TOOLBAR_GAP = 4;

interface MarkSpec {
  name: string;
  label: string;
  title: string;
}

const MARKS: MarkSpec[] = [
  { name: 'bold', label: 'B', title: 'Bold (⌘B)' },
  { name: 'italic', label: 'I', title: 'Italic (⌘I)' },
  { name: 'strike', label: 'S', title: 'Strikethrough' },
  { name: 'subscript', label: 'x₂', title: 'Subscript' },
  { name: 'superscript', label: 'x²', title: 'Superscript' },
];

export function RichTextToolbar({
  editor,
  trailing,
}: {
  editor: Editor;
  /** Optional content rendered at the END of the toolbar row, after a separator
   *  (bd-9x3zbuj8 Task 2: the inline nesting breadcrumb). */
  trailing?: ReactNode;
}) {
  // Re-render on selection/content changes so isActive() highlights stay current.
  const [, force] = useState(0);
  useEffect(() => {
    const bump = () => force((n) => n + 1);
    editor.on('selectionUpdate', bump);
    editor.on('transaction', bump);
    return () => {
      editor.off('selectionUpdate', bump);
      editor.off('transaction', bump);
    };
  }, [editor]);

  // Vertical placement (bd-pvcnea83): the toolbar floats ABOVE the edit box by
  // default, but flips BELOW when there isn't room above (e.g. editing the first
  // block of a title-less document, flush against the viewport top — otherwise it
  // is clipped above the scroll area, with no way to scroll up to it).
  const toolbarRef = useRef<HTMLDivElement | null>(null);
  const [placeBelow, setPlaceBelow] = useState(false);
  useLayoutEffect(() => {
    const tb = toolbarRef.current;
    const box = tb?.closest('.q2-richtext-editor');
    if (!tb || !box) return;
    const height = tb.offsetHeight;
    // Degenerate layout (jsdom zero-rects): keep the default 'above' placement.
    if (height <= 0) return;
    const surfaceTop = box.getBoundingClientRect().top;
    setPlaceBelow(shouldPlaceChromeBelow(surfaceTop, height, TOOLBAR_GAP));
    // Mount-only: the toolbar remounts per edit target, and the edit box's top is
    // stable for a given target, so a single measurement suffices.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const [linkOpen, setLinkOpen] = useState(false);
  const [linkUrl, setLinkUrl] = useState('');
  const linkInputRef = useRef<HTMLInputElement | null>(null);

  // Reliably focus + select the URL input when the link editor opens (more robust
  // than the autoFocus prop alone).
  useEffect(() => {
    if (linkOpen && linkInputRef.current) {
      linkInputRef.current.focus();
      linkInputRef.current.select();
    }
  }, [linkOpen]);

  const toggleMark = (name: string) => (e: MouseEvent) => {
    e.preventDefault();
    editor.chain().focus().toggleMark(name).run();
  };

  const openLinkEditor = (e: MouseEvent) => {
    e.preventDefault();
    const existing = editor.isActive('link') ? (editor.getAttributes('link').href as string) : '';
    setLinkUrl(existing ?? '');
    setLinkOpen(true);
  };

  const applyLink = () => {
    const url = linkUrl.trim();
    if (!url) {
      // Empty URL on an existing link removes it; otherwise just cancel.
      if (editor.isActive('link')) {
        editor.chain().focus().extendMarkRange('link').unsetLink().run();
      }
    } else if (editor.state.selection.empty && !editor.isActive('link')) {
      // No selection and not in a link: insert the URL as linked text.
      editor.chain().focus().insertContent({ type: 'text', text: url, marks: [{ type: 'link', attrs: { href: url } }] }).run();
    } else {
      editor.chain().focus().extendMarkRange('link').setLink({ href: url }).run();
    }
    setLinkOpen(false);
  };

  const removeLink = () => {
    editor.chain().focus().extendMarkRange('link').unsetLink().run();
    setLinkOpen(false);
  };

  const cancelLink = () => {
    setLinkOpen(false);
    editor.chain().focus().run();
  };

  return (
    <div
      ref={toolbarRef}
      className={`q2-rt-toolbar${placeBelow ? ' q2-rt-toolbar-below' : ''}`}
      contentEditable={false}
    >
      {!linkOpen ? (
        <>
          {MARKS.map((m) => (
            <button
              key={m.name}
              type="button"
              title={m.title}
              aria-pressed={editor.isActive(m.name)}
              className={`q2-rt-tb-btn q2-rt-tb-${m.name}${editor.isActive(m.name) ? ' q2-rt-tb-active' : ''}`}
              onMouseDown={toggleMark(m.name)}
            >
              {m.label}
            </button>
          ))}
          <span className="q2-rt-tb-sep" />
          <button
            type="button"
            title="Link"
            aria-pressed={editor.isActive('link')}
            className={`q2-rt-tb-btn q2-rt-tb-link${editor.isActive('link') ? ' q2-rt-tb-active' : ''}`}
            onMouseDown={openLinkEditor}
          >
            🔗
          </button>
        </>
      ) : (
        <div className="q2-rt-link-editor">
          <input
            ref={linkInputRef}
            type="url"
            className="q2-rt-link-input"
            placeholder="https://…"
            value={linkUrl}
            onChange={(e) => setLinkUrl(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault();
                applyLink();
              } else if (e.key === 'Escape') {
                e.preventDefault();
                cancelLink();
              }
            }}
          />
          <button type="button" className="q2-rt-tb-btn" title="Apply" onMouseDown={(e) => { e.preventDefault(); applyLink(); }}>✓</button>
          {editor.isActive('link') && (
            <button type="button" className="q2-rt-tb-btn" title="Remove link" onMouseDown={(e) => { e.preventDefault(); removeLink(); }}>✕</button>
          )}
        </div>
      )}
      {trailing != null && (
        <>
          <span className="q2-rt-tb-sep" />
          {trailing}
        </>
      )}
    </div>
  );
}
