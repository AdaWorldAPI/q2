// FMA torso · map view — the splat AS the GUID substrate.
//
// Renders the same torso.splat (SPL3: real BodyParts3D geometry, per-vertex normal,
// per-surfel node-row tag) but treats it as the value-tenant SoA it is: click a
// surfel -> its node row -> O(1) into torso.nodes.json (the node SoA) -> the FMA
// structure's identity (concept id, tissue, is_a DistinguishedName, container:identity
// GUID, gaussian range). Click a structure in the list -> its surfels highlight.
// Geometry, graph, and splat are three tenants of one identity, switch-selected.
//
// OPAQUE surfels (matches /torso-live): nearest surface wins, no soft-alpha fog.
//
// Geometry: BodyParts3D, (c) The Database Center for Life Science (CC-BY 4.0 /
// CC-BY-SA 2.1 JP). Attribution shown in-view.
import { useEffect, useMemo, useRef, useState } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

interface NodeRow {
  row: number;
  fma: string;
  name: string;
  depth: number;
  parent: number | null;
  tissue: string;
  is_a: string; // DistinguishedName path (/system/<type ancestors>/<self>)
  container: number;
  identity: number;
  guid: number; // (container << 16) | identity
  rgb: [number, number, number];
  g_start: number;
  g_count: number;
  centroid: [number, number, number] | null;
}
interface NodesDoc { attribution: string; nodes: NodeRow[] }

interface Spl3 {
  count: number;
  positions: Float32Array;
  colors: Float32Array; // 0..1 rgb (flat per-structure tissue colour)
  normals: Float32Array; // unit surface normal (drives lighting)
  opacity: Float32Array; // 0..1 per-surfel (depth-peel tissue tag; gates skin/flesh)
  scale: Float32Array; // per-surfel world disk radius (k-NN sized)
  rows: Float32Array; // node_row per surfel (the O(1) switch key)
}

// SPL3 body (22 B): pos 3f | normal 3i8 | rgb 3u8 | opacity u8 | scale u8 | node_row u16.
// Orientation: (x,y,z) -> (-x, z, y), a proper rotation that stands the body head-up
// in three.js's Y-up world (BodyParts3D model +Z is superior -> +Y up).
function decodeSpl3(buf: ArrayBuffer): Spl3 {
  const dv = new DataView(buf);
  const magic = String.fromCharCode(dv.getUint8(0), dv.getUint8(1), dv.getUint8(2), dv.getUint8(3));
  if (magic !== 'SPL3') throw new Error(`bad magic "${magic}" (expected SPL3)`);
  const count = dv.getUint32(4, true);
  const scaleMax = dv.getFloat32(12, true);
  const off = 40;
  const positions = new Float32Array(count * 3);
  const colors = new Float32Array(count * 3);
  const normals = new Float32Array(count * 3);
  const opacity = new Float32Array(count);
  const scale = new Float32Array(count);
  const rows = new Float32Array(count);
  for (let i = 0; i < count; i++) {
    const b = off + i * 22;
    const x = dv.getFloat32(b, true), y = dv.getFloat32(b + 4, true), z = dv.getFloat32(b + 8, true);
    positions[i * 3] = -x; positions[i * 3 + 1] = z; positions[i * 3 + 2] = y;
    const nx = dv.getInt8(b + 12) / 127, ny = dv.getInt8(b + 13) / 127, nz = dv.getInt8(b + 14) / 127;
    normals[i * 3] = -nx; normals[i * 3 + 1] = nz; normals[i * 3 + 2] = ny;
    colors[i * 3] = dv.getUint8(b + 15) / 255;
    colors[i * 3 + 1] = dv.getUint8(b + 16) / 255;
    colors[i * 3 + 2] = dv.getUint8(b + 17) / 255;
    opacity[i] = dv.getUint8(b + 18) / 255;
    scale[i] = (dv.getUint8(b + 19) / 255) * scaleMax;
    rows[i] = dv.getUint16(b + 20, true);
  }
  return { count, positions, colors, normals, opacity, scale, rows };
}

