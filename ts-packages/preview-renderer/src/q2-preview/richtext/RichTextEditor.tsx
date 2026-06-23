// Phase 1a (bd-sjb4pzx8) — the WYSIWYG block editor.
//
// Drop-in alternative to `EditTextarea`: rendered inside the SAME measured box,
// seeded from the block's untransformed AST subtree (astToDoc), committing
// markdown through the UNCHANGED `commitTextEdit` path. Because it lives in the
// preview iframe — which already has the Bootstrap + Quarto theme CSS loaded —
// the editor's semantic tags (<p>, <em>, <strong>, <a>) are styled by the theme
// automatically, so editing looks like the rendered page.
//
// Scope (1a): single paragraph, inline editing only. Enter is intercepted (no
// structural splits yet — that arrives in 1c). Dirtiness is read from
// ProseMirror's own change signal (`doc.eq`), NOT from comparing serialized text,
// so an unedited open-and-close is a true no-op (C3) and never reformats.

import { useEffect, useMemo, useRef } from 'react';
import { useEditor, EditorContent } from '@tiptap/react';
import StarterKit from '@tiptap/starter-kit';
import type { Editor } from '@tiptap/core';
import type { Node as PMNode } from '@tiptap/pm/model';
import type { PreviewContextValue, ResolvedSource } from './../PreviewContext';
import { buildNestingCommitDestination } from './../nestingNav';
import { astToDoc } from './astToProseMirror';
import { docToMarkdown } from './serializer';
import { Chip } from './chipExtension';
import type { AstNode, PoolEntry } from './ast';
import { ensureRichTextStyles } from './styles';

function commitDestination(ctx: PreviewContextValue, resolved: ResolvedSource): string | null {
  if (ctx.editTargetRef !== undefined) {
    return buildNestingCommitDestination(ctx.editTargetRef.current);
  }
  return JSON.stringify(resolved.sourceEntry);
}

/** True when this editor is still the active target (guards stale-unmount blur). */
function isStillActive(ctx: PreviewContextValue, resolved: ResolvedSource): boolean {
  if (ctx.editTargetRef === undefined) return true;
  const cur = ctx.editTargetRef.current;
  return !!cur && cur.anchorR0 === resolved.sourceEntry.r[0];
}

export function RichTextEditor({
  ctx,
  resolved,
}: {
  ctx: PreviewContextValue;
  resolved: ResolvedSource;
}) {
  ensureRichTextStyles();

  // Seed the document from the AST subtree once, at mount.
  const seedJSON = useMemo(() => {
    const pool = (ctx.pool ?? []) as PoolEntry[];
    const src = ctx.content ?? '';
    const { doc } = astToDoc([resolved.sourceNode as unknown as AstNode], pool, src);
    return doc.toJSON();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // The seeded doc, captured post-normalization in onCreate — the dirty baseline.
  const initialDocRef = useRef<PMNode | null>(null);
  // Latch so a commit fires at most once (blur can follow a key-commit).
  const committedRef = useRef(false);

  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        // 1a: paragraph + inline marks only. Structural block types arrive in 1b/1c.
        heading: false,
        blockquote: false,
        bulletList: false,
        orderedList: false,
        listItem: false,
        codeBlock: false,
        horizontalRule: false,
        link: { openOnClick: false },
      }),
      Chip,
    ],
    content: seedJSON,
    autofocus: 'end',
    onCreate({ editor: ed }) {
      initialDocRef.current = ed.state.doc;
    },
  });

  const commit = (ed: Editor) => {
    if (committedRef.current) return;
    // Stale-unmount guard (mirrors EditTextarea): a dropped/re-anchored editor's
    // blur must not write to a byte range it no longer owns.
    if (!isStillActive(ctx, resolved)) return;
    committedRef.current = true;

    const base = initialDocRef.current;
    const changed = base ? !ed.state.doc.eq(base) : false;
    if (!changed) {
      // True no-op: unedited open-and-close never reformats (C3).
      ctx.setEditTarget?.(null);
      return;
    }
    const dest = commitDestination(ctx, resolved);
    if (dest === null) {
      ctx.setEditTarget?.(null);
      return;
    }
    ctx.commitTextEdit?.(dest, docToMarkdown(ed.state.doc));
    ctx.setEditTarget?.(null);
  };

  const cancel = () => {
    committedRef.current = true;
    ctx.requestFocusRestore?.(resolved.sourceEntry.r[0]);
    ctx.setEditTarget?.(null);
  };

  // Keyboard: Esc cancels; Mod-Enter commits; plain Enter is swallowed (no split).
  useEffect(() => {
    if (!editor) return;
    const dom = editor.view.dom as HTMLElement;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        cancel();
      } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        ctx.requestFocusRestore?.(resolved.sourceEntry.r[0]);
        commit(editor);
      } else if (e.key === 'Enter' && !e.shiftKey) {
        // 1a: no structural split. Swallow plain Enter (Shift+Enter = hard break).
        e.preventDefault();
      }
    };
    const onBlur = () => {
      ctx.requestFocusRestore?.(resolved.sourceEntry.r[0]);
      commit(editor);
    };
    dom.addEventListener('keydown', onKeyDown);
    dom.addEventListener('blur', onBlur);
    return () => {
      dom.removeEventListener('keydown', onKeyDown);
      dom.removeEventListener('blur', onBlur);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor]);

  return <div className="q2-richtext-editor">{editor && <EditorContent editor={editor} />}</div>;
}
