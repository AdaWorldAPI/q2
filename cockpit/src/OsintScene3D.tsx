// 3D OSINT scene — the CAM address space rendered from raw SoA bytes.
//
// Fetches the binary SoA buffer `/osint.soa` (node GUIDs + class byte + edge
// index pairs) and decodes each 16-byte GUID → xyz IN THE BROWSER — the
// `osint_gotham::position()` logic ported to JS. No JSON anywhere: the bits feed
// the render. The address IS the coordinate; there is no force-layout pass.
import { useEffect, useRef, useState } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

const GOLDEN_ANGLE = 2.3999632;
const NODE_REC = 17; // guid(16) + class(1)
const EDGE_REC = 5; // src(2) + tgt(2) + rel(1)
const HUB_CLASS = 0xff;

// class order → colour (matches OSINT_SCHEMA order in osint_gotham.rs).
const ORDER_COLOR = [
  0x4dd0e1, // 0 System
  0xffb547, // 1 Stakeholder
  0x35d07f, // 2 Person
  0x9b8cff, // 3 CivicSystem
  0xff637d, // 4 HistoricalSystem
  0x5a6b7f, // 5 SchemaValue
  0xc792ea, // 6 SchemaAxis
];
const LEGEND: Array<[string, number]> = [
  ['System', 0x4dd0e1],
  ['Stakeholder', 0xffb547],
  ['Person', 0x35d07f],
  ['CivicSystem', 0x9b8cff],
  ['HistoricalSystem', 0xff637d],
  ['SchemaValue', 0x5a6b7f],
  ['SchemaAxis', 0xc792ea],
  ['family hub', 0xffffff],
];

function colorOf(cls: number): number {
  return cls === HUB_CLASS ? 0xffffff : ORDER_COLOR[cls] ?? 0x8899aa;
}

// ── Relation types: the `rel` byte the bake already emits (rel_code in
// osint_gotham.rs). 0 member-of / 1 interfaces are the basin SCAFFOLD (the
// adapter's own spokes to the dissolved hubs); 2..9 are the real neo4j
// relations — the VIEW. The client had been discarding this byte and drawing
// every edge one colour, so the scaffold drowned the semantic graph.
const REL_NAME = [
  'member-of', 'interfaces', 'CONNECTED_TO', 'DEVELOPED_BY', 'DEPLOYED_BY',
  'PERSON_LINK', 'USED_IN', 'HIERARCHICAL', 'VALID_FOR', 'related',
];
const REL_COLOR = [
  0x223040, 0x223040, // 0,1 scaffold (unused once the family node dissolves)
  0x4dd0e1, // 2 CONNECTED_TO
  0xffb547, // 3 DEVELOPED_BY
  0x35d07f, // 4 DEPLOYED_BY
  0xff637d, // 5 PERSON_LINK
  0x9b8cff, // 6 USED_IN
  0xc792ea, // 7 HIERARCHICAL
  0x7fd1c7, // 8 VALID_FOR
  0x8fa6c4, // 9 related/other
];

// The family-basin MIXIN — the inverse of Louvain. Louvain reads edges and
// infers communities; the mixin reads the community LABEL (basin, on the GUID)
// and emits the intra-community edges it implies. Clique for small families
// (the true "connective category": membership ⇒ every pair related); ring for
// large families (bounded O(m) proxy of the same idea).
const MIXIN_COLOR = 0x3f6090;
const MIXIN_CLIQUE_MAX = 8;

// Legend for the relation layers (the mixin + the typed neo4j relations).
const REL_LEGEND: Array<[string, number]> = [
  ['family (mixin)', MIXIN_COLOR],
  ...REL_NAME.slice(2).map((nm, i) => [nm, REL_COLOR[i + 2]] as [string, number]),
];

// Port of osint_gotham::basin_center — basins on a golden-angle spiral.
function basinCenter(basin: number): [number, number, number] {
  const r = 40 * Math.sqrt(basin + 1);
  const a = basin * GOLDEN_ANGLE;
  return [r * Math.cos(a), 0, r * Math.sin(a)];
}

// Port of osint_gotham::position — decode a 16-byte GUID → xyz.
// GUID (little-endian): classid[0..4] heel[4..6] hip[6..8] twig[8..10]
//                       leaf[10..12] family[12..14] identity[14..16].
function decodePosition(
  dv: DataView,
  off: number,
  isHub: boolean,
): [number, number, number] {
  const heel = dv.getUint16(off + 4, true);
  const family = dv.getUint16(off + 12, true);
  const identity = dv.getUint16(off + 14, true);
  const basin = family & 0xff;
  const c = basinCenter(basin);
  if (isHub) return c;
  const r = 6 * Math.sqrt(identity + 1);
  const a = identity * GOLDEN_ANGLE;
  const y = heel * 0.5;
  return [c[0] + r * Math.cos(a), c[1] + y, c[2] + r * Math.sin(a)];
}

