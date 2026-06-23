// Sleek OSINT graph — the SAME vis-network renderer the Palantir cockpit uses
// (hollow ring nodes, smooth "wobbly" edges, edge labels, force layout), but
// REROUTED to the SoA: it decodes the baked `/osint.soa` bytes (920 nodes /
// 3344 edges) instead of the 221-node aiwar JSON. The 3D CAM scene lives on at
// /osint3d as the alternative view; this is the default for its cleaner appeal.
//
// The reasoning is wired in: search an entity (or "trump + anthropic" for the
// path between two), or fire an analysis lens, and the NARS trace blooms as
// edge highlights over the graph while a readout box streams the traversal.
import { useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { Network, type Options } from 'vis-network';
import { DataSet } from 'vis-data';

const HUB_CLASS = 0xff;
const PAGE_BG = '#0a0e17';
const TARGET = 40; // theories that survive the prune
const MAX_LINES = 48; // cap the readout — streaming 170 lines froze the menu
const SPREAD_MAX = 64; // bound the BFS fallback so a hub doesn't light the world
const INFER_TIMEOUT_MS = 2500; // don't hang the UI on a slow /api/graph/infer

// OSINT class order → colour + name (matches OSINT_SCHEMA in osint_gotham.rs).
const CLASS = [
  { name: 'System', color: '#4dd0e1' },
  { name: 'Stakeholder', color: '#ffb547' },
  { name: 'Person', color: '#35d07f' },
  { name: 'CivicSystem', color: '#9b8cff' },
  { name: 'HistoricalSystem', color: '#ff637d' },
  { name: 'SchemaValue', color: '#5a6b7f' },
  { name: 'SchemaAxis', color: '#c792ea' },
];
const classColor = (c: number) => (c === HUB_CLASS ? '#dfe9f5' : CLASS[c]?.color ?? '#8899aa');

// The six analysis lenses (same as the cockpit) → seed class. Civil Engineering
// is the civic lens; AI Development / Surveillance both read System (distinct hubs).
const ANGLES: Array<{ name: string; cls: number }> = [
  { name: 'Economic Review', cls: 1 },
  { name: 'Civil Engineering', cls: 3 },
  { name: 'Political Dynamics', cls: 2 },
  { name: 'AI Development', cls: 0 },
  { name: 'Kill Chain', cls: 4 },
  { name: 'Surveillance', cls: 0 },
];

// rel byte → relation name + colour (rel_code in osint_gotham.rs). 0/1 are the
// basin scaffold; 2..9 are the real neo4j relations (the VIEW we render).
const REL_NAME = [
  'member-of', 'interfaces', 'CONNECTED_TO', 'DEVELOPED_BY', 'DEPLOYED_BY',
  'PERSON_LINK', 'USED_IN', 'HIERARCHICAL', 'VALID_FOR', 'related',
];
const REL_COLOR = [
  '#223040', '#223040', '#4dd0e1', '#ffb547', '#35d07f',
  '#ff637d', '#9b8cff', '#c792ea', '#7fd1c7', '#8fa6c4',
];

const DIM_NODE = { background: 'rgba(10,14,23,0.55)', border: '#26323f' };
const DIM_EDGE = 'rgba(50,66,84,0.12)';
const ACTIVE = '#6cf0ff';

interface Soa {
  nodeCount: number;
  edgeCount: number;
  cls: Uint8Array;
  edges: Array<{ s: number; t: number; r: number }>;
  labels: string[];
}

/** One readable step of the reasoning traversal, streamed into the readout. */
interface ReasonLine {
  text: string;
  conf: number;
  survived: boolean;
}
interface Readout {
  seed: string;
  kind: 'nars' | 'path' | 'spread';
  lines: ReasonLine[];
  theories: number;
}
/** Imperative handle the controls use to drive the live network. */
interface GraphApi {
  query: (text: string) => 'seed' | 'path' | 'not-found';
  fireLens: (angleIdx: number) => void;
  clear: () => void;
}

// Decode the OSO1 wire: magic(4) | nodeCount u32 | edgeCount u32 |
// nodeCount×[guid:16|class:1] | edgeCount×[src:u16|tgt:u16|rel:u8] |
// nodeCount×[len:u8|utf8 name]  (the label tail is additive / may be absent).
function decodeSoa(buf: ArrayBuffer): Soa {
  const dv = new DataView(buf);
  const magicOk =
    dv.getUint8(0) === 0x4f && dv.getUint8(1) === 0x53 &&
    dv.getUint8(2) === 0x4f && dv.getUint8(3) === 0x31;
  if (!magicOk) throw new Error('bad SoA magic (expected OSO1)');
  const nodeCount = dv.getUint32(4, true);
  const edgeCount = dv.getUint32(8, true);
  let off = 12;
  const cls = new Uint8Array(nodeCount);
  for (let i = 0; i < nodeCount; i++) {
    cls[i] = dv.getUint8(off + 16);
    off += 17;
  }
  const edges: Array<{ s: number; t: number; r: number }> = [];
  for (let i = 0; i < edgeCount; i++) {
    edges.push({
      s: dv.getUint16(off, true),
      t: dv.getUint16(off + 2, true),
      r: dv.getUint8(off + 4),
    });
    off += 5;
  }
  const labels: string[] = new Array(nodeCount).fill('');
  if (off < dv.byteLength) {
    const dec = new TextDecoder();
    for (let i = 0; i < nodeCount && off < dv.byteLength; i++) {
      const len = dv.getUint8(off);
      off += 1;
      labels[i] = dec.decode(new Uint8Array(buf, off, len));
      off += len;
    }
  }
  return { nodeCount, edgeCount, cls, edges, labels };
}

// vis-network options tuned to the Palantir look: hollow ring nodes (dark fill
// + coloured border), smooth curved edges (the "wobbly"), faint edge labels,
// force-directed spread.
const NETWORK_OPTIONS: Options = {
  nodes: {
    shape: 'dot',
    borderWidth: 2.5,
    font: {
      color: '#d9e9f9',
      face: 'Inter, ui-sans-serif, system-ui, sans-serif',
      size: 13,
      strokeWidth: 3,
      strokeColor: PAGE_BG,
    },
    shadow: { enabled: true, color: 'rgba(0,0,0,0.55)', size: 12, x: 0, y: 3 },
  },
  edges: {
    color: { color: 'rgba(125,162,186,0.32)', highlight: '#4dd0e1', hover: 'rgba(77,208,225,0.7)', inherit: false },
    font: {
      color: 'rgba(147,169,191,0.6)',
      size: 9,
      face: 'Inter, ui-sans-serif, system-ui, sans-serif',
      strokeWidth: 0,
      align: 'middle',
    },
    width: 1.1,
    smooth: { enabled: true, type: 'continuous', roundness: 0.18 },
    selectionWidth: 2.4,
    hoverWidth: 0.5,
    arrows: { to: { enabled: true, scaleFactor: 0.45, type: 'arrow' } },
  },
  physics: {
    solver: 'forceAtlas2Based',
    forceAtlas2Based: {
      gravitationalConstant: -64,
      centralGravity: 0.006,
      springLength: 150,
      springConstant: 0.035,
      damping: 0.5,
      avoidOverlap: 0.4,
    },
    stabilization: { iterations: 160, fit: true },
  },
  interaction: {
    hover: true,
    tooltipDelay: 90,
    zoomView: true,
    dragView: true,
    dragNodes: true,
    navigationButtons: false,
    keyboard: false,
  },
  layout: { improvedLayout: false }, // 900+ nodes — skip the O(n²) refinement pass
};

/** The streaming readout box — watches the traversal name itself line by line. */
function ReasonBox({ readout, onClose }: { readout: Readout; onClose: () => void }) {
  const [shown, setShown] = useState(0);
  const scrollRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    setShown(0);
    let n = 0;
    const id = setInterval(() => {
      n += 1;
      setShown(n);
      if (n >= readout.lines.length) clearInterval(id);
    }, 30); // stagger ≈ the bloom cadence — the "traversal"
    return () => clearInterval(id);
  }, [readout]);
  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [shown]);
  const lines = readout.lines.slice(0, shown);
  const head = readout.kind === 'nars' ? '⟳ NARS trace' : readout.kind === 'path' ? '⇄ path' : '⟳ spread';
  return (
    <div
      style={{
        position: 'absolute',
        left: 16,
        bottom: 56,
        width: 400,
        maxHeight: '46%',
        display: 'flex',
        flexDirection: 'column',
        fontFamily: 'monospace',
        fontSize: 11,
        color: '#cfe7ff',
        background: 'rgba(8,12,20,0.86)',
        border: '1px solid #2a4a6a',
        borderRadius: 8,
        padding: '10px 12px',
        boxShadow: '0 6px 24px rgba(0,0,0,0.4)',
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 6 }}>
        <span style={{ color: '#7fd1ff' }}>
          {head} · {readout.seed}
        </span>
        <span onClick={onClose} style={{ cursor: 'pointer', color: '#6f87a0' }}>
          ✕
        </span>
      </div>
      <div ref={scrollRef} style={{ overflowY: 'auto', lineHeight: 1.5 }}>
        {lines.map((l, i) => (
          <div
            key={i}
            style={{
              color: l.survived ? '#cfe7ff' : '#566779',
              whiteSpace: 'nowrap',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
            }}
          >
            <span style={{ color: l.survived ? '#35d07f' : '#3a4a5a' }}>{l.conf.toFixed(2)}</span> {l.text}
          </div>
        ))}
      </div>
      <div style={{ marginTop: 6, color: '#7f97b0', borderTop: '1px solid #1b2c3e', paddingTop: 4 }}>
        {readout.lines.length} steps → {readout.theories} {readout.kind === 'path' ? 'hops' : 'theories'}
      </div>
    </div>
  );
}

