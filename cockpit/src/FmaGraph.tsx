// FMA anatomy slice — the "real test" of the dual-membership lattice. Decodes
// the baked /fma.soa (heart organ subtree + cross-cutting tissue TYPES) into the
// same vis-network renderer, and on click shows a node resolving to BOTH:
//   · its part-of position (basin-local: organ → chamber → wall → structure)
//   · its leaf-limited global TYPE (the 0xFFFF ceiling pole — cross-cutting,
//     e.g. one "Subdivision of cavity of cardiac chamber" or "Endothelium of
//     endocardium" shared by structures in different parts of the heart).
//
// This is the `Cascade` (ontology / part-of) reading of OGAR PR #116's HhtlMode
// FMA tier model: each node is a stack of 8:8 [container:identity] tiers —
// HEEL=[Organ:Heart], HIP=[Chamber:id], TWIG=[Wall:id], LEAF=[Tissue:id] — where
// the container byte is the KIND mixin node and the identity the instance, so the
// partonomy IS the key and the layout reads straight off it. OGAR's
// ogar-fma-skeleton is the `Located` (spatial) sibling (the same 8:8 tiers carry
// coronal x:y / depth z Morton cells). classid 0x0A01 = anatomical_structure in
// OGAR's ConceptDomain::Anatomy (0x0A).
import { useEffect, useMemo, useRef, useState } from 'react';
import { Network, type Options } from 'vis-network';
import { DataSet } from 'vis-data';
import { decodeSoa, type Soa } from './OsintGraph';

const PAGE_BG = '#0a0e17';
const CEILING_COLOR = '#ffd166';

// class byte → colour/name (mirrors the C_* consts in src/bin/fma.rs).
const FMA_CLASS = [
  { name: 'Organ', color: '#ff637d' },
  { name: 'Chamber', color: '#ffb547' },
  { name: 'Wall', color: '#4dd0e1' },
  { name: 'Tissue', color: '#35d07f' },
  { name: 'Cell', color: '#9b8cff' },
  { name: 'Type · global', color: CEILING_COLOR },
];
const classColor = (c: number) => FMA_CLASS[c]?.color ?? '#8899aa';

// rel byte → name (REL_* in src/bin/fma.rs). 2 part-of, 3 is-a(global type).
const REL = ['member-of', 'interfaces', 'part-of', 'is-a'];
const REL_COLOR = ['#223040', '#223040', '#7fa6c4', CEILING_COLOR];

// ── 8:8 [container:identity] HHTL-tier layout ────────────────────────────────
// The bake addresses each node as a stack of 8:8 tiers (see src/bin/fma.rs):
// HEEL=[Organ:Heart] HIP=[Chamber:id] TWIG=[Wall:id] LEAF=[Tissue:id]
// family=[Cell:id]. The container (high byte) is the KIND mixin node, the
// identity (low byte) is the instance. The non-zero tier identities ARE the
// partonomy path — so position is read straight off the tiers, no Morton decode:
// y = depth (class), x = a nested slot that subdivides under each parent.
const COL = 1500; // total layout width in vis units
const ROW = 210; // vertical gap per depth level
const POLE_Y = -1.7 * ROW; // the cross-cutting global types hover above the body

/// the instance (low) byte of an 8:8 tier.
const inst = (t: number) => t & 0xff;

// Nested horizontal slot from the [Chamber][Wall][Tissue][Cell] instance path:
// each level subdivides its parent's slot (max-siblings per level: 4/3/2/2), so
// children cluster under their parent. y is the depth band (class).
function tierPos(
  soa: Soa,
  i: number,
): { x: number; y: number } {
  const path: Array<[number, number]> = [
    [inst(soa.hip[i]), 4], // chamber 1..4
    [inst(soa.twig[i]), 3], // wall 1..3
    [inst(soa.leaf[i]), 2], // tissue 1..2
    [inst(soa.family[i]), 2], // cell 1..2
  ];
  let lo = 0;
  let hi = 1;
  for (const [id, n] of path) {
    if (id <= 0) break;
    const w = (hi - lo) / n;
    lo += (id - 1) * w;
    hi = lo + w;
  }
  return { x: ((lo + hi) / 2) * COL, y: soa.cls[i] * ROW };
}

const isCeiling = (soa: Soa, i: number) => soa.ceiling[i] === 1 || soa.cls[i] === 5;