/** How a family-basin mixin is projected — the ClassView lens, as a render param. */
type MixinMode = 'category' | 'adapter';

interface Scene {
  pos: Float32Array; // nodeCount * 3
  cls: Uint8Array; // nodeCount
  basin: Uint8Array; // nodeCount — family-basin community label (the mixin)
  edges: Uint32Array; // edgeCount * 2 (src, tgt)
  rels: Uint8Array; // edgeCount — relation type per edge (rel_code)
  nodeCount: number;
  edgeCount: number;
}

function parseSoa(buf: ArrayBuffer): Scene {
  const dv = new DataView(buf);
  // magic "OSO1"
  if (dv.getUint8(0) !== 0x4f || dv.getUint8(1) !== 0x53) {
    throw new Error('bad SoA magic');
  }
  const nodeCount = dv.getUint32(4, true);
  const edgeCount = dv.getUint32(8, true);
  const pos = new Float32Array(nodeCount * 3);
  const cls = new Uint8Array(nodeCount);
  const basin = new Uint8Array(nodeCount);
  let off = 12;
  for (let i = 0; i < nodeCount; i++) {
    const o = off + i * NODE_REC;
    const c = dv.getUint8(o + 16);
    cls[i] = c;
    basin[i] = dv.getUint16(o + 12, true) & 0xff; // family low byte = the mixin label
    const [x, y, z] = decodePosition(dv, o, c === HUB_CLASS);
    pos[i * 3] = x;
    pos[i * 3 + 1] = y;
    pos[i * 3 + 2] = z;
  }
  off += nodeCount * NODE_REC;
  const edges = new Uint32Array(edgeCount * 2);
  const rels = new Uint8Array(edgeCount);
  for (let i = 0; i < edgeCount; i++) {
    const o = off + i * EDGE_REC;
    edges[i * 2] = dv.getUint16(o, true);
    edges[i * 2 + 1] = dv.getUint16(o + 2, true);
    rels[i] = dv.getUint8(o + 4); // the relation type the client had been discarding
  }
  return { pos, cls, basin, edges, rels, nodeCount, edgeCount };
}

