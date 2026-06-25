// Phase 1 (bd-sjb4pzx8) — tiptap node for opaque "chip" atoms.
//
// An inline, atomic, non-editable node that renders as a small pill showing its
// verbatim qmd source and serializes back to that source unchanged (the
// serializer's `chip` rule emits `node.attrs.src` unescaped). v1 chips are
// source-text pills (per user decision); a richer NodeView (KaTeX math, resolved
// crossref) is a deferred option.
//
// The node NAME + attrs (`src`, `kind`) match `richtext/schema.ts` so the seed
// doc (built by `astToDoc` and handed to tiptap as JSON) parses cleanly here.

import { Node, mergeAttributes } from '@tiptap/core';

export const Chip = Node.create({
  name: 'chip',
  group: 'inline',
  inline: true,
  atom: true,
  selectable: true,
  draggable: false,

  addAttributes() {
    return {
      src: { default: '' },
      kind: { default: 'raw' },
    };
  },

  parseHTML() {
    return [{ tag: 'span.q2-chip' }];
  },

  renderHTML({ HTMLAttributes, node }) {
    return [
      'span',
      mergeAttributes(HTMLAttributes, {
        class: `q2-chip q2-chip-${node.attrs.kind as string}`,
        'data-src': node.attrs.src as string,
        contenteditable: 'false',
      }),
      node.attrs.src as string,
    ];
  },
});
