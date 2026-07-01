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
// 0/1 are the basin scaffold; 2..9 the real neo4j relations; 10..15 are the
// dual-use FACET edges (entity → its SchemaValue), the dimensions made
// traversable — the toggleable "dimension layer".
const REL_NAME = [
  'member-of', 'interfaces', 'CONNECTED_TO', 'DEVELOPED_BY', 'DEPLOYED_BY',
  'PERSON_LINK', 'USED_IN', 'HIERARCHICAL', 'VALID_FOR', 'related',
  'militaryUse', 'civicUse', 'airo:type', 'MLType', 'purpose', 'capacity',
];
const REL_COLOR = [
  '#4a6a8c', '#3f5a78', '#4dd0e1', '#ffb547', '#35d07f',
  '#ff637d', '#9b8cff', '#c792ea', '#7fd1c7', '#8fa6c4',
  '#ff637d', '#35d07f', '#c792ea', '#7fd1ff', '#ffb547', '#9b8cff',
];
// rel codes that make up the dimension layer: VALID_FOR (8) + the facets (10..15).
const isFacetRel = (r: number) => r === 8 || (r >= 10 && r <= 15);

// The dual-use facet AXES in tenant-byte order (value[1..=stride]). The SoA tenant
// tail ships one code per axis per node; the facet lens / property filter group
// nodes by them LIVE (the dynamic layer — the twin of the materialized facet edges).
// value byte = 1 + axis index. Order MUST match FACET_AXES / REL_FACET_* in
// osint_gotham.rs. The first 6 are the original dual-use pairs; 7..11 the enrichment.
const FACET_AXES_UI = [
  'militaryUse', 'civicUse', 'airo:type', 'MLType', 'purpose', 'capacity',
  'currentStatus', 'type', 'output', 'impact', 'stakeholder',
];

// ── Semantic typing of the 12-item mask (tenant index = value byte − 1) ───────
// Reasoning measures DISTANCE along two ORTHOGONAL axes, never materialised:
//   DEMAND    = offer ⟷ need     (does supply meet demand)
//   CAUSALITY = intent ⟷ impact  (how far the outcome drifted from the goal → divergence)
const AX = {
  militaryUse: 0, civicUse: 1, airoRole: 2, mlType: 3, purpose: 4,
  capacity: 5, currentStatus: 6, type: 7, output: 8, impact: 9, stakeholder: 10,
};
// McClelland motive — and its adjacency to Freud's developmental gradient.
// demand (need) and intent are INHERENT in the motive: the motive is the source
// of both reasoning axes. The POWER motive (nPow) isn't flat — it's a 4-level
// control-directionality scale (Freud's psychosexual stages), and it sits
// ADJACENT to airo:type: the actor role IS the power level.
//   P1 Oral    "consume from others to myself"   → extraction        (the consumed = AISubject)
//   P2 Anal    "control myself"                  → self-control      (internal systems)
//   P3 Phallic "control OTHERS"                  → AIDeployer/AIOperator (fields the tool)
//   P4 Genital "empower OTHERS to control others"→ AIDeveloper/AIProvider/AISupplier (builds it)
// airo:type bits: 0=Subject(1) 1=Deployer(2) 2=Developer(4) 3=Provider(8)
// 4=Operator(16) 5=Supplier(32).
const MOTIVE = ['nPow', 'nAch', 'nAff'];
const POWER_LEVEL = ['—', 'P1·oral·consume', 'P2·anal·self', 'P3·phallic·control-others', 'P4·genital·empower'];
// Power level (0..4) read straight from the airo:type bitset — the adjacency.
// The boomerang (Deployer ∧ Subject) is P3 that has become P1's object.
const powerOfAiro = (bits: number): number => {
  if (bits & (4 | 8 | 32)) return 4; // Developer | Provider | Supplier — empower others
  if (bits & (2 | 16)) return 3; // Deployer | Operator — control others
  if (bits & 1) return 1; // Subject — the consumed
  return 0;
};
// nAch / nAff still come from the intent/use LABELS (keyword heuristic); nPow is
// carried by the power level above.
const MOTIVE_KEYS = [
  /risk|offend|criminal|detect|monitor|surveil|control|identif|backgroundcheck|lie|privacy|freedom|power|escalat|policy|weapon|command|intel|target/i,
  /evaluat|candidate|performance|predict|mapping|recommend|assess|optimi|rank|classif|generat|research|achiev/i,
  /advertis|social|welfare|assist|chat|consumer|market|delivery|smart|game|connect|translat|affili/i,
];