function mount(container: HTMLDivElement, s: Scene, mode: MixinMode): () => void {
  let w = container.clientWidth || window.innerWidth;
  let h = container.clientHeight || window.innerHeight;

  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0x0a0e17);
  scene.fog = new THREE.FogExp2(0x0a0e17, 0.0014);
  const camera = new THREE.PerspectiveCamera(55, w / h, 0.1, 4000);
  const renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setSize(w, h);
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  container.appendChild(renderer.domElement);

  // normalize the decoded cloud to a centred radius-R sphere.
  const n = Math.max(s.nodeCount, 1);
  let cx = 0;
  let cy = 0;
  let cz = 0;
  for (let i = 0; i < s.nodeCount; i++) {
    cx += s.pos[i * 3];
    cy += s.pos[i * 3 + 1];
    cz += s.pos[i * 3 + 2];
  }
  cx /= n;
  cy /= n;
  cz /= n;
  let maxd = 1e-6;
  for (let i = 0; i < s.nodeCount; i++) {
    maxd = Math.max(
      maxd,
      Math.hypot(s.pos[i * 3] - cx, s.pos[i * 3 + 1] - cy, s.pos[i * 3 + 2] - cz),
    );
  }
  const R = 100;
  const scale = R / maxd;
  const p = new Float32Array(s.nodeCount * 3);
  for (let i = 0; i < s.nodeCount; i++) {
    p[i * 3] = (s.pos[i * 3] - cx) * scale;
    p[i * 3 + 1] = (s.pos[i * 3 + 1] - cy) * scale;
    p[i * 3 + 2] = (s.pos[i * 3 + 2] - cz) * scale;
  }

  // nodes as spheres. In `category` mode the family node is DISSOLVED into the
  // edges it implies (the connective category), so hubs aren't drawn; in
  // `adapter` mode the family node is a modest visible connector.
  const showHubs = mode === 'adapter';
  const sphereGeom = new THREE.SphereGeometry(1, 10, 8);
  const matCache = new Map<number, THREE.MeshBasicMaterial>();
  for (let i = 0; i < s.nodeCount; i++) {
    const c = s.cls[i];
    const isHub = c === HUB_CLASS;
    if (isHub && !showHubs) continue;
    let mat = matCache.get(c);
    if (!mat) {
      mat = new THREE.MeshBasicMaterial({
        color: colorOf(c),
        transparent: true,
        opacity: isHub ? 0.32 : 0.92,
      });
      matCache.set(c, mat);
    }
    const mesh = new THREE.Mesh(sphereGeom, mat);
    mesh.position.set(p[i * 3], p[i * 3 + 1], p[i * 3 + 2]);
    mesh.scale.setScalar(isHub ? 1.5 : 1.3);
    scene.add(mesh);
  }

  const pushSeg = (arr: number[], a: number, b: number) => {
    arr.push(
      p[a * 3], p[a * 3 + 1], p[a * 3 + 2],
      p[b * 3], p[b * 3 + 1], p[b * 3 + 2],
    );
  };

  // ── Layer 1: the family-basin MIXIN, projected per the ClassView mode ──
  // `category`: the connective category — clique among co-family members for
  //   small families (membership ⇒ every pair related), ring for large ones
  //   (bounded proxy). The inverse of Louvain: community label → edges.
  // `adapter`: the family node as a connector — the member→hub spokes the bake
  //   already emits (rel 0 member-of / 1 interfaces).
  const famPos: number[] = [];
  if (mode === 'category') {
    const byBasin = new Map<number, number[]>();
    for (let i = 0; i < s.nodeCount; i++) {
      if (s.cls[i] === HUB_CLASS) continue; // members only
      const arr = byBasin.get(s.basin[i]);
      if (arr) arr.push(i);
      else byBasin.set(s.basin[i], [i]);
    }
    byBasin.forEach((members) => {
      const m = members.length;
      if (m < 2) return;
      if (m <= MIXIN_CLIQUE_MAX) {
        for (let a = 0; a < m; a++) {
          for (let b = a + 1; b < m; b++) pushSeg(famPos, members[a], members[b]);
        }
      } else {
        for (let a = 0; a < m; a++) pushSeg(famPos, members[a], members[(a + 1) % m]);
      }
    });
  } else {
    for (let i = 0; i < s.edgeCount; i++) {
      if (s.rels[i] > 1) continue; // 0 member-of / 1 interfaces = the adapter spokes
      const a = s.edges[i * 2];
      const b = s.edges[i * 2 + 1];
      if (a < s.nodeCount && b < s.nodeCount) pushSeg(famPos, a, b);
    }
  }
  const famGeom = new THREE.BufferGeometry();
  famGeom.setAttribute('position', new THREE.BufferAttribute(new Float32Array(famPos), 3));
  const famMat = new THREE.LineBasicMaterial({
    color: MIXIN_COLOR,
    transparent: true,
    opacity: 0.2,
    blending: THREE.AdditiveBlending,
    depthWrite: false,
  });
  scene.add(new THREE.LineSegments(famGeom, famMat));

  // ── Layer 2: the explicit neo4j relations (rel ≥ 2), colour-typed ──
  // The VIEW on top of the mixin fabric. The bake already carries these; the
  // client now honours the `rel` byte instead of painting every edge alike.
  const semPos: number[] = [];
  const semCol: number[] = [];
  const tmp = new THREE.Color();
  for (let i = 0; i < s.edgeCount; i++) {
    const rel = s.rels[i];
    if (rel < 2) continue; // scaffold handled by the family layer
    const a = s.edges[i * 2];
    const b = s.edges[i * 2 + 1];
    if (a >= s.nodeCount || b >= s.nodeCount) continue;
    pushSeg(semPos, a, b);
    tmp.set(REL_COLOR[rel] ?? 0x8fa6c4);
    semCol.push(tmp.r, tmp.g, tmp.b, tmp.r, tmp.g, tmp.b);
  }
  const semGeom = new THREE.BufferGeometry();
  semGeom.setAttribute('position', new THREE.BufferAttribute(new Float32Array(semPos), 3));
  semGeom.setAttribute('color', new THREE.BufferAttribute(new Float32Array(semCol), 3));
  const semMat = new THREE.LineBasicMaterial({
    vertexColors: true,
    transparent: true,
    opacity: 0.6,
    blending: THREE.AdditiveBlending,
    depthWrite: false,
  });
  scene.add(new THREE.LineSegments(semGeom, semMat));

  // Frame the centred radius-R cloud, then hand control to the user:
  // left-drag = orbit, scroll = zoom, right-drag = pan. A gentle idle
  // auto-rotate runs until the first interaction, then yields fully.
  const fitDist = R / Math.sin((camera.fov * Math.PI) / 360); // fit sphere R in FOV
  camera.position.set(fitDist * 0.55, fitDist * 0.62, fitDist * 0.95);
  camera.lookAt(0, 0, 0);

  const controls = new OrbitControls(camera, renderer.domElement);
  controls.target.set(0, 0, 0);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.rotateSpeed = 0.6;
  controls.zoomSpeed = 0.9;
  controls.panSpeed = 0.7;
  controls.minDistance = 15;
  controls.maxDistance = 1500;
  controls.autoRotate = true;
  controls.autoRotateSpeed = 0.5;
  controls.addEventListener('start', () => {
    controls.autoRotate = false;
  });

  let animId = 0;
  const animate = () => {
    controls.update();
    renderer.render(scene, camera);
    animId = requestAnimationFrame(animate);
  };
  animate();

  const handleResize = () => {
    w = container.clientWidth || window.innerWidth;
    h = container.clientHeight || window.innerHeight;
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
    renderer.setSize(w, h);
  };
  window.addEventListener('resize', handleResize);

  return () => {
    cancelAnimationFrame(animId);
    controls.dispose();
    window.removeEventListener('resize', handleResize);
    sphereGeom.dispose();
    famGeom.dispose();
    famMat.dispose();
    semGeom.dispose();
    semMat.dispose();
    matCache.forEach((m) => m.dispose());
    renderer.dispose();
    if (container.contains(renderer.domElement)) {
      container.removeChild(renderer.domElement);
    }
  };
}