// Lighting computed at the vertex from the baked surface normal — hemisphere ambient +
// key diffuse + soft fill, light fixed in object space. Per-surfel size = its own world
// radius projected to pixels (perspective-correct). OPAQUE: selection dims the rest by
// BRIGHTNESS (not alpha) so the picker stays crisp.
const VERT = `
attribute vec3 aColor;
attribute vec3 aNormal;
attribute float aScale;
attribute float aRow;
uniform float uSelected;
uniform float uFocalPx;
varying vec3 vColor;
void main() {
  vec3 n = normalize(aNormal);
  const vec3 L = vec3(-0.401, 0.783, 0.476);
  float ndl = max(dot(n, L), 0.0);
  float hemi = 0.34 + 0.20 * (n.y * 0.5 + 0.5);
  float fill = 0.12 * (-n.x * 0.5 + 0.5);
  float shade = min(hemi + fill + 0.92 * ndl, 1.3);
  bool sel = uSelected < 0.0 || abs(aRow - uSelected) < 0.5;
  vColor = aColor * shade * (sel ? 1.0 : 0.18);
  vec4 mv = modelViewMatrix * vec4(position, 1.0);
  gl_Position = projectionMatrix * mv;
  gl_PointSize = max(2.0 * aScale * uFocalPx / max(-mv.z, 0.02), 1.5);
}`;
const FRAG = `
precision mediump float;
varying vec3 vColor;
void main() {
  vec2 c = gl_PointCoord - 0.5;
  if (dot(c, c) > 0.25) discard;
  gl_FragColor = vec4(vColor, 1.0);
}`;

function mount(
  container: HTMLDivElement,
  splat: Spl3,
  onPick: (row: number) => void,
  apiRef: { current: { select: (row: number) => void } | null },
  onStats: (s: { fps: number; frac: number }) => void,
): () => void {
  let w = container.clientWidth || window.innerWidth;
  let h = container.clientHeight || window.innerHeight;
  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0x0a0e17);
  const camera = new THREE.PerspectiveCamera(45, w / h, 0.01, 100);
  camera.position.set(0, 0.05, 3.0);
  const renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setSize(w, h);
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  container.appendChild(renderer.domElement);

  // LazyLock: build the geometry ONCE; selection + throttle are uniforms / drawRange,
  // never a rebuild. A shuffled index makes a drawRange prefix a uniform spatial
  // subsample, so adaptive decimation thins evenly (not by structure order).
  const geom = new THREE.BufferGeometry();
  geom.setAttribute('position', new THREE.BufferAttribute(splat.positions, 3));
  geom.setAttribute('aColor', new THREE.BufferAttribute(splat.colors, 3));
  geom.setAttribute('aNormal', new THREE.BufferAttribute(splat.normals, 3));
  geom.setAttribute('aScale', new THREE.BufferAttribute(splat.scale, 1));
  geom.setAttribute('aRow', new THREE.BufferAttribute(splat.rows, 1));
  const idx = new Uint32Array(splat.count);
  for (let i = 0; i < splat.count; i++) idx[i] = i;
  let s = 0x9e3779b9 >>> 0;
  for (let i = splat.count - 1; i > 0; i--) {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    const j = s % (i + 1);
    const t = idx[i]; idx[i] = idx[j]; idx[j] = t;
  }
  geom.setIndex(new THREE.BufferAttribute(idx, 1));
  geom.setDrawRange(0, splat.count);
  const focalPx = () =>
    renderer.domElement.height / (2 * Math.tan(THREE.MathUtils.degToRad(camera.fov) / 2));
  const mat = new THREE.ShaderMaterial({
    vertexShader: VERT,
    fragmentShader: FRAG,
    uniforms: { uSelected: { value: -1 }, uFocalPx: { value: focalPx() } },
    transparent: false, // OPAQUE — crisp picker, no soft-alpha fog
    depthTest: true,
    depthWrite: true,
  });
  const points = new THREE.Points(geom, mat);
  scene.add(points);

  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.target.set(0, 0, 0);
  controls.minDistance = 0.6;
  controls.maxDistance = 12;

  const ray = new THREE.Raycaster();
  ray.params.Points = { threshold: 0.02 };
  const ndc = new THREE.Vector2();
  let downAt = { x: 0, y: 0 };
  renderer.domElement.addEventListener('pointerdown', (e) => { downAt = { x: e.clientX, y: e.clientY }; });
  renderer.domElement.addEventListener('pointerup', (e) => {
    if (Math.hypot(e.clientX - downAt.x, e.clientY - downAt.y) > 5) return; // drag, not click
    const r = renderer.domElement.getBoundingClientRect();
    ndc.x = ((e.clientX - r.left) / r.width) * 2 - 1;
    ndc.y = -((e.clientY - r.top) / r.height) * 2 + 1;
    ray.setFromCamera(ndc, camera);
    const hit = ray.intersectObject(points, false);
    if (hit.length && hit[0].index != null) onPick(splat.rows[hit[0].index]);
  });

  apiRef.current = { select: (row: number) => { mat.uniforms.uSelected.value = row; } };

  // Adaptive-FPS: an EMA of the frame time predicts next-frame cost; over the 60fps
  // budget we thin the draw range (uniform subsample) + drop pixelRatio BEFORE the
  // stutter lands, and recover when frames are cheap. Active fraction is surfaced —
  // never silent decimation.
  let raf = 0;
  let ema = 16.6;
  let last = performance.now();
  let active = splat.count;
  let sinceStat = 0;
  const tick = () => {
    raf = requestAnimationFrame(tick);
    const now = performance.now();
    ema = ema * 0.9 + (now - last) * 0.1;
    last = now;
    if (ema > 22 && active > splat.count * 0.2) {
      active = Math.max(Math.round(splat.count * 0.2), Math.round(active * 0.9));
    } else if (ema < 15 && active < splat.count) {
      active = Math.min(splat.count, Math.round(active * 1.05) + 2000);
    }
    geom.setDrawRange(0, active);
    const pr = ema > 30 ? 1 : Math.min(window.devicePixelRatio, 2);
    if (renderer.getPixelRatio() !== pr) renderer.setPixelRatio(pr);
    mat.uniforms.uFocalPx.value = focalPx();
    controls.update();
    renderer.render(scene, camera);
    if (++sinceStat >= 20) {
      sinceStat = 0;
      onStats({ fps: Math.round(1000 / Math.max(ema, 1)), frac: active / splat.count });
    }
  };
  tick();
  const onResize = () => {
    w = container.clientWidth || window.innerWidth; h = container.clientHeight || window.innerHeight;
    camera.aspect = w / h; camera.updateProjectionMatrix(); renderer.setSize(w, h);
  };
  const ro = new ResizeObserver(onResize); ro.observe(container);
  return () => {
    cancelAnimationFrame(raf); ro.disconnect(); controls.dispose();
    geom.dispose(); mat.dispose(); renderer.dispose();
    if (renderer.domElement.parentNode === container) container.removeChild(renderer.domElement);
  };
}