// A node n is INSIDE container c's wall iff its 8:8 tier instances match c's down
// to c's depth — i.e. c's address is a prefix of n's. The address IS the wall.
function inside(soa: Soa, c: number, n: number): boolean {
  const d = soa.cls[c];
  if (soa.cls[n] < d) return false;
  if (d >= 1 && inst(soa.hip[n]) !== inst(soa.hip[c])) return false;
  if (d >= 2 && inst(soa.twig[n]) !== inst(soa.twig[c])) return false;
  if (d >= 3 && inst(soa.leaf[n]) !== inst(soa.leaf[c])) return false;
  return true;
}

interface Wall {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
  color: string;
  depth: number;
}

// One nested "outer wall" rectangle per container (Organ→Tissue; cells are
// leaves). Each box bounds its tier-prefix descendants, so the walls nest exactly
// like the partonomy — the Heart's box is the outermost wall (its epicardium).
function containerWalls(soa: Soa): Wall[] {
  const center = new Map<number, { x: number; y: number }>();
  for (let i = 0; i < soa.nodeCount; i++) if (!isCeiling(soa, i)) center.set(i, tierPos(soa, i));
  const walls: Wall[] = [];
  for (let c = 0; c < soa.nodeCount; c++) {
    if (isCeiling(soa, c) || soa.cls[c] > 3) continue; // only Organ..Tissue contain
    let x0 = Infinity;
    let y0 = Infinity;
    let x1 = -Infinity;
    let y1 = -Infinity;
    let found = false;
    for (const [n, p] of center) {
      if (!inside(soa, c, n)) continue;
      found = true;
      x0 = Math.min(x0, p.x);
      y0 = Math.min(y0, p.y);
      x1 = Math.max(x1, p.x);
      y1 = Math.max(y1, p.y);
    }
    if (!found) continue;
    const pad = 42 - soa.cls[c] * 7; // coarser container → roomier wall
    walls.push({ x0: x0 - pad, y0: y0 - pad, x1: x1 + pad, y1: y1 + pad, color: classColor(soa.cls[c]), depth: soa.cls[c] });
  }
  return walls.sort((a, b) => a.depth - b.depth); // coarsest drawn first (behind)
}

// Stroke a rounded rect in vis-network coordinates (the beforeDrawing ctx is
// already in network space, so it aligns with node positions).
function strokeWall(ctx: CanvasRenderingContext2D, w: Wall): void {
  const r = 16;
  ctx.beginPath();
  ctx.moveTo(w.x0 + r, w.y0);
  ctx.arcTo(w.x1, w.y0, w.x1, w.y1, r);
  ctx.arcTo(w.x1, w.y1, w.x0, w.y1, r);
  ctx.arcTo(w.x0, w.y1, w.x0, w.y0, r);
  ctx.arcTo(w.x0, w.y0, w.x1, w.y0, r);
  ctx.closePath();
  ctx.fillStyle = `${w.color}0f`; // very faint compartment fill
  ctx.fill();
  ctx.lineWidth = w.depth === 0 ? 2.6 : 1.4; // the Heart's outer wall is boldest
  ctx.strokeStyle = `${w.color}66`;
  ctx.stroke();
}

const OPTIONS: Options = {
  nodes: { shape: 'dot', borderWidth: 2.5, font: { color: '#d9e9f9', size: 13, strokeWidth: 3, strokeColor: PAGE_BG } },
  edges: {
    color: { color: 'rgba(125,162,186,0.3)', inherit: false },
    font: { color: 'rgba(147,169,191,0.55)', size: 9, strokeWidth: 0, align: 'middle' },
    width: 1.1,
    smooth: { enabled: true, type: 'continuous', roundness: 0.2 },
    arrows: { to: { enabled: true, scaleFactor: 0.45 } },
  },
  // positions are fixed 8:8-tier slots (see tierPos) — no force simulation.
  physics: { enabled: false },
  interaction: { hover: true, tooltipDelay: 90, dragNodes: true },
  layout: { improvedLayout: false },
};

interface Trace {
  node: string;
  partOf: string[];
  type: string | null;
  shared: number;
}