/** Route component: fetch the binary SoA and render it as a 3D scene. */
export function OsintScene3D() {
  const ref = useRef<HTMLDivElement>(null);
  const sceneRef = useRef<Scene | null>(null);
  const [mode, setMode] = useState<MixinMode>('category');
  const [info, setInfo] = useState<{ nodes: number; edges: number } | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Fetch + parse the SoA once.
  useEffect(() => {
    let cancelled = false;
    fetch('/osint.soa')
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.arrayBuffer();
      })
      .then((buf) => {
        if (cancelled) return;
        const s = parseSoa(buf);
        sceneRef.current = s;
        setInfo({ nodes: s.nodeCount, edges: s.edgeCount });
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // (Re)mount whenever the data lands or the mixin projection flips.
  useEffect(() => {
    const container = ref.current;
    const s = sceneRef.current;
    if (!container || !s) return;
    return mount(container, s, mode);
  }, [mode, info]);

  return (
    <div
      style={{
        position: 'relative',
        width: '100%',
        height: '100vh',
        background: '#0a0e17',
        overflow: 'hidden',
      }}
    >
      <div ref={ref} style={{ position: 'absolute', inset: 0 }} />
      <div
        style={{
          position: 'absolute',
          top: 16,
          left: 16,
          fontFamily: 'monospace',
          color: '#93a9bf',
          fontSize: 12,
          textShadow: '0 0 4px #0a0e17',
          pointerEvents: 'none',
        }}
      >
        <div style={{ fontSize: 14, color: '#cfe7ff', marginBottom: 4 }}>
          OSINT · classid 0x0700 · SoA bytes → GUID-decoded 3D
        </div>
        {info && (
          <div>
            {info.nodes} nodes · {info.edges} edges · decoded client-side
          </div>
        )}
        {error && <div style={{ color: '#ff637d' }}>load error: {error}</div>}
        <div style={{ marginTop: 8 }}>
          {LEGEND.map(([name, c]) => (
            <span key={name} style={{ marginRight: 10, whiteSpace: 'nowrap' }}>
              <span
                style={{
                  display: 'inline-block',
                  width: 8,
                  height: 8,
                  borderRadius: 8,
                  background: `#${c.toString(16).padStart(6, '0')}`,
                  marginRight: 4,
                }}
              />
              {name}
            </span>
          ))}
        </div>
        <div style={{ marginTop: 6 }}>
          {REL_LEGEND.map(([name, c]) => (
            <span key={name} style={{ marginRight: 10, whiteSpace: 'nowrap' }}>
              <span
                style={{
                  display: 'inline-block',
                  width: 14,
                  borderTop: `2px solid #${c.toString(16).padStart(6, '0')}`,
                  marginRight: 4,
                  verticalAlign: 'middle',
                }}
              />
              {name}
            </span>
          ))}
        </div>
      </div>
      <button
        onClick={() => setMode((m) => (m === 'category' ? 'adapter' : 'category'))}
        title="Project the family-basin mixin as a connective category (clique among co-family members) or as an adapter (family node + member→hub spokes)"
        style={{
          position: 'absolute',
          top: 16,
          right: 16,
          fontFamily: 'monospace',
          fontSize: 12,
          color: '#cfe7ff',
          background: 'rgba(17,32,48,0.6)',
          border: '1px solid #2a4a6a',
          borderRadius: 6,
          padding: '6px 12px',
          cursor: 'pointer',
        }}
      >
        mixin: {mode === 'category' ? 'connective category' : 'adapter'}
      </button>
    </div>
  );
}
