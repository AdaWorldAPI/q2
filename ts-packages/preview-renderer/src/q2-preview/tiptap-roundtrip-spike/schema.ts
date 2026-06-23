// THROWAWAY SPIKE (bd-sjb4pzx8) — ProseMirror schema = prosemirror-markdown's schema
// plus an atomic inline `chip` node carrying verbatim qmd source for opaque constructs.
// (tiptap is a wrapper over ProseMirror; the round-trip fidelity question lives here,
//  in the model + serializer, not in the React editing UI.) Safe to delete.

import { schema as mdSchema } from 'prosemirror-markdown';
import { Schema, type NodeSpec } from 'prosemirror-model';

export type ChipKind = 'math' | 'cite' | 'shortcode' | 'span' | 'raw' | 'block';

// Inline atom: renders as a non-editable pill in the editor; serializes back to its
// exact source bytes. This is posit-assistant's "mention" pattern, generalized.
const chipSpec: NodeSpec = {
  inline: true,
  atom: true,
  group: 'inline',
  selectable: true,
  attrs: {
    src: {}, // verbatim qmd source token, e.g. "{{< video x >}}" / "$x^2$" / "@fig-1"
    kind: { default: 'raw' },
  },
  toDOM(node) {
    const { src, kind } = node.attrs as { src: string; kind: ChipKind };
    return ['span', { class: `q2-chip q2-chip-${kind}`, 'data-src': src, contenteditable: 'false' }, src];
  },
  parseDOM: [
    {
      tag: 'span.q2-chip',
      getAttrs(dom) {
        const el = dom as HTMLElement;
        return {
          src: el.getAttribute('data-src') ?? '',
          kind: (el.className.match(/q2-chip-(\w+)/)?.[1] ?? 'raw') as ChipKind,
        };
      },
    },
  ],
};

export const schema = new Schema({
  nodes: mdSchema.spec.nodes.addToEnd('chip', chipSpec),
  marks: mdSchema.spec.marks,
});