/** Default view: the SoA decoded into the Palantir vis-network renderer + reasoning. */
export function OsintGraph() {
  const hostRef = useRef<HTMLDivElement>(null);
  const netRef = useRef<Network | null>(null);
  const apiRef = useRef<GraphApi | null>(null);
  const [soa, setSoa] = useState<Soa | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState('loading SoA…');
  const [readout, setReadout] = useState<Readout | null>(null);
  const [search, setSearch] = useState('');
  const [angle, setAngle] = useState<number | null>(null);

  // Fetch + decode the SoA once.
  useEffect(() => {
    let cancelled = false;
    fetch('/osint.soa')
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.arrayBuffer();
      })
      .then((buf) => {
        if (cancelled) return;
        setSoa(decodeSoa(buf));
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // The semantic VIEW: only the real neo4j relations (rel ≥ 2) and the nodes
  // they touch — the connected entity graph, not the schema scaffold.
  const view = useMemo(() => {
    if (!soa) return null;
    const semantic = soa.edges
      .filter((e) => e.r >= 2 && e.s < soa.nodeCount && e.t < soa.nodeCount)
      .map((e, id) => ({ ...e, id }));
    const degree = new Map<number, number>();
    const touched = new Set<number>();
    for (const e of semantic) {
      touched.add(e.s);
      touched.add(e.t);
      degree.set(e.s, (degree.get(e.s) ?? 0) + 1);
      degree.set(e.t, (degree.get(e.t) ?? 0) + 1);
    }
    return { semantic, degree, touched };
  }, [soa]);

  // Build the network + the reasoning API whenever the data lands.
  useEffect(() => {
    if (!hostRef.current || !soa || !view) return;
    const { degree, touched, semantic } = view;

    const baseSize = (i: number) => 11 + Math.min(degree.get(i) ?? 1, 16) * 1.5;
    const baseNode = (i: number) => ({
      id: i,
      label: soa.labels[i] || `#${i}`,
      color: {
        background: 'rgba(10,14,23,0.88)',
        border: classColor(soa.cls[i]),
        highlight: { background: 'rgba(10,14,23,0.96)', border: '#9fe8ff' },
        hover: { background: 'rgba(10,14,23,0.82)', border: classColor(soa.cls[i]) },
      },
      size: baseSize(i),
      font: { color: '#d9e9f9' },
      title: `${soa.labels[i] || `#${i}`}\n${CLASS[soa.cls[i]]?.name ?? 'concept'} · ${degree.get(i) ?? 0} links`,
    });
    const baseEdge = (e: { id: number; s: number; t: number; r: number }) => ({
      id: e.id,
      from: e.s,
      to: e.t,
      label: REL_NAME[e.r] ?? 'related',
      color: { color: `${REL_COLOR[e.r] ?? '#8fa6c4'}55`, highlight: REL_COLOR[e.r] ?? '#4dd0e1' },
      width: 1.1,
      dashes: false,
      font: { color: 'rgba(147,169,191,0.6)' },
    });

    // vis-network DataSets — typed loosely (`any`) because we add inferred
    // edges with string ids and partial colour/dashes updates over the run.
    const visNodes = new DataSet<any>(Array.from(touched).map(baseNode));
    const visEdges = new DataSet<any>(semantic.map(baseEdge));

    // adjacency over the rendered semantic edges (for path-finding).
    const adj = new Map<number, Array<{ to: number; rel: number; edgeId: number }>>();
    const push = (a: number, b: number, rel: number, edgeId: number) => {
      const arr = adj.get(a);
      if (arr) arr.push({ to: b, rel, edgeId });
      else adj.set(a, [{ to: b, rel, edgeId }]);
    };
    for (const e of semantic) {
      push(e.s, e.t, e.r, e.id);
      push(e.t, e.s, e.r, e.id);
    }
    // name → node id (the same resolution the bake used to build the edges).
    const nameToId = new Map<string, number>();
    touched.forEach((i) => {
      if (soa.labels[i]) nameToId.set(soa.labels[i], i);
    });
    // top-degree node per class (lens seeds).
    const topByClass = new Map<number, number>();
    Array.from(touched)
      .sort((a, b) => (degree.get(b) ?? 0) - (degree.get(a) ?? 0))
      .forEach((i) => {
        if (!topByClass.has(soa.cls[i])) topByClass.set(soa.cls[i], i);
      });

    setStatus(`laying out ${visNodes.length} nodes · ${visEdges.length} relations…`);
    const net = new Network(hostRef.current, { nodes: visNodes, edges: visEdges }, NETWORK_OPTIONS);
    net.once('stabilizationIterationsDone', () => {
      net.setOptions({ physics: { enabled: false } });
      setStatus(`${visNodes.length} nodes · ${visEdges.length} relations`);
    });
    netRef.current = net;

    // ── reasoning over the network ──────────────────────────────────────────
    let addedEdges: string[] = []; // inferred (NARS) edges, removed on clear
    let pruneTimer: ReturnType<typeof setTimeout> | null = null;
    let reasonGen = 0; // bumped per reason; a stale async result bails
    let activeAbort: AbortController | null = null;

    // cheap teardown of the PREVIOUS trace (no full-graph repaint — dimAll will
    // overwrite every colour anyway, so repainting to base first was wasted work
    // and, doubled with dimAll, part of the click lag).
    const clearAdded = () => {
      if (pruneTimer) {
        clearTimeout(pruneTimer);
        pruneTimer = null;
      }
      if (addedEdges.length) {
        visEdges.remove(addedEdges);
        addedEdges = [];
      }
    };
    // full restore to base styling — only the user "clear" needs this.
    const restore = () => {
      clearAdded();
      visNodes.update(Array.from(touched).map(baseNode));
      visEdges.update(semantic.map(baseEdge));
    };
    const dimAll = () => {
      visNodes.update(Array.from(touched).map((i) => ({ id: i, color: DIM_NODE, font: { color: '#5a6b7f' } })));
      visEdges.update(semantic.map((e) => ({ id: e.id, color: { color: DIM_EDGE }, width: 0.6, font: { color: 'rgba(0,0,0,0)' } })));
    };
    const brighten = (id: number) => {
      visNodes.update({
        id,
        color: { background: 'rgba(10,14,23,0.95)', border: classColor(soa.cls[id]) },
        font: { color: '#eaf4ff' },
      });
    };
    const focusOn = (ids: number[]) => {
      if (ids.length) net.fit({ nodes: ids, animation: { duration: 600, easingFunction: 'easeInOutQuad' } });
    };
    const confColor = (c: number) => `rgba(108,240,255,${(0.32 + 0.62 * c).toFixed(3)})`;

    const reasonFrom = async (seedId: number) => {
      const nm = soa.labels[seedId];
      const gen = ++reasonGen; // claim this run
      activeAbort?.abort(); // cancel any in-flight infer from a prior click
      clearAdded();
      let infs: Array<{ source: string; target: string; relation?: string; via?: string[]; truth_f?: number; truth_c?: number }> = [];
      if (nm) {
        const ac = new AbortController();
        activeAbort = ac;
        const to = setTimeout(() => ac.abort(), INFER_TIMEOUT_MS);
        try {
          const r = await fetch('/api/graph/infer', {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({ node_id: nm, max_hops: 3, min_confidence: 0.25 }),
            signal: ac.signal,
          });
          if (r.ok) infs = (await r.json()).inferences ?? [];
        } catch {
          /* timeout / abort / endpoint down → spread fallback below */
        } finally {
          clearTimeout(to);
        }
        if (gen !== reasonGen) return; // a newer reason superseded us — bail
      }
      // Map inferences → add inferred edges; collect lines + scores.
      const ids = new Set<number>([seedId]);
      const lines: ReasonLine[] = [];
      const edgeOps: Array<{ id: string; from: number; to: number; conf: number }> = [];
      let k = 0;
      for (const inf of infs) {
        const a = nameToId.get(inf.source);
        const c = nameToId.get(inf.target);
        if (a == null || c == null) continue;
        const tc = Math.max(0, Math.min(1, Number(inf.truth_c) || 0));
        edgeOps.push({ id: `inf-${k++}`, from: a, to: c, conf: tc });
        const via = inf.via && inf.via.length ? ` · via ${inf.via.join(', ')}` : '';
        lines.push({ text: `${inf.source} —${inf.relation ?? 'infers'}→ ${inf.target}${via}`, conf: tc, survived: false });
      }

      if (!edgeOps.length) {
        spreadFrom(seedId); // no NARS edges mapped → literal-neighbourhood spread
        return;
      }

      // keep only the strongest MAX_LINES — streaming the full set froze the UI.
      const order = edgeOps
        .map((_, i) => i)
        .sort((a, b) => edgeOps[b].conf - edgeOps[a].conf)
        .slice(0, MAX_LINES);
      const ops = order.map((i) => edgeOps[i]);
      const capLines = order.map((i) => lines[i]);
      const thr = ops.map((o) => o.conf).sort((x, y) => y - x)[Math.min(TARGET, ops.length) - 1] ?? 0;

      dimAll();
      brighten(seedId);
      ops.forEach((o) => {
        visEdges.add({
          id: o.id,
          from: o.from,
          to: o.to,
          color: { color: confColor(o.conf) },
          width: 1 + 2 * o.conf,
          dashes: [4, 3],
        });
        addedEdges.push(o.id);
        ids.add(o.from);
        ids.add(o.to);
        brighten(o.from);
        brighten(o.to);
      });
      capLines.forEach((l) => (l.survived = l.conf >= thr));
      capLines.sort((a, b) => b.conf - a.conf);
      setReadout({ seed: nm || `#${seedId}`, kind: 'nars', lines: capLines, theories: capLines.filter((l) => l.survived).length });
      focusOn(Array.from(ids));
      // prune: fade the below-threshold inferences after a beat.
      pruneTimer = setTimeout(() => {
        ops.forEach((o) => {
          if (o.conf < thr) visEdges.update({ id: o.id, color: { color: 'rgba(50,66,84,0.10)' }, width: 0.5 });
        });
      }, 1500);
    };

    const spreadFrom = (seedId: number) => {
      // heuristic fallback: light the seed's literal neighbourhood, bounded so a
      // hub like "Epstein" doesn't light hundreds of edges and freeze the menu.
      clearAdded();
      dimAll();
      brighten(seedId);
      const lines: ReasonLine[] = [];
      const ids = new Set<number>([seedId]);
      let frontier = [seedId];
      for (let hop = 0; hop < 2 && lines.length < SPREAD_MAX; hop++) {
        const next: number[] = [];
        for (const u of frontier) {
          for (const { to, rel, edgeId } of adj.get(u) ?? []) {
            if (ids.has(to)) continue;
            if (lines.length >= SPREAD_MAX) break;
            ids.add(to);
            next.push(to);
            visEdges.update({ id: edgeId, color: { color: confColor(0.7 - hop * 0.25) }, width: 1.6 });
            brighten(to);
            lines.push({ text: `${soa.labels[u]} —${REL_NAME[rel] ?? 'related'}→ ${soa.labels[to]}`, conf: 0.7 - hop * 0.25, survived: true });
          }
        }
        frontier = next;
      }
      setReadout({ seed: soa.labels[seedId] || `#${seedId}`, kind: 'spread', lines, theories: lines.length });
      focusOn(Array.from(ids));
    };

    const bfsPath = (a: number, b: number): Array<{ from: number; to: number; rel: number; edgeId: number }> | null => {
      if (a === b) return [];
      const prev = new Map<number, { p: number; rel: number; edgeId: number }>();
      const seen = new Set<number>([a]);
      const queue = [a];
      while (queue.length) {
        const u = queue.shift() as number;
        for (const { to, rel, edgeId } of adj.get(u) ?? []) {
          if (seen.has(to)) continue;
          seen.add(to);
          prev.set(to, { p: u, rel, edgeId });
          if (to === b) {
            const out: Array<{ from: number; to: number; rel: number; edgeId: number }> = [];
            let cur = b;
            while (cur !== a) {
              const pr = prev.get(cur) as { p: number; rel: number; edgeId: number };
              out.unshift({ from: pr.p, to: cur, rel: pr.rel, edgeId: pr.edgeId });
              cur = pr.p;
            }
            return out;
          }
          queue.push(to);
        }
      }
      return null;
    };

    const connect = (aId: number, bId: number): boolean => {
      reasonGen++; // invalidate any in-flight infer so it can't clobber the path
      activeAbort?.abort();
      clearAdded();
      const path = bfsPath(aId, bId);
      if (!path) {
        setReadout({ seed: `${soa.labels[aId]} ↮ ${soa.labels[bId]}`, kind: 'path', lines: [{ text: 'no connecting path', conf: 0, survived: false }], theories: 0 });
        return false;
      }
      dimAll();
      brighten(aId);
      brighten(bId);
      const ids = new Set<number>([aId, bId]);
      const lines: ReasonLine[] = path.map(({ from, to, rel, edgeId }) => {
        visEdges.update({ id: edgeId, color: { color: ACTIVE }, width: 3, font: { color: '#cfe7ff' } });
        brighten(from);
        brighten(to);
        ids.add(from);
        ids.add(to);
        return { text: `${soa.labels[from]} —${REL_NAME[rel] ?? 'related'}→ ${soa.labels[to]}`, conf: 1, survived: true };
      });
      setReadout({ seed: `${soa.labels[aId]} → ${soa.labels[bId]}`, kind: 'path', lines, theories: lines.length });
      focusOn(Array.from(ids));
      return true;
    };

    const resolve = (q: string): number | null => {
      const t = q.trim();
      if (!t) return null;
      if (nameToId.has(t)) return nameToId.get(t) as number;
      const lc = t.toLowerCase();
      let prefix: number | null = null;
      let sub: number | null = null;
      for (const i of touched) {
        const nm = soa.labels[i];
        if (!nm) continue;
        const nlc = nm.toLowerCase();
        if (nlc === lc) return i;
        if (prefix == null && nlc.startsWith(lc)) prefix = i;
        else if (sub == null && nlc.includes(lc)) sub = i;
      }
      return prefix ?? sub;
    };

    apiRef.current = {
      query: (text) => {
        const parts = text.split(/[+,&]/).map((s) => s.trim()).filter(Boolean);
        if (!parts.length) return 'not-found';
        if (parts.length === 1) {
          const idx = resolve(parts[0]);
          if (idx == null) return 'not-found';
          void reasonFrom(idx);
          return 'seed';
        }
        const a = resolve(parts[0]);
        const b = resolve(parts[1]);
        if (a == null || b == null) return 'not-found';
        return connect(a, b) ? 'path' : 'not-found';
      },
      fireLens: (ai) => {
        const seed = topByClass.get(ANGLES[ai].cls);
        if (seed != null) void reasonFrom(seed);
      },
      clear: () => {
        reasonGen++;
        activeAbort?.abort();
        restore();
        setReadout(null);
      },
    };

    net.on('click', (params: { nodes: unknown[] }) => {
      if (params.nodes.length) void reasonFrom(params.nodes[0] as number);
    });

    return () => {
      apiRef.current = null;
      net.destroy();
      netRef.current = null;
    };
  }, [soa, view]);

  // ── controls ──────────────────────────────────────────────────────────────
  const runSearch = () => {
    if (!search.trim()) return;
    setAngle(null);
    const r = apiRef.current?.query(search);
    if (r === 'not-found') setStatus(`no match for “${search}”`);
  };
  const fireLens = (i: number) => {
    setAngle(i);
    apiRef.current?.fireLens(i);
  };
  const clearReason = () => {
    setAngle(null);
    apiRef.current?.clear();
  };

  const lensChip = (i: number): CSSProperties => ({
    fontFamily: 'monospace',
    fontSize: 11,
    color: angle === i ? '#0a0e17' : '#cfe7ff',
    background: angle === i ? CLASS[ANGLES[i].cls]?.color ?? '#4dd0e1' : 'rgba(17,32,48,0.6)',
    border: `1px solid ${CLASS[ANGLES[i].cls]?.color ?? '#4dd0e1'}`,
    borderRadius: 6,
    padding: '5px 9px',
    cursor: 'pointer',
    fontWeight: angle === i ? 700 : 400,
  });

  return (
    <div style={{ position: 'relative', width: '100%', height: '100vh', background: PAGE_BG, overflow: 'hidden' }}>
      <div ref={hostRef} style={{ position: 'absolute', inset: 0 }} />

      {/* title + status */}
      <div
        style={{
          position: 'absolute',
          top: 16,
          left: 16,
          fontFamily: 'monospace',
          color: '#93a9bf',
          fontSize: 12,
          pointerEvents: 'none',
          textShadow: '0 0 4px #0a0e17',
        }}
      >
        <div style={{ fontSize: 14, color: '#cfe7ff' }}>OSINT · classid 0x0700 · SoA → graph</div>
        <div>{error ? <span style={{ color: '#ff637d' }}>load error: {error}</span> : status}</div>
      </div>

      {/* search + lenses */}
      <div
        style={{
          position: 'absolute',
          top: 58,
          left: 16,
          width: 320,
          fontFamily: 'monospace',
          display: 'flex',
          flexDirection: 'column',
          gap: 8,
        }}
      >
        <div style={{ display: 'flex', gap: 6 }}>
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') runSearch();
            }}
            placeholder="trump + anthropic"
            style={{
              flex: 1,
              minWidth: 0,
              fontFamily: 'monospace',
              fontSize: 12,
              color: '#cfe7ff',
              background: 'rgba(8,12,20,0.8)',
              border: '1px solid #2a4a6a',
              borderRadius: 6,
              padding: '6px 8px',
            }}
          />
          <button
            onClick={runSearch}
            style={{
              fontFamily: 'monospace',
              fontSize: 12,
              color: '#0a0e17',
              background: '#4dd0e1',
              border: '1px solid #4dd0e1',
              borderRadius: 6,
              padding: '6px 12px',
              cursor: 'pointer',
              fontWeight: 700,
            }}
          >
            ▸ trace
          </button>
        </div>
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 5 }}>
          {ANGLES.map((a, i) => (
            <button key={a.name} onClick={() => fireLens(i)} style={lensChip(i)}>
              {a.name}
            </button>
          ))}
          {readout && (
            <button
              onClick={clearReason}
              style={{
                fontFamily: 'monospace',
                fontSize: 11,
                color: '#93a9bf',
                background: 'transparent',
                border: '1px solid #243a52',
                borderRadius: 6,
                padding: '5px 9px',
                cursor: 'pointer',
              }}
            >
              clear
            </button>
          )}
        </div>
        <div style={{ fontSize: 10, color: '#6f87a0' }}>
          one entity reasons from it; “A + B” traces the path. click any node to reason.
        </div>
      </div>

      {readout && <ReasonBox readout={readout} onClose={clearReason} />}

      {/* class legend */}
      <div
        style={{
          position: 'absolute',
          bottom: 16,
          left: 16,
          fontFamily: 'monospace',
          fontSize: 11,
          color: '#93a9bf',
          pointerEvents: 'none',
          textShadow: '0 0 4px #0a0e17',
        }}
      >
        {CLASS.map((k) => (
          <span key={k.name} style={{ marginRight: 12, whiteSpace: 'nowrap' }}>
            <span
              style={{
                display: 'inline-block',
                width: 9,
                height: 9,
                borderRadius: 9,
                border: `2px solid ${k.color}`,
                marginRight: 5,
                verticalAlign: 'middle',
              }}
            />
            {k.name}
          </span>
        ))}
      </div>

      {/* alternative-view link */}
      <a
        href="/osint3d"
        style={{
          position: 'absolute',
          top: 14,
          right: 16,
          fontFamily: 'monospace',
          fontSize: 12,
          color: '#cfe7ff',
          background: 'rgba(17,32,48,0.7)',
          border: '1px solid #2a4a6a',
          borderRadius: 6,
          padding: '6px 12px',
          textDecoration: 'none',
        }}
      >
        ◳ 3D reasoning view →
      </a>
    </div>
  );
}
