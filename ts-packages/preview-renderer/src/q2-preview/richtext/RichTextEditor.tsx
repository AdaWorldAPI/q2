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
import Subscript from '@tiptap/extension-subscript';
import Superscript from '@tiptap/extension-superscript';
import type { Editor } from '@tiptap/core';
import type { Node as PMNode } from '@tiptap/pm/model';
import type { PreviewContextValue, ResolvedSource } from './../PreviewContext';
import { buildNestingCommitDestination } from './../nestingNav';
import { astToDoc } from './astToProseMirror';
import { docToMarkdown } from './serializer';
import { Chip } from './chipExtension';
import { RichTextToolbar } from './RichTextToolbar';
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
  // The original markdown the textarea seeds from (so reverting an unedited rich
  // doc restores the exact source rather than a re-serialized form).
  const originalMarkdownRef = useRef<string | null>(null);
  // Latch so a commit fires at most once (blur can follow a key-commit).
  const committedRef = useRef(false);

  const editor = useEditor({
    extensions: [
      StarterKit.configure({
        // 1a: paragraphs + inline marks. 1b: + headings. (1c: lists/quotes/code.)
        heading: { levels: [1, 2, 3, 4, 5, 6] },
        blockquote: false,
        bulletList: false,
        orderedList: false,
        listItem: false,
        codeBlock: false,
        horizontalRule: false,
        // No phantom trailing paragraph: a single-heading (or any non-paragraph)
        // block would otherwise get an empty trailing <p> — extra vertical space
        // in the editor AND a stray blank block on commit.
        trailingNode: false,
        link: { openOnClick: false },
      }),
      Subscript,
      Superscript,
      Chip,
    ],
    content: seedJSON,
    autofocus: 'end',
    // 1b: edit existing structure only — no markdown auto-conversion (e.g. typing
    // "## " must not turn a paragraph into a heading, or change a heading's level).
    // Structural edits are a later phase; bold/italic via Cmd-B/I still work.
    enableInputRules: false,
    enablePasteRules: false,
    onCreate({ editor: ed }) {
      initialDocRef.current = ed.state.doc;
      // editDraftRef was seeded with the original markdown at activation; remember
      // it so we can restore it verbatim if the user reverts their rich edits.
      originalMarkdownRef.current = ctx.editDraftRef?.current ?? null;
    },
    onUpdate({ editor: ed }) {
      // Keep the shared markdown draft current so a switch to plain text carries
      // the rich edits across (dirty-aware: when unchanged, restore the verbatim
      // original so an untouched toggle doesn't reformat the block — C3).
      if (!ctx.editDraftRef) return;
      const base = initialDocRef.current;
      ctx.editDraftRef.current =
        base && !ed.state.doc.eq(base)
          ? docToMarkdown(ed.state.doc)
          : originalMarkdownRef.current ?? docToMarkdown(ed.state.doc);
    },
  });

  const commit = (ed: Editor) => {
    if (committedRef.current) return;
    // A rich/plain surface swap is not a commit — the content is preserved in
    // editDraftRef; the swap must not close the edit session.
    if (ctx.editorModeSwitchRef?.current) return;
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

  // The whole edit box (editor + toolbar + link input) is one focus scope: we
  // commit only when focus leaves it entirely, so focusing the toolbar's link
  // input keeps the session open.
  const rootRef = useRef<HTMLDivElement | null>(null);

  // Keyboard: Esc cancels; Mod-Enter commits; plain Enter is swallowed (no split).
  // Commit: a focusout from the edit box (focus moved outside it) commits.
  useEffect(() => {
    if (!editor) return;
    const dom = editor.view.dom as HTMLElement;
    const root = rootRef.current;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        cancel();
      } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        ctx.requestFocusRestore?.(resolved.sourceEntry.r[0]);
        commit(editor);
      } else if (e.key === 'Enter' && !e.shiftKey) {
        // 1a/1b: no structural split. Swallow plain Enter (Shift+Enter = hard break).
        e.preventDefault();
      }
    };
    const onFocusOut = (e: FocusEvent) => {
      // A surface swap fires this as the editor unmounts — not a commit.
      if (ctx.editorModeSwitchRef?.current) return;
      const next = e.relatedTarget as Node | null;
      // Focus stayed within the edit box (toolbar button / link input) — not a commit.
      if (next && root && root.contains(next)) return;
      ctx.requestFocusRestore?.(resolved.sourceEntry.r[0]);
      commit(editor);
    };
    dom.addEventListener('keydown', onKeyDown);
    root?.addEventListener('focusout', onFocusOut);
    return () => {
      dom.removeEventListener('keydown', onKeyDown);
      root?.removeEventListener('focusout', onFocusOut);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editor]);

  // The left-margin affordance (Editing… + rich/plain toggle) is rendered by
  // renderMeasuredEdit so it is shared with the plain-text surface. The
  // formatting toolbar is rich-only and lives here (it needs the editor).
  return (
    <div className="q2-richtext-editor" ref={rootRef}>
      {editor && <RichTextToolbar editor={editor} />}
      {editor && <EditorContent editor={editor} />}
    </div>
  );
}
