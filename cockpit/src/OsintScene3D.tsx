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

interface Scene {
  pos: Float32Array; // nodeCount * 3
  cls: Uint8Array; // nodeCount
  edges: Uint32Array; // edgeCount * 2 (src, tgt)
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
  let off = 12;
  for (let i = 0; i < nodeCount; i++) {
    const o = off + i * NODE_REC;
    const c = dv.getUint8(o + 16);
    cls[i] = c;
    const [x, y, z] = decodePosition(dv, o, c === HUB_CLASS);
    pos[i * 3] = x;
    pos[i * 3 + 1] = y;
    pos[i * 3 + 2] = z;
  }
  off += nodeCount * NODE_REC;
  const edges = new Uint32Array(edgeCount * 2);
  for (let i = 0; i < edgeCount; i++) {
    const o = off + i * EDGE_REC;
    edges[i * 2] = dv.getUint16(o, true);
    edges[i * 2 + 1] = dv.getUint16(o + 2, true);
  }
  return { pos, cls, edges, nodeCount, edgeCount };
}

function mount(container: HTMLDivElement, s: Scene): () => void {
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

  // nodes as spheres (one material per class; basin hubs larger + translucent).
  const sphereGeom = new THREE.SphereGeometry(1, 10, 8);
  const matCache = new Map<number, THREE.MeshBasicMaterial>();
  for (let i = 0; i < s.nodeCount; i++) {
    const c = s.cls[i];
    const isHub = c === HUB_CLASS;
    let mat = matCache.get(c);
    if (!mat) {
      mat = new THREE.MeshBasicMaterial({
        color: colorOf(c),
        transparent: true,
        opacity: isHub ? 0.45 : 0.92,
      });
      matCache.set(c, mat);
    }
    const mesh = new THREE.Mesh(sphereGeom, mat);
    mesh.position.set(p[i * 3], p[i * 3 + 1], p[i * 3 + 2]);
    mesh.scale.setScalar(isHub ? 3.2 : 1.3);
    scene.add(mesh);
  }

  // edges as line segments.
  const linePos = new Float32Array(s.edgeCount * 6);
  for (let i = 0; i < s.edgeCount; i++) {
    const a = s.edges[i * 2];
    const b = s.edges[i * 2 + 1];
    linePos[i * 6] = p[a * 3];
    linePos[i * 6 + 1] = p[a * 3 + 1];
    linePos[i * 6 + 2] = p[a * 3 + 2];
    linePos[i * 6 + 3] = p[b * 3];
    linePos[i * 6 + 4] = p[b * 3 + 1];
    linePos[i * 6 + 5] = p[b * 3 + 2];
  }
  const lineGeom = new THREE.BufferGeometry();
  lineGeom.setAttribute('position', new THREE.BufferAttribute(linePos, 3));
  const lineMat = new THREE.LineBasicMaterial({
    color: 0x5fa8d8,
    transparent: true,
    opacity: 0.34,
    blending: THREE.AdditiveBlending,
    depthWrite: false,
  });
  scene.add(new THREE.LineSegments(lineGeom, lineMat));

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
    lineGeom.dispose();
    lineMat.dispose();
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
  const [info, setInfo] = useState<{ nodes: number; edges: number } | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const container = ref.current;
    if (!container) return;
    let cancelled = false;
    let cleanup = () => {};

    fetch('/osint.soa')
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.arrayBuffer();
      })
      .then((buf) => {
        if (cancelled || !container) return;
        const s = parseSoa(buf);
        setInfo({ nodes: s.nodeCount, edges: s.edgeCount });
        cleanup = mount(container, s);
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(String(e));
      });

    return () => {
      cancelled = true;
      cleanup();
    };
  }, []);

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
      </div>
    </div>
  );
}
