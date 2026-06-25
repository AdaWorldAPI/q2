// THROWAWAY SPIKE (bd-sjb4pzx8) — Pandoc untransformed AST -> ProseMirror doc.
// The "input/seed" bridge: we build the PM doc from the typed AST we already have
// (NOT by re-lexing markdown with markdown-it), and lift opaque node types
// (Math/Cite/Span.shortcode/RawInline/RawBlock/Div) into verbatim chips by slicing
// source via their byte ranges. Safe to delete.

import type { Mark, Node as PMNode } from 'prosemirror-model';
import { schema, type ChipKind } from './schema';
import { type AstNode, type PoolEntry, nodeSource } from './pampa';

interface Ctx {
  pool: PoolEntry[];
  src: string;
  /** constructs that fell back to a chip, for the findings report */
  chips: { kind: ChipKind; src: string }[];
  /** node types we did not know how to map (gaps to report) */
  unknown: Set<string>;
}

const m = schema.marks;
const n = schema.nodes;

function chip(node: AstNode, kind: ChipKind, ctx: Ctx, fallbackText: string): PMNode {
  const src = nodeSource(node, ctx.pool, ctx.src) ?? fallbackText;
  ctx.chips.push({ kind, src });
  return n.chip.create({ src, kind });
}

function asArray(c: unknown): AstNode[] {
  return Array.isArray(c) ? (c as AstNode[]) : [];
}

// ---- inlines ---------------------------------------------------------------

function inlines(items: AstNode[], marks: readonly Mark[], ctx: Ctx): PMNode[] {
  const out: PMNode[] = [];
  for (const node of items) {
    switch (node.t) {
      case 'Str':
        out.push(schema.text(node.c as string, marks));
        break;
      case 'Space':
      case 'SoftBreak':
        // SoftBreak collapses to a space (reformatted-but-equivalent; noted in oracle).
        out.push(schema.text(' ', marks));
        break;
      case 'LineBreak':
        out.push(n.hard_break.create(undefined, undefined, marks));
        break;
      case 'Emph':
        out.push(...inlines(asArray(node.c), marks.concat(m.em.create()), ctx));
        break;
      case 'Strong':
        out.push(...inlines(asArray(node.c), marks.concat(m.strong.create()), ctx));
        break;
      case 'Underline':
      case 'Strikeout':
      case 'Superscript':
      case 'Subscript':
      case 'SmallCaps':
      case 'Quoted':
        // Not in the prose-rich v1 mark set -> chip the whole construct verbatim.
        out.push(chip(node, 'span', ctx, ''));
        break;
      case 'Code': {
        const [, code] = node.c as [unknown, string];
        out.push(schema.text(code, marks.concat(m.code.create())));
        break;
      }
      case 'Link': {
        const [, label, target] = node.c as [unknown, AstNode[], [string, string]];
        const link = m.link.create({ href: target[0], title: target[1] || null });
        out.push(...inlines(label, marks.concat(link), ctx));
        break;
      }
      case 'Math':
        out.push(chip(node, 'math', ctx, ''));
        break;
      case 'Cite':
        out.push(chip(node, 'cite', ctx, ''));
        break;
      case 'RawInline':
        out.push(chip(node, 'raw', ctx, (node.c as [string, string])?.[1] ?? ''));
        break;
      case 'Span': {
        const attr = (node.c as [AstNode] )?.[0] as unknown as [string, string[], [string, string][]];
        const classes = attr?.[1] ?? [];
        const isShortcode = classes.includes('quarto-shortcode__');
        out.push(chip(node, isShortcode ? 'shortcode' : 'span', ctx, ''));
        break;
      }
      case 'Image':
        // Treat as opaque for v1 (attrs/alt fidelity is out of scope for the spike).
        out.push(chip(node, 'raw', ctx, ''));
        break;
      case 'Note':
        out.push(chip(node, 'raw', ctx, ''));
        break;
      default:
        ctx.unknown.add(`inline:${node.t}`);
        out.push(chip(node, 'raw', ctx, ''));
        break;
    }
  }
  return out;
}

// ---- blocks ----------------------------------------------------------------

function listItems(itemsC: unknown, ctx: Ctx): PMNode[] {
  // BulletList: Block[][]; each item is Block[].
  const items = (itemsC as AstNode[][]) ?? [];
  return items.map((itemBlocks) => n.list_item.create(null, blocks(itemBlocks, ctx)));
}

function blocks(items: AstNode[], ctx: Ctx): PMNode[] {
  const out: PMNode[] = [];
  for (const node of items) {
    switch (node.t) {
      case 'Para':
      case 'Plain':
        out.push(n.paragraph.create(null, inlines(asArray(node.c), [], ctx)));
        break;
      case 'Header': {
        const [level, , inl] = node.c as [number, unknown, AstNode[]];
        out.push(n.heading.create({ level }, inlines(inl, [], ctx)));
        break;
      }
      case 'BulletList':
        out.push(n.bullet_list.create({ tight: true }, listItems(node.c, ctx)));
        break;
      case 'OrderedList': {
        const [attrs, its] = node.c as [[number, unknown, unknown], unknown];
        out.push(n.ordered_list.create({ order: attrs?.[0] ?? 1, tight: true }, listItems(its, ctx)));
        break;
      }
      case 'BlockQuote':
        out.push(n.blockquote.create(null, blocks(asArray(node.c), ctx)));
        break;
      case 'CodeBlock': {
        const [attr, code] = node.c as [[string, string[], [string, string][]], string];
        const classes = attr?.[1] ?? [];
        // pampa stores the fence info string as a class verbatim, INCLUDING braces
        // for Quarto executable cells (```{python} -> class "{python}"; ```python ->
        // class "python"). So the round-trip is just: params = classes joined.
        // prosemirror-markdown's code_block emits ```${params}\n...code...\n```.
        // (id / key-value attrs on a code block are out of scope for the spike.)
        const params = classes.join(' ');
        out.push(n.code_block.create({ params }, code ? [schema.text(code)] : []));
        break;
      }
      case 'HorizontalRule':
        out.push(n.horizontal_rule.create());
        break;
      case 'RawBlock':
      case 'Div':
      case 'Table':
      case 'Figure':
      case 'DefinitionList': {
        // v1: opaque block -> paragraph wrapping a verbatim chip. (In real integration
        // we "reach into" a Div instead of chipping it; see plan refinement 2.)
        out.push(n.paragraph.create(null, [chip(node, 'block', ctx, '')]));
        break;
      }
      case 'Null':
        break;
      default:
        ctx.unknown.add(`block:${node.t}`);
        out.push(n.paragraph.create(null, [chip(node, 'block', ctx, '')]));
        break;
    }
  }
  return out;
}

export interface AstToPmResult {
  doc: PMNode;
  chips: { kind: ChipKind; src: string }[];
  unknown: string[];
}

export function astToPm(ast: { blocks: AstNode[]; astContext: { p: PoolEntry[] } }, src: string): AstToPmResult {
  const ctx: Ctx = { pool: ast.astContext.p, src, chips: [], unknown: new Set() };
  const doc = n.doc.create(null, blocks(ast.blocks, ctx));
  return { doc, chips: ctx.chips, unknown: [...ctx.unknown] };
}
