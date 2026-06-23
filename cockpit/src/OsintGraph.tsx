// Sleek OSINT graph — the SAME vis-network renderer the Palantir cockpit uses
// (hollow ring nodes, smooth "wobbly" edges, edge labels, force layout), but
// REROUTED to the SoA: it decodes the baked `/osint.soa` bytes (920 nodes /
// 3344 edges) instead of the 221-node aiwar JSON. The 3D CAM scene lives on at
// /osint3d as the alternative view; this is the default for its cleaner appeal.
import { useEffect, useMemo, useRef, useState } from 'react';
import { Network, type Options } from 'vis-network';
import { DataSet } from 'vis-data';

const HUB_CLASS = 0xff;
const PAGE_BG = '#0a0e17';

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

interface Soa {
  nodeCount: number;
  edgeCount: number;
  cls: Uint8Array;
  edges: Array<{ s: number; t: number; r: number }>;
  labels: string[];
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
    cls[i] = dv.getUint8(off + 16); // class byte trails the 16-byte GUID
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

/** Default view: the SoA decoded into the Palantir vis-network renderer. */
export function OsintGraph() {
  const hostRef = useRef<HTMLDivElement>(null);
  const netRef = useRef<Network | null>(null);
  const [soa, setSoa] = useState<Soa | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState('loading SoA…');

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
    const semantic = soa.edges.filter((e) => e.r >= 2 && e.s < soa.nodeCount && e.t < soa.nodeCount);
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

  // Build the network whenever the data lands.
  useEffect(() => {
    if (!hostRef.current || !soa || !view) return;
    const { degree, touched, semantic } = view;

    const visNodes = new DataSet(
      Array.from(touched).map((i) => {
        const c = soa.cls[i];
        const border = classColor(c);
        const deg = degree.get(i) ?? 1;
        return {
          id: i,
          label: soa.labels[i] || `#${i}`,
          color: {
            background: 'rgba(10,14,23,0.88)', // dark fill → the ring reads hollow
            border,
            highlight: { background: 'rgba(10,14,23,0.96)', border: '#9fe8ff' },
            hover: { background: 'rgba(10,14,23,0.82)', border },
          },
          size: 11 + Math.min(deg, 16) * 1.5,
          title: `${soa.labels[i] || `#${i}`}\n${CLASS[c]?.name ?? 'concept'} · ${deg} links`,
        };
      }),
    );

    const visEdges = new DataSet(
      semantic.map((e, i) => ({
        id: i,
        from: e.s,
        to: e.t,
        label: REL_NAME[e.r] ?? 'related',
        color: { color: `${REL_COLOR[e.r] ?? '#8fa6c4'}55`, highlight: REL_COLOR[e.r] ?? '#4dd0e1' },
      })),
    );

    setStatus(`laying out ${visNodes.length} nodes · ${visEdges.length} relations…`);
    const net = new Network(hostRef.current, { nodes: visNodes, edges: visEdges }, NETWORK_OPTIONS);
    net.once('stabilizationIterationsDone', () => {
      net.setOptions({ physics: { enabled: false } }); // freeze once settled
      setStatus(`${visNodes.length} nodes · ${visEdges.length} relations`);
    });
    netRef.current = net;
    return () => {
      net.destroy();
      netRef.current = null;
    };
  }, [soa, view]);

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
        <div style={{ color: '#6f87a0', marginTop: 2 }}>
          decoded client-side from the baked bytes · the same renderer as the cockpit
        </div>
      </div>

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