const hex8 = (n: number) => '0x' + (n >>> 0).toString(16).padStart(8, '0');

export function TorsoMap() {
  const ref = useRef<HTMLDivElement>(null);
  const apiRef = useRef<{ select: (row: number) => void } | null>(null);
  const [splat, setSplat] = useState<Spl3 | null>(null);
  const [doc, setDoc] = useState<NodesDoc | null>(null);
  const [sel, setSel] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [stats, setStats] = useState<{ fps: number; frac: number } | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetch('/torso.splat')
      .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status} torso.splat`); return r.arrayBuffer(); })
      .then((b) => !cancelled && setSplat(decodeSpl3(b)))
      .catch((e) => !cancelled && setError(String(e)));
    fetch('/torso.nodes.json')
      .then((r) => (r.ok ? r.json() : null))
      .then((d) => !cancelled && d && setDoc(d as NodesDoc))
      .catch(() => {});
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    const c = ref.current;
    if (!c || !splat) return;
    return mount(c, splat, (row) => setSel(row), apiRef, setStats);
  }, [splat]);

  useEffect(() => { apiRef.current?.select(sel ?? -1); }, [sel]);

  // the "switch": row -> node, built once (O(1) lookup)
  const byRow = useMemo(() => {
    const m = new Map<number, NodeRow>();
    doc?.nodes.forEach((n) => m.set(n.row, n));
    return m;
  }, [doc]);

  // partonomy breadcrumb (walk parent links) for the selected node
  const path = useMemo(() => {
    if (sel == null) return [];
    const out: NodeRow[] = [];
    let cur: number | null = sel;
    for (let i = 0; i < 24 && cur != null; i++) {
      const n = byRow.get(cur);
      if (!n) break;
      out.push(n);
      cur = n.parent;
    }
    return out.reverse();
  }, [sel, byRow]);

  const owners = useMemo(
    () => (doc?.nodes ?? []).filter((n) => n.g_count > 0).sort((a, b) => b.g_count - a.g_count),
    [doc],
  );

  const selNode = sel != null ? byRow.get(sel) : undefined;
  const css = (rgb: [number, number, number]) => `rgb(${rgb[0]},${rgb[1]},${rgb[2]})`;

  return (
    <div style={{ position: 'fixed', inset: 0, background: '#0a0e17', overflow: 'hidden', color: '#cdd9e5', font: '12px ui-monospace, monospace' }}>
      <div ref={ref} style={{ position: 'absolute', inset: 0 }} />

      <div style={{ position: 'absolute', top: 12, left: 16, pointerEvents: 'none' }}>
        <div style={{ fontSize: 15, color: '#fff' }}>FMA torso · map</div>
        <div style={{ opacity: 0.7 }}>
          click a structure — splat ↔ FMA, one node at one address
          {doc ? ` · ${owners.length} structures` : ''}
        </div>
        {splat && (
          <div style={{ opacity: 0.55, marginTop: 2 }}>
            {splat.count.toLocaleString()} surfels
            {stats ? ` · ${stats.fps} fps · ${Math.round(stats.frac * 100)}% drawn (adaptive)` : ''}
          </div>
        )}
      </div>

      {error && (
        <div style={{ position: 'absolute', top: '46%', width: '100%', textAlign: 'center', color: '#ff8095' }}>{error}</div>
      )}

      {/* selected structure: identity (tissue + container:identity GUID + is_a DN) + partonomy */}
      {selNode && (
        <div style={{ position: 'absolute', top: 12, right: 16, width: 300, background: '#0e1422cc', border: '1px solid #20304a', borderRadius: 6, padding: '10px 12px' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style={{ width: 11, height: 11, borderRadius: 3, background: css(selNode.rgb) }} />
            <span style={{ color: '#fff', fontSize: 14 }}>{selNode.name}</span>
          </div>
          <div style={{ opacity: 0.7, marginTop: 4 }}>
            {selNode.fma} · {selNode.g_count.toLocaleString()} surfels · depth {selNode.depth}
          </div>
          <div style={{ opacity: 0.7, marginTop: 4 }}>
            <span style={{ color: css(selNode.rgb) }}>{selNode.tissue}</span>
            {' · GUID '}{selNode.container}:{String(selNode.identity).padStart(4, '0')}{' '}
            <span style={{ opacity: 0.7 }}>{hex8(selNode.guid)}</span>
          </div>
          <div style={{ opacity: 0.5, marginTop: 4, fontSize: 11, wordBreak: 'break-all' }}>{selNode.is_a}</div>
          <div style={{ marginTop: 8, borderTop: '1px solid #1b2740', paddingTop: 6, opacity: 0.85 }}>
            {path.map((n, i) => (
              <div key={n.row} style={{ paddingLeft: i * 8, color: n.row === sel ? '#fff' : '#8aa0bb', cursor: 'pointer' }} onClick={() => setSel(n.row)}>
                {i > 0 ? '└ ' : ''}{n.name}
              </div>
            ))}
          </div>
        </div>
      )}

      {/* structure list (graph -> splat) */}
      <div style={{ position: 'absolute', bottom: 30, left: 16, maxHeight: '42%', width: 250, overflowY: 'auto', background: '#0e1422aa', border: '1px solid #1b2740', borderRadius: 6, padding: 6 }}>
        {owners.map((n) => (
          <div
            key={n.row}
            onClick={() => setSel(n.row)}
            style={{ display: 'flex', alignItems: 'center', gap: 7, padding: '2px 4px', borderRadius: 3, cursor: 'pointer', background: n.row === sel ? '#1d2b40' : 'transparent', color: n.row === sel ? '#fff' : '#9fb2c9' }}
          >
            <span style={{ width: 9, height: 9, borderRadius: 2, background: css(n.rgb), flex: '0 0 auto' }} />
            <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{n.name}</span>
          </div>
        ))}
      </div>

      <div style={{ position: 'absolute', top: 14, right: 320, font: '12px ui-monospace, monospace' }}>
        <a href="/torso-live" style={{ color: '#7fa6c4', textDecoration: 'none' }}>live orbit →</a>
      </div>
      <div style={{ position: 'absolute', bottom: 8, left: 16, color: '#5a6b7e', fontSize: 10, pointerEvents: 'none' }}>
        {doc?.attribution ?? 'BodyParts3D, (c) The Database Center for Life Science (CC-BY 4.0 / CC-BY-SA 2.1 JP)'}
      </div>
    </div>
  );
}