// categorical palette for facet codes (code 0 = absent → dim slate).
const FACET_PALETTE = [
  '#4dd0e1', '#ffb547', '#35d07f', '#9b8cff', '#ff637d', '#c792ea',
  '#7fd1c7', '#f0a868', '#6cf0ff', '#b5e853', '#ff8fab', '#8fb8ff',
];
const facetColor = (code: number) =>
  code === 0 ? '#2a3a4a' : FACET_PALETTE[(code - 1) % FACET_PALETTE.length];

const DIM_NODE = { background: 'rgba(10,14,23,0.55)', border: '#26323f' };
const DIM_EDGE = 'rgba(50,66,84,0.12)';
const ACTIVE = '#6cf0ff';
// the cross-cutting global-category hubs (HEEL=HIP=0xFFFF ceiling pole) — drawn
// as bright diamonds so the dual-use axes read as global, not basin-local.
const CEILING_COLOR = '#ffd166';

export interface Soa {
  nodeCount: number;
  edgeCount: number;
  cls: Uint8Array;
  edges: Array<{ s: number; t: number; r: number }>;
  labels: string[];
  // per-node facet tenant: `tenantStride` codes (value[1..=stride]) × nodeCount, or
  // null if the asset predates the tenant tail. The dynamic attributes the facet
  // lens / property filter group by. Stride is 11 (current) or 6 (legacy).
  tenants: Uint8Array | null;
  tenantStride: number;
  // per-node global-category flag (HEEL=HIP=0xFFFF ceiling pole): 1 = cross-cutting.
  ceiling: Uint8Array;
  // per-node GUID identity field (bytes 14-15 LE) — the stable node id.
  identity: Uint16Array;
  // the four 8:8 [container:identity] HHTL tiers + the family tier (each a u16:
  // high byte = mixin/kind node, low byte = instance-on-it). The FMA cockpit lays
  // out straight from these; OSINT ignores them.
  heel: Uint16Array;
  hip: Uint16Array;
  twig: Uint16Array;
  leaf: Uint16Array;
  family: Uint16Array;
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
  setDims: (show: boolean) => void;
  setFacet: (axis: number | null) => void;
  setPropFilter: (keys: Set<string>) => number; // → count of surviving nodes
  heatNodes: (heat: Map<number, number> | null) => void; // divergence heat overlay
}

