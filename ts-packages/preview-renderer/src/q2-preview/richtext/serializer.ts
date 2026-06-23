// Phase 1 (bd-sjb4pzx8) — ProseMirror document -> markdown text.
//
// Reuses prosemirror-markdown's validated default rules, re-keyed to tiptap's
// node/mark NAMES, with custom rules where tiptap's attrs differ from
// prosemirror-markdown's (codeBlock.language vs code_block.params; orderedList.start
// vs ordered_list.order) and for the `chip` atom (verbatim, unescaped). The same
// serializer runs over both the live tiptap editor's doc and the test schema's doc.

import {
  MarkdownSerializer,
  MarkdownSerializerState,
  defaultMarkdownSerializer,
} from 'prosemirror-markdown';
import type { Node as PMNode } from '@tiptap/pm/model';

const d = defaultMarkdownSerializer;

type NodeRule = (state: MarkdownSerializerState, node: PMNode, parent: PMNode, index: number) => void;

const nodes: Record<string, NodeRule> = {
  paragraph: d.nodes.paragraph as NodeRule,
  heading: d.nodes.heading as NodeRule,
  blockquote: d.nodes.blockquote as NodeRule,
  bulletList: d.nodes.bullet_list as NodeRule,
  listItem: d.nodes.list_item as NodeRule,
  hardBreak: d.nodes.hard_break as NodeRule,
  text: d.nodes.text as NodeRule,

  // tiptap codeBlock carries `language`; prosemirror-markdown's code_block reads
  // `params`. Emit a fence + the language verbatim (handles ```{python} and ```python).
  codeBlock(state, node) {
    const language = (node.attrs.language as string) || '';
    state.write('```' + language + '\n');
    state.text(node.textContent, false);
    state.ensureNewLine();
    state.write('```');
    state.closeBlock(node);
  },

  // tiptap orderedList carries `start`; prosemirror-markdown's ordered_list reads
  // `order`. Replicate the default rule against `start`.
  orderedList(state, node) {
    const start = (node.attrs.start as number) || 1;
    const maxW = String(start + node.childCount - 1).length;
    const space = state.repeat(' ', maxW + 2);
    state.renderList(node, space, (i) => {
      const nStr = String(start + i);
      return state.repeat(' ', maxW - nStr.length) + nStr + '. ';
    });
  },

  // Opaque construct: emit the verbatim source, UNESCAPED (so `{`, `$`, `@`, `<`
  // survive). The chip round-trips byte-for-byte.
  chip(state, node) {
    state.text(node.attrs.src as string, false);
  },
};

const marks = {
  bold: d.marks.strong,
  // qmd disallows `***` (triple-star). Use `_` for italic so bold+italic
  // serializes as `**_…_**` (valid) instead of `***…***` (rejected by pampa).
  // `_` is intraword-safe, so this is the better qmd choice regardless.
  italic: { open: '_', close: '_', mixable: true, expelEnclosingWhitespace: true },
  code: d.marks.code,
  link: d.marks.link,
  strike: { open: '~~', close: '~~', mixable: true, expelEnclosingWhitespace: true },
  // qmd subscript `~x~` / superscript `^x^` (Pandoc). Single tilde for subscript
  // (double `~~` is strikethrough, above).
  subscript: { open: '~', close: '~', mixable: true, expelEnclosingWhitespace: true },
  superscript: { open: '^', close: '^', mixable: true, expelEnclosingWhitespace: true },
};

export const richTextSerializer = new MarkdownSerializer(nodes, marks);

/** Serialize a ProseMirror document to markdown text. */
export function docToMarkdown(doc: PMNode): string {
  return richTextSerializer.serialize(doc);
}
