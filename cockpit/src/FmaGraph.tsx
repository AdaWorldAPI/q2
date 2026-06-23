// FMA anatomy slice — the "real test" of the dual-membership lattice. Decodes
// the baked /fma.soa (heart organ subtree + cross-cutting tissue TYPES) into the
// same vis-network renderer, and on click shows a node resolving to BOTH:
//   · its part-of position (basin-local: organ → chamber → wall → structure)
//   · its leaf-limited global TYPE (the 0xFFFF ceiling pole — cross-cutting,
//     the same "Cardiac muscle tissue" shared by every chamber).
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

const OPTIONS: Options = {
  nodes: { shape: 'dot', borderWidth: 2.5, font: { color: '#d9e9f9', size: 13, strokeWidth: 3, strokeColor: PAGE_BG } },
  edges: {
    color: { color: 'rgba(125,162,186,0.3)', inherit: false },
    font: { color: 'rgba(147,169,191,0.55)', size: 9, strokeWidth: 0, align: 'middle' },
    width: 1.1,
    smooth: { enabled: true, type: 'continuous', roundness: 0.2 },
    arrows: { to: { enabled: true, scaleFactor: 0.45 } },
  },
  physics: {
    solver: 'forceAtlas2Based',
    forceAtlas2Based: { gravitationalConstant: -70, centralGravity: 0.008, springLength: 130, springConstant: 0.04, damping: 0.5, avoidOverlap: 0.5 },
    stabilization: { iterations: 180, fit: true },
  },
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
    const baseNode = (i: number) => ({
      id: i,
      label: soa.labels[i] || `#${i}`,
      shape: ceiling(i) ? 'diamond' : 'dot',
      color: {
        background: ceiling(i) ? 'rgba(255,209,102,0.14)' : 'rgba(10,14,23,0.88)',
        border: ceiling(i) ? CEILING_COLOR : classColor(soa.cls[i]),
      },
      size: ceiling(i) ? 22 : 13,
      font: { color: ceiling(i) ? '#ffe9b0' : '#d9e9f9' },
      title: `${soa.labels[i]}\n${ceiling(i) ? '◈ global type (leaf-limited, cross-cutting)' : FMA_CLASS[soa.cls[i]]?.name}`,
    });
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
    net.once('stabilizationIterationsDone', () => {
      net.setOptions({ physics: { enabled: false } });
      setStatus(`${soa.nodeCount} nodes · ${soa.edgeCount} edges — click a tissue to see its dual membership`);
    });

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