export function FmaGraph() {
  const hostRef = useRef<HTMLDivElement>(null);
  const [soa, setSoa] = useState<Soa | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState('loading FMA slice…');
  const [trace, setTrace] = useState<Trace | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetch('/fma.soa')
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.arrayBuffer();
      })
      .then((buf) => !cancelled && setSoa(decodeSoa(buf)))
      .catch((e: unknown) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, []);

  // part-of parent (rel 2) and is-a global type (rel 3) per node, + how many
  // tissues each type gathers (its cross-cutting reach).
  const rel = useMemo(() => {
    if (!soa) return null;
    const parentOf = new Map<number, number>();
    const typeOf = new Map<number, number>();
    const typeMembers = new Map<number, number>();
    for (const e of soa.edges) {
      if (e.r === 2) parentOf.set(e.s, e.t);
      else if (e.r === 3) {
        typeOf.set(e.s, e.t);
        typeMembers.set(e.t, (typeMembers.get(e.t) ?? 0) + 1);
      }
    }
    return { parentOf, typeOf, typeMembers };
  }, [soa]);

  useEffect(() => {
    if (!hostRef.current || !soa || !rel) return;
    const ceiling = (i: number) => soa.ceiling[i] === 1 || soa.cls[i] === 5;

    // Fixed position per node, read straight off the 8:8 [container:identity]
    // HHTL tiers: part-of nodes nest by their [Chamber][Wall][Tissue][Cell]
    // instance path (y = depth = class); the cross-cutting global types line up
    // along the pole above the body, spread across the same width.
    const poleNodes = Array.from({ length: soa.nodeCount }, (_, i) => i).filter(ceiling);
    const posOf = (i: number): { x: number; y: number; size: number } => {
      if (ceiling(i)) {
        const k = poleNodes.indexOf(i);
        const x = ((k + 0.5) / Math.max(poleNodes.length, 1)) * COL;
        return { x, y: POLE_Y, size: 22 };
      }
      const { x, y } = tierPos(soa, i);
      return { x, y, size: 30 - soa.cls[i] * 4 }; // coarser tier → larger dot
    };

    const baseNode = (i: number) => {
      const p = posOf(i);
      return {
        id: i,
        label: soa.labels[i] || `#${i}`,
        x: p.x,
        y: p.y,
        fixed: { x: true, y: true }, // the address is the layout — pin it
        shape: ceiling(i) ? 'diamond' : 'dot',
        color: {
          background: ceiling(i) ? 'rgba(255,209,102,0.14)' : 'rgba(10,14,23,0.88)',
          border: ceiling(i) ? CEILING_COLOR : classColor(soa.cls[i]),
        },
        size: p.size,
        font: { color: ceiling(i) ? '#ffe9b0' : '#d9e9f9' },
        title: `${soa.labels[i]}\n${ceiling(i) ? '◈ global type (leaf-limited, cross-cutting)' : FMA_CLASS[soa.cls[i]]?.name}`,
      };
    };
    const baseEdge = (e: { s: number; t: number; r: number }, id: number) => ({
      id,
      from: e.s,
      to: e.t,
      label: REL[e.r] ?? '',
      color: { color: `${REL_COLOR[e.r] ?? '#8fa6c4'}66`, highlight: REL_COLOR[e.r] ?? '#4dd0e1' },
      dashes: e.r === 3 ? [4, 3] : false,
    });

    const visNodes = new DataSet<any>(Array.from({ length: soa.nodeCount }, (_, i) => baseNode(i)));
    const visEdges = new DataSet<any>(soa.edges.map((e, id) => baseEdge(e, id)));
    const net = new Network(hostRef.current, { nodes: visNodes, edges: visEdges }, OPTIONS);
    // nested "outer walls" — one rounded box per container, drawn behind the
    // nodes; they nest exactly like the partonomy (the address IS the wall).
    const walls = containerWalls(soa);
    net.on('beforeDrawing', (ctx: CanvasRenderingContext2D) => {
      for (const w of walls) strokeWall(ctx, w);
    });
    // fixed 8:8-tier slots, no simulation — just frame the nested cascade.
    net.once('afterDrawing', () => net.fit({ animation: false }));
    setStatus(`${soa.nodeCount} nodes · ${soa.edgeCount} edges — Z-order tile pyramid; click a tissue for its dual membership`);

    const dim = () => {
      visNodes.update(Array.from({ length: soa.nodeCount }, (_, i) => ({ id: i, color: { background: 'rgba(10,14,23,0.5)', border: '#26323f' }, font: { color: '#566779' } })));
      visEdges.update(soa.edges.map((_, id) => ({ id, color: { color: 'rgba(50,66,84,0.1)' } })));
    };
    const restore = () => {
      visNodes.update(Array.from({ length: soa.nodeCount }, (_, i) => baseNode(i)));
      visEdges.update(soa.edges.map((e, id) => baseEdge(e, id)));
      setTrace(null);
    };
    const bright = (i: number) => visNodes.update({ id: i, color: { background: 'rgba(10,14,23,0.96)', border: ceiling(i) ? CEILING_COLOR : '#9fe8ff' }, font: { color: '#eaf4ff' } });
    const litEdge = (from: number, to: number, c: string) => {
      const hit = soa.edges.findIndex((e) => e.s === from && e.t === to);
      if (hit >= 0) visEdges.update({ id: hit, color: { color: c }, width: 3 });
    };

    net.on('click', (p: { nodes: unknown[] }) => {
      if (!p.nodes.length) {
        restore();
        return;
      }
      const seed = p.nodes[0] as number;
      dim();
      bright(seed);
      // part-of chain up to the organ (basin-local position).
      const partOf: string[] = [];
      let cur = seed;
      for (let hop = 0; hop < 12; hop++) {
        const parent = rel.parentOf.get(cur);
        if (parent == null) break;
        litEdge(cur, parent, '#6cf0ff');
        bright(parent);
        partOf.push(soa.labels[parent]);
        cur = parent;
      }
      // is-a leaf-limited global type (the ceiling pole, cross-cutting).
      const ty = rel.typeOf.get(seed);
      if (ty != null) {
        litEdge(seed, ty, CEILING_COLOR);
        bright(ty);
      }
      setTrace({
        node: soa.labels[seed],
        partOf,
        type: ty != null ? soa.labels[ty] : null,
        shared: ty != null ? rel.typeMembers.get(ty) ?? 0 : 0,
      });
    });

    return () => net.destroy();
  }, [soa, rel]);

  return (
    <div style={{ position: 'relative', width: '100%', height: '100vh', background: PAGE_BG, overflow: 'hidden' }}>
      <div ref={hostRef} style={{ position: 'absolute', inset: 0, zIndex: 0 }} />
      <div style={{ position: 'absolute', top: 16, left: 16, zIndex: 10, fontFamily: 'monospace', color: '#93a9bf', fontSize: 12, pointerEvents: 'none', textShadow: '0 0 4px #0a0e17' }}>
        <div style={{ fontSize: 14, color: '#cfe7ff' }}>FMA heart slice · part-of basin × leaf-limited global type</div>
        <div>{error ? <span style={{ color: '#ff637d' }}>load error: {error}</span> : status}</div>
      </div>

      {trace && (
        <div style={{ position: 'absolute', left: 16, bottom: 56, width: 380, zIndex: 10, fontFamily: 'monospace', fontSize: 11, color: '#cfe7ff', background: 'rgba(8,12,20,0.88)', border: '1px solid #2a4a6a', borderRadius: 8, padding: '10px 12px' }}>
          <div style={{ color: '#7fd1ff', marginBottom: 6 }}>◎ {trace.node}</div>
          <div style={{ color: '#9fb4c8' }}>part-of (basin-local position):</div>
          <div style={{ marginBottom: 6 }}>{trace.partOf.length ? trace.partOf.join(' › ') : '(root)'}</div>
          <div style={{ color: '#9fb4c8' }}>is-a (leaf-limited global type — ceiling pole):</div>
          <div style={{ color: trace.type ? CEILING_COLOR : '#566779' }}>
            {trace.type ? `◈ ${trace.type}  · cross-cuts ${trace.shared} chambers` : '— (no global type at this grain)'}
          </div>
        </div>
      )}

      {/* legend */}
      <div style={{ position: 'absolute', bottom: 16, left: 16, zIndex: 10, fontFamily: 'monospace', fontSize: 11, color: '#93a9bf', pointerEvents: 'none', textShadow: '0 0 4px #0a0e17' }}>
        {FMA_CLASS.map((k) => (
          <span key={k.name} style={{ marginRight: 12, whiteSpace: 'nowrap' }}>
            <span style={{ display: 'inline-block', width: 9, height: 9, borderRadius: k.name.startsWith('Type') ? 0 : 9, border: `2px solid ${k.color}`, marginRight: 5, verticalAlign: 'middle', transform: k.name.startsWith('Type') ? 'rotate(45deg)' : 'none' }} />
            {k.name}
          </span>
        ))}
      </div>

      <a href="/osint" style={{ position: 'absolute', top: 14, right: 16, zIndex: 10, fontFamily: 'monospace', fontSize: 12, color: '#cfe7ff', background: 'rgba(17,32,48,0.7)', border: '1px solid #2a4a6a', borderRadius: 6, padding: '6px 12px', textDecoration: 'none' }}>
        ← OSINT graph
      </a>
    </div>
  );
}