// Decode the OSO1 wire: magic(4) | nodeCount u32 | edgeCount u32 |
// nodeCount×[guid:16|class:1] | edgeCount×[src:u16|tgt:u16|rel:u8] |
// nodeCount×[len:u8|utf8 name]  (the label tail is additive / may be absent).
export function decodeSoa(buf: ArrayBuffer): Soa {
  const dv = new DataView(buf);
  const magicOk =
    dv.getUint8(0) === 0x4f && dv.getUint8(1) === 0x53 &&
    dv.getUint8(2) === 0x4f && dv.getUint8(3) === 0x31;
  if (!magicOk) throw new Error('bad SoA magic (expected OSO1)');
  const nodeCount = dv.getUint32(4, true);
  const edgeCount = dv.getUint32(8, true);
  let off = 12;
  const cls = new Uint8Array(nodeCount);
  // ceiling[i] = 1 when the GUID's HEEL and HIP are both the 0xFFFF sentinel —
  // the node is a cross-cutting GLOBAL category (the dual-use axes), not
  // basin-local. Read straight off the 16-byte GUID (HEEL @4, HIP @6).
  const ceiling = new Uint8Array(nodeCount);
  const identity = new Uint16Array(nodeCount);
  // the 8:8 [container:identity] HHTL tiers (high byte = mixin node, low byte =
  // instance). HEEL/HIP also carry the 0xFFFF/0xFFFF ceiling-pole sentinel.
  const heelA = new Uint16Array(nodeCount);
  const hipA = new Uint16Array(nodeCount);
  const twigA = new Uint16Array(nodeCount);
  const leafA = new Uint16Array(nodeCount);
  const familyA = new Uint16Array(nodeCount);
  for (let i = 0; i < nodeCount; i++) {
    const heel = dv.getUint16(off + 4, true);
    const hip = dv.getUint16(off + 6, true);
    heelA[i] = heel;
    hipA[i] = hip;
    twigA[i] = dv.getUint16(off + 8, true);
    leafA[i] = dv.getUint16(off + 10, true);
    familyA[i] = dv.getUint16(off + 12, true);
    if (heel === 0xffff && hip === 0xffff) ceiling[i] = 1;
    identity[i] = dv.getUint16(off + 14, true);
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
  // optional tenant tail (OSO1 additive): node_count × STRIDE facet bytes
  // (value[1..=STRIDE]). The current bake is 11-wide (militaryUse..stakeholder);
  // a legacy asset is 6-wide. Old readers stop after the labels; here we pick the
  // widest stride that fits so both assets decode.
  let tenants: Uint8Array | null = null;
  let tenantStride = 0;
  if (off + nodeCount * 11 <= dv.byteLength) {
    tenantStride = 11;
  } else if (off + nodeCount * 6 <= dv.byteLength) {
    tenantStride = 6;
  }
  if (tenantStride) {
    tenants = new Uint8Array(buf, off, nodeCount * tenantStride);
    off += nodeCount * tenantStride;
  }
  return {
    nodeCount, edgeCount, cls, edges, labels, tenants, tenantStride, ceiling, identity,
    heel: heelA, hip: hipA, twig: twigA, leaf: leafA, family: familyA,
  };
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
        zIndex: 10,
        pointerEvents: 'auto',
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
        <span
          onClick={onClose}
          title="close"
          style={{ cursor: 'pointer', color: '#9fb4c8', padding: '0 4px', fontSize: 13 }}
        >
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
  const facetAxisRef = useRef<number | null>(null); // mirrors facetAxis for the build closures
  const [soa, setSoa] = useState<Soa | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState('loading SoA…');
  const [readout, setReadout] = useState<Readout | null>(null);
  const [search, setSearch] = useState('');
  const [angle, setAngle] = useState<number | null>(null);
  const showDims = true; // dimension-layer scaffold is inert now (schema nodes de-rendered)
  // active facet lens (0..N = a FACET_AXES_UI axis, or null = colour by class).
  const [facetAxis, setFacetAxis] = useState<number | null>(null);
  // property FILTER: selected "axis:code" keys — a node survives if, for every axis
  // that has ≥1 selected code, its code on that axis is in the set (AND across axes,
  // OR within an axis). The explicit prefix; the graph filters to matches live.
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const selectedRef = useRef<Set<string>>(selected); // mirror for the build closures
  const [openAxis, setOpenAxis] = useState<number | null>(null); // expanded palette axis
  const [divOn, setDivOn] = useState(false); // dual-use divergence lens active

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

  // The semantic VIEW: entity↔entity relations only — the real neo4j relations
  // (2..7, 9) PLUS the basin tissue (member-of 0 / interfaces 1, now docked to real
  // ANCHOR entities, not synthetic hubs). Schema property-nodes (cls 5/6) and their
  // facet spokes (VALID_FOR 8, facets 10..20) are NOT rendered: a dimension is a
  // prefix carried ON the node (read live by the facet lens), never a node to spoke
  // into. That is what dissolves the islands — no "look up a property as a node".
  const view = useMemo(() => {
    if (!soa) return null;
    const semantic = soa.edges
      .filter(
        (e) =>
          e.s < soa.nodeCount &&
          e.t < soa.nodeCount &&
          soa.cls[e.s] < 5 &&
          soa.cls[e.t] < 5 &&
          e.r !== 8 &&
          e.r < 10,
      )
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
    // facet-lens colouring: when an axis is active, a node's border is its tenant
    // code on that axis (categorical, computed live across every node); else the
    // class colour. This is the dynamic group-by — no baked edges involved.
    const nodeBorder = (i: number) => {
      if (soa.ceiling[i]) return CEILING_COLOR; // global hubs stay prominent in every mode
      const ax = facetAxisRef.current;
      if (ax != null && soa.tenants) return facetColor(soa.tenants[i * soa.tenantStride + ax]);
      return classColor(soa.cls[i]);
    };
    const nodeKind = (i: number) =>
      soa.ceiling[i] ? '◈ global category (cross-cutting)' : CLASS[soa.cls[i]]?.name ?? 'concept';
    const baseNode = (i: number) => ({
      id: i,
      label: soa.labels[i] || `#${i}`,
      shape: soa.ceiling[i] ? 'diamond' : 'dot',
      color: {
        background: soa.ceiling[i] ? 'rgba(255,209,102,0.14)' : 'rgba(10,14,23,0.88)',
        border: nodeBorder(i),
        highlight: { background: 'rgba(10,14,23,0.96)', border: '#9fe8ff' },
        hover: { background: 'rgba(10,14,23,0.82)', border: nodeBorder(i) },
      },
      size: baseSize(i) + (soa.ceiling[i] ? 7 : 0),
      font: { color: soa.ceiling[i] ? '#ffe9b0' : '#d9e9f9' },
      title: `${soa.labels[i] || `#${i}`}\n${nodeKind(i)} · ${degree.get(i) ?? 0} links`,
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
        color: { background: 'rgba(10,14,23,0.95)', border: nodeBorder(id) },
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

    // the dimension layer = SchemaValue/SchemaAxis nodes (cls 5/6) + their
    // VALID_FOR and facet edges. Toggle hides it via vis `hidden` (no relayout),
    // so the "family concepts" can be dropped to a clean entity graph and back.
    const schemaNodeIds = Array.from(touched).filter((i) => soa.cls[i] === 5 || soa.cls[i] === 6);
    const schemaEdgeIds = semantic.filter((e) => isFacetRel(e.r)).map((e) => e.id);
    const setDims = (show: boolean) => {
      visNodes.update(schemaNodeIds.map((id) => ({ id, hidden: !show })));
      visEdges.update(schemaEdgeIds.map((id) => ({ id, hidden: !show })));
    };
    // facet lens: recolour every rendered node by its tenant code on `axis`
    // (the dynamic group-by across all nodes); null restores the class colours.
    // Read-only over the tenant column — no edges, no relayout.
    const setFacet = (axis: number | null) => {
      facetAxisRef.current = axis != null && soa.tenants ? axis : null;
      visNodes.update(Array.from(touched).map(baseNode));
      if (selectedRef.current.size) applyPropFilter(selectedRef.current);
    };
    // property filter: a node survives if, for every axis carrying ≥1 selected
    // code, its tenant code on that axis is selected (AND across axes, OR within an
    // axis). The explicit prefix — matches stay lit, the rest dim, edges survive
    // only between two matches. Returns the surviving count.
    const matchesFilter = (i: number, byAxis: Map<number, Set<number>>): boolean => {
      if (!soa.tenants) return true;
      for (const [ax, codes] of byAxis) {
        if (!codes.has(soa.tenants[i * soa.tenantStride + ax])) return false;
      }
      return true;
    };
    const applyPropFilter = (keys: Set<string>): number => {
      selectedRef.current = keys;
      const byAxis = new Map<number, Set<number>>();
      keys.forEach((k) => {
        const [ax, code] = k.split(':').map(Number);
        const s = byAxis.get(ax) ?? new Set<number>();
        s.add(code);
        byAxis.set(ax, s);
      });
      const ids = Array.from(touched);
      if (!byAxis.size) {
        visNodes.update(ids.map(baseNode));
        visEdges.update(semantic.map(baseEdge));
        return ids.length;
      }
      const match = new Set<number>();
      ids.forEach((i) => {
        if (matchesFilter(i, byAxis)) match.add(i);
      });
      visNodes.update(
        ids.map((i) =>
          match.has(i) ? baseNode(i) : { id: i, color: DIM_NODE, font: { color: '#4a5766' } },
        ),
      );
      visEdges.update(
        semantic.map((e) =>
          match.has(e.s) && match.has(e.t)
            ? baseEdge(e)
            : { id: e.id, color: { color: DIM_EDGE }, width: 0.5, font: { color: 'rgba(0,0,0,0)' } },
        ),
      );
      return match.size;
    };
    // heat overlay: colour each touched node by a [0,1] score (null = restore to
    // base). Used by the dual-use divergence lens — the causality distance of the
    // capability the node offers, painted cool→hot.
    const heatNodes = (heat: Map<number, number> | null) => {
      const ids = Array.from(touched);
      if (!heat) {
        visNodes.update(ids.map(baseNode));
        return;
      }
      visNodes.update(
        ids.map((i) => {
          const t = heat.get(i);
          if (t == null) return { id: i, color: DIM_NODE, font: { color: '#4a5766' } };
          const col = `rgb(${Math.round(70 + 185 * t)},${Math.round(205 - 155 * t)},${Math.round(255 - 210 * t)})`;
          return {
            id: i,
            color: { background: 'rgba(10,14,23,0.92)', border: col },
            font: { color: '#eaf4ff' },
          };
        }),
      );
    };
    // apply the current toggle/lens/filter state on (re)build — covers a control
    // that landed before the network (and apiRef) existed, so nothing desyncs.
    setDims(showDims);
    setFacet(facetAxis);
    if (selectedRef.current.size) applyPropFilter(selectedRef.current);

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
      setDims,
      setFacet,
      setPropFilter: applyPropFilter,
      heatNodes,
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
    setDivOn(false);
    apiRef.current?.heatNodes(null);
    apiRef.current?.clear();
  };
  // dual-use divergence lens: paint every node by the causality distance of the
  // capability it offers, and stream the ranked capabilities (demand fork ·
  // Δimpact · power level · motive) into the readout. Toggle off to restore.
  const fireDivergence = () => {
    if (divOn) {
      setDivOn(false);
      apiRef.current?.heatNodes(null);
      setReadout(null);
      return;
    }
    if (!duModel) return;
    setDivOn(true);
    setAngle(null);
    apiRef.current?.heatNodes(duModel.nodeDiv);
    // Person × Situation (Lewin/Atkinson/Rheinberg). The SITUATION is the causal
    // chain of the 4 outside factors — capability → (mil/civ demand) → declared
    // purpose (explicit intent) ⟹ revealed impact (implicit) — the chain AIwar
    // builds to prove the harm the companies deny. The PERSON is the trait
    // (POWER_LEVEL from airo:type, else the McClelland motive) reasoned against it:
    // the divergence is trait-driven, not incidental to a "neutral" dual-use.
    const lines = duModel.rows.slice(0, 40).map((r) => {
      const trait = r.pow ? POWER_LEVEL[r.pow] : r.motive >= 0 ? MOTIVE[r.motive] : '—';
      return {
        text: `${r.label} → [mil ${r.mil}/civ ${r.civ}] → ${r.expl || '—'} ⟹ ${r.impl || '—'}  │  ${trait}`,
        conf: r.divergence,
        survived: r.divergence >= 0.5,
      };
    });
    // Person→Situation attribution: how much of the high-divergence (the situational
    // intent→impact drift) is carried by a power trait (P3/P4 or nPow). High % = the
    // harm is trait-driven — the causal chain the "can't prove it's harmful" defense denies.
    const hi = duModel.rows.filter((r) => r.divergence >= 0.5);
    const macht = hi.length ? hi.filter((r) => r.motive === 0 || r.pow >= 3).length / hi.length : 0;
    setReadout({
      seed: `Person×Situation · chain ⟹ impact · Macht-driven ${Math.round(macht * 100)}%`,
      kind: 'spread',
      lines,
      theories: hi.length,
    });
  };
  const toggleFacet = (axis: number) => {
    const next = facetAxis === axis ? null : axis;
    setFacetAxis(next);
    apiRef.current?.setFacet(next);
  };
  // keep the build-closure mirror in sync with the selection state.
  useEffect(() => {
    selectedRef.current = selected;
  }, [selected]);
  // toggle one "axis:code" property in/out of the filter, then re-apply live.
  const toggleProp = (axis: number, code: number) => {
    const key = `${axis}:${code}`;
    const next = new Set(selected);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    setSelected(next);
    apiRef.current?.setPropFilter(next);
  };
  const clearProps = () => {
    const empty = new Set<string>();
    setSelected(empty);
    apiRef.current?.setPropFilter(empty);
  };

  // property CATALOG: for every facet axis, the value-set carried ON the nodes —
  // code → {label (named via the facet edges), count across rendered nodes}. This
  // powers the expandable lower-right palette; selecting values filters the graph
  // by that explicit prefix. airo:type is a bitset, so its codes read as role-masks.
  const catalog = useMemo(() => {
    if (!soa || !soa.tenants || !soa.tenantStride || !view) return null;
    const tenants = soa.tenants;
    const stride = soa.tenantStride;
    return Array.from({ length: Math.min(stride, FACET_AXES_UI.length) }, (_, axis) => {
      const rel = 10 + axis;
      const name = new Map<number, string>();
      for (const e of soa.edges) {
        if (e.r === rel && e.s < soa.nodeCount && e.t < soa.nodeCount) {
          const code = tenants[e.s * stride + axis];
          if (code !== 0 && !name.has(code)) name.set(code, soa.labels[e.t] || `code ${code}`);
        }
      }
      const count = new Map<number, number>();
      view.touched.forEach((i) => {
        const code = tenants[i * stride + axis];
        if (code !== 0) count.set(code, (count.get(code) ?? 0) + 1);
      });
      const values = Array.from(count.entries())
        .sort((a, b) => b[1] - a[1])
        .map(([code, n]) => ({ code, n, label: name.get(code) ?? `code ${code}` }));
      return { axis, name: FACET_AXES_UI[axis], values };
    }).filter((a) => a.values.length);
  }, [soa, view]);

  // live count of nodes surviving the current filter (AND across axes, OR within).
  const matchCount = useMemo(() => {
    if (!soa || !soa.tenants || !selected.size || !view) return null;
    const stride = soa.tenantStride;
    const byAxis = new Map<number, Set<number>>();
    selected.forEach((k) => {
      const [ax, code] = k.split(':').map(Number);
      const s = byAxis.get(ax) ?? new Set<number>();
      s.add(code);
      byAxis.set(ax, s);
    });
    let n = 0;
    view.touched.forEach((i) => {
      let ok = true;
      for (const [ax, codes] of byAxis) {
        if (!codes.has(soa.tenants![i * stride + ax])) {
          ok = false;
          break;
        }
      }
      if (ok) n += 1;
    });
    return n;
  }, [soa, selected, view]);

  // dual-use reasoning model — the two orthogonal distances + the McClelland/Freud
  // power flow, all measured over the tenant (nothing materialised).
  //   CAUSALITY distance = intent→impact drift, per capability = the divergence,
  //     computed as the Jaccard distance of the impact sets between the militaryUse
  //     branch and the civicUse branch of the SAME offer (the shared pivot).
  //   DEMAND fork = mil vs civ count for that offer.
  //   POWER level = airo:type bits (Freud gradient), the adjacency to nPow.
  const duModel = useMemo(() => {
    if (!soa || !soa.tenants || !soa.tenantStride || !view) return null;
    const T = soa.tenants;
    const stride = soa.tenantStride;
    // naming per axis (code → label) from the facet edges still in the wire.
    const nmeth = new Map<number, Map<number, string>>();
    for (let ax = 0; ax < Math.min(stride, FACET_AXES_UI.length); ax++) {
      const m = new Map<number, string>();
      const rel = 10 + ax;
      for (const e of soa.edges) {
        if (e.r === rel && e.s < soa.nodeCount && e.t < soa.nodeCount) {
          const code = T[e.s * stride + ax];
          if (code && !m.has(code)) m.set(code, soa.labels[e.t] || `code ${code}`);
        }
      }
      nmeth.set(ax, m);
    }
    const nameOf = (ax: number, code: number) => nmeth.get(ax)?.get(code) ?? '';
    const powerOf = (i: number) => powerOfAiro(T[i * stride + AX.airoRole]);
    const motiveOf = (i: number): number => {
      if (powerOf(i) > 0) return 0; // nPow is carried by the power level
      const txt = [
        nameOf(AX.purpose, T[i * stride + AX.purpose]),
        nameOf(AX.militaryUse, T[i * stride + AX.militaryUse]),
        nameOf(AX.civicUse, T[i * stride + AX.civicUse]),
        nameOf(AX.impact, T[i * stride + AX.impact]),
      ].join(' ');
      let best = -1;
      let hi = 0;
      MOTIVE_KEYS.forEach((re, mi) => {
        const c = (txt.match(re) || []).length;
        if (c > hi) {
          hi = c;
          best = mi;
        }
      });
      return best;
    };
    const byCap = new Map<
      number,
      {
        mil: Set<number>;
        civ: Set<number>;
        milImp: Set<number>;
        civImp: Set<number>;
        pow: number[];
        mot: Map<number, number>;
        pur: Map<number, number>;
        imp: Map<number, number>;
      }
    >();
    view.touched.forEach((i) => {
      const c = T[i * stride + AX.capacity];
      if (!c) return;
      const mil = T[i * stride + AX.militaryUse] !== 0;
      const civ = T[i * stride + AX.civicUse] !== 0;
      if (!mil && !civ) return;
      let r = byCap.get(c);
      if (!r) {
        r = { mil: new Set(), civ: new Set(), milImp: new Set(), civImp: new Set(), pow: [], mot: new Map(), pur: new Map(), imp: new Map() };
        byCap.set(c, r);
      }
      const imp = T[i * stride + AX.impact];
      if (mil) {
        r.mil.add(i);
        if (imp) r.milImp.add(imp);
      }
      if (civ) {
        r.civ.add(i);
        if (imp) r.civImp.add(imp);
      }
      if (imp) r.imp.set(imp, (r.imp.get(imp) ?? 0) + 1); // implicit: the revealed impact
      const pur = T[i * stride + AX.purpose]; // explicit: the declared purpose
      if (pur) r.pur.set(pur, (r.pur.get(pur) ?? 0) + 1);
      const p = powerOf(i);
      if (p) r.pow.push(p);
      const mo = motiveOf(i);
      if (mo >= 0) r.mot.set(mo, (r.mot.get(mo) ?? 0) + 1);
    });
    let rows = Array.from(byCap.entries())
      .map(([code, r]) => {
        const mil = r.mil.size;
        const civ = r.civ.size;
        const forked = mil > 0 && civ > 0;
        const balance = forked ? Math.min(mil, civ) / Math.max(mil, civ) : 0;
        const uni = new Set<number>([...r.milImp, ...r.civImp]);
        const inter = [...r.milImp].filter((x) => r.civImp.has(x)).length;
        const jac = uni.size ? 1 - inter / uni.size : 0; // causality distance
        const divergence = forked ? balance * (0.4 + 0.6 * jac) : 0;
        const pow = r.pow.length ? Math.round(r.pow.reduce((a, b) => a + b, 0) / r.pow.length) : 0;
        let dom = -1;
        let dv = 0;
        r.mot.forEach((n, m) => {
          if (n > dv) {
            dv = n;
            dom = m;
          }
        });
        // the causal-chain links: declared purpose (explicit) and revealed impact
        // (implicit) — the two ends AIwar chains to prove the harm.
        let ep = -1;
        let epv = 0;
        r.pur.forEach((n, cc) => {
          if (n > epv) {
            epv = n;
            ep = cc;
          }
        });
        let im = -1;
        let imv = 0;
        r.imp.forEach((n, cc) => {
          if (n > imv) {
            imv = n;
            im = cc;
          }
        });
        return {
          code,
          label: nameOf(AX.capacity, code) || `cap ${code}`,
          mil,
          civ,
          jac,
          divergence,
          pow,
          motive: dom,
          expl: ep >= 0 ? nameOf(AX.purpose, ep) : '',
          impl: im >= 0 ? nameOf(AX.impact, im) : '',
        };
      })
      .filter((r) => r.divergence > 0);
    const max = rows.reduce((m, r) => Math.max(m, r.divergence), 0) || 1;
    rows = rows.map((r) => ({ ...r, divergence: r.divergence / max })).sort((a, b) => b.divergence - a.divergence);
    const capDiv = new Map<number, number>();
    rows.forEach((r) => capDiv.set(r.code, r.divergence));
    const nodeDiv = new Map<number, number>();
    view.touched.forEach((i) => {
      const d = capDiv.get(T[i * stride + AX.capacity]);
      if (d != null) nodeDiv.set(i, d);
    });
    return { rows, nodeDiv };
  }, [soa, view]);

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
      {/* zIndex:0 traps the vis-network canvas in its OWN stacking context, so
          the overlays (zIndex:10) reliably win pointer events — otherwise the
          canvas swallowed clicks on the search box (no focus) and the ✕. */}
      <div ref={hostRef} style={{ position: 'absolute', inset: 0, zIndex: 0 }} />

      {/* title + status */}
      <div
        style={{
          position: 'absolute',
          top: 16,
          left: 16,
          zIndex: 10,
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
          zIndex: 10,
          pointerEvents: 'auto',
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
          <button
            onClick={fireDivergence}
            title="dual-use causal chain — Person × Situation. SITUATION: capability → mil/civ demand → explicit purpose ⟹ implicit impact (the intent→impact drift AIwar chains to prove harm). PERSON: the McClelland/Freud power trait (P1 consume · P3 control-others · P4 empower) reasoned against it."
            style={{
              fontFamily: 'monospace',
              fontSize: 11,
              color: divOn ? '#0a0e17' : '#ffb0a0',
              background: divOn ? '#ff8f6a' : 'rgba(17,32,48,0.6)',
              border: '1px solid #ff8f6a',
              borderRadius: 6,
              padding: '5px 9px',
              cursor: 'pointer',
              fontWeight: divOn ? 700 : 400,
            }}
          >
            ◆ dual-use
          </button>
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
        {/* facet lens — colour every node by a tenant axis, the dynamic group-by */}
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 5, alignItems: 'center' }}>
          <span style={{ fontSize: 10, color: '#6f87a0', marginRight: 2 }}>◐ facet:</span>
          {FACET_AXES_UI.map((ax, i) => (
            <button
              key={ax}
              onClick={() => toggleFacet(i)}
              title={`colour every node by its ${ax} code — a live group-by across all nodes (the tenant column)`}
              style={{
                fontFamily: 'monospace',
                fontSize: 10,
                color: facetAxis === i ? '#0a0e17' : '#9fb4c8',
                background: facetAxis === i ? facetColor(i + 1) : 'rgba(17,32,48,0.6)',
                border: `1px solid ${facetColor(i + 1)}`,
                borderRadius: 6,
                padding: '4px 7px',
                cursor: 'pointer',
                fontWeight: facetAxis === i ? 700 : 400,
              }}
            >
              {ax}
            </button>
          ))}
        </div>
        <div style={{ fontSize: 10, color: '#6f87a0' }}>
          one entity reasons from it; “A + B” traces the path. click any node to reason.
        </div>
      </div>

      {readout && <ReasonBox readout={readout} onClose={clearReason} />}

      {/* property palette — expandable per-axis value catalogue. Select N values
          to filter the graph by that explicit prefix (AND across axes, OR within an
          axis). e.g. militaryUse=* + stakeholder=* + a purpose value = the
          quid-pro-quo query, live. */}
      {catalog && catalog.length > 0 && (
        <div
          style={{
            position: 'absolute',
            bottom: 16,
            right: 16,
            zIndex: 10,
            fontFamily: 'monospace',
            fontSize: 11,
            color: '#cfe7ff',
            background: 'rgba(8,12,20,0.9)',
            border: '1px solid #2a4a6a',
            borderRadius: 8,
            padding: '8px 10px',
            width: 250,
            maxHeight: '58%',
            overflowY: 'auto',
            pointerEvents: 'auto',
          }}
        >
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              marginBottom: 6,
            }}
          >
            <span style={{ color: '#7fd1ff' }}>
              ◧ properties{matchCount != null ? ` · ${matchCount} match` : ''}
            </span>
            {selected.size > 0 && (
              <span
                onClick={clearProps}
                title="clear filter"
                style={{ cursor: 'pointer', color: '#9fb4c8' }}
              >
                clear ✕
              </span>
            )}
          </div>
          {catalog.map((ax) => {
            const sel = ax.values.filter((v) => selected.has(`${ax.axis}:${v.code}`)).length;
            const open = openAxis === ax.axis;
            return (
              <div key={ax.axis} style={{ marginBottom: 3 }}>
                <div
                  onClick={() => setOpenAxis(open ? null : ax.axis)}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 6,
                    cursor: 'pointer',
                    padding: '2px 0',
                    color: sel ? '#6cf0ff' : '#b7c8db',
                  }}
                >
                  <span style={{ width: 10, color: '#6f87a0' }}>{open ? '▾' : '▸'}</span>
                  <span style={{ overflow: 'hidden', textOverflow: 'ellipsis' }}>{ax.name}</span>
                  <span style={{ marginLeft: 'auto', color: '#7f97b0' }}>
                    {sel ? `${sel}/` : ''}
                    {ax.values.length}
                  </span>
                </div>
                {open && (
                  <div style={{ paddingLeft: 14 }}>
                    {ax.values.map((v) => {
                      const on = selected.has(`${ax.axis}:${v.code}`);
                      return (
                        <div
                          key={v.code}
                          onClick={() => toggleProp(ax.axis, v.code)}
                          style={{
                            display: 'flex',
                            alignItems: 'center',
                            gap: 6,
                            cursor: 'pointer',
                            whiteSpace: 'nowrap',
                            padding: '1px 0',
                            color: on ? '#eaf4ff' : '#8ba0b6',
                          }}
                        >
                          <span
                            style={{
                              display: 'inline-block',
                              width: 9,
                              height: 9,
                              borderRadius: 9,
                              border: `2px solid ${facetColor(v.code)}`,
                              background: on ? facetColor(v.code) : 'transparent',
                              flex: '0 0 auto',
                            }}
                          />
                          <span style={{ overflow: 'hidden', textOverflow: 'ellipsis' }}>
                            {v.label}
                          </span>
                          <span style={{ color: '#7f97b0', marginLeft: 'auto' }}>{v.n}</span>
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* class legend */}
      <div
        style={{
          position: 'absolute',
          bottom: 16,
          left: 16,
          zIndex: 10,
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
        <span style={{ marginRight: 12, whiteSpace: 'nowrap' }}>
          <span
            style={{
              display: 'inline-block',
              width: 9,
              height: 9,
              border: `2px solid ${CEILING_COLOR}`,
              marginRight: 5,
              verticalAlign: 'middle',
              transform: 'rotate(45deg)',
            }}
          />
          ◈ global axis
        </span>
      </div>

      {/* alternative-view link */}
      <a
        href="/osint3d"
        style={{
          position: 'absolute',
          top: 14,
          right: 16,
          zIndex: 10,
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
