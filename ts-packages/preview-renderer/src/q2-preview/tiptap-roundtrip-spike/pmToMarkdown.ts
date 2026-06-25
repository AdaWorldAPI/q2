// THROWAWAY SPIKE (bd-sjb4pzx8) — ProseMirror doc -> markdown text.
// Reuses prosemirror-markdown's default serializer rules and adds a `chip` rule that
// emits the verbatim source UNESCAPED (so shortcodes/math/cites survive byte-exact).
// This is the C1 "serializer fidelity" probe. Safe to delete.

import { MarkdownSerializer, defaultMarkdownSerializer } from 'prosemirror-markdown';
import type { Node as PMNode } from 'prosemirror-model';

export const serializer = new MarkdownSerializer(
  {
    ...defaultMarkdownSerializer.nodes,
    chip(state: { text: (s: string, escape?: boolean) => void }, node: PMNode) {
      // escape=false: do NOT markdown-escape `{`, `$`, `@`, `<`, etc.
      state.text(node.attrs.src as string, false);
    },
  },
  defaultMarkdownSerializer.marks,
);

export function pmToMarkdown(doc: PMNode): string {
  return serializer.serialize(doc);
}
