// FMA body · the FULL-RESOLUTION polygon surface on the V3 substrate.
//
// Renders cockpit/public/body.soa — the operator-directed successor to /torso-live:
// ALL points (the 4.2 M-vertex / 6.7 M-triangle BodyParts3D is_a surface, NO cell_mm
// decimation — not the "2000 bubbles" / confetti splat), where every concept is
// addressed on the CLASSID_FMA_V3 (part_of:is_a) cascade rather than the flat
// guid=(container<<16)|identity the torso carried. Filled THREE.Mesh, smooth Phong —
// solid CAD anatomy (ivory bone, red muscle, blue cartilage…), polygons not surfels.
//
// body.soa wire (BSO1, little-endian), baked by crates/osint-bake/src/bin/body.rs:
//   header 18 B: magic "BSO1" | version u16 | node_count u32 | nodes_len u32 | spm1_len u32
//   node table  nodes_len B:  node_count × [ key 16 (V3 NodeGuid) | tissue u8 | depth u8
//                                            | rgb 3u8 | v_start u32 | v_count u32 | label ]
//   geometry    spm1_len  B:  the SPM1 block verbatim (decoded by decodeSpm1 below)
//
// Geometry/data: BodyParts3D, (c) The Database Center for Life Science, CC-BY 4.0.
// Attribution is shown in-view (required by the licence).
import { useEffect, useRef, useState } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

const PAGE_BG = 0x0a0e17;

// 8 compartment layers — the per-vertex byte-19 LAYER id (same scheme as /fma-body),
// baked by bake_body_v3.py's LAYER_OF. Each toggles independently; this is what makes
// /body compartmentalized instead of /torso-live's single depth-peel floor.
const LAYERS: { id: number; name: string; color: string }[] = [
  { id: 1, name: 'skin', color: '#dba88a' },
  { id: 2, name: 'muscle', color: '#bd5c57' },
  { id: 3, name: 'organ', color: '#cc9484' },
  { id: 4, name: 'skeleton', color: '#ebe0c7' },
  { id: 5, name: 'vessel', color: '#cc3838' },
  { id: 6, name: 'nerve', color: '#ebd152' },
  { id: 7, name: 'connective', color: '#e0dbcc' },
  { id: 8, name: 'other', color: '#9696a0' },
];

interface Mesh {
  vertCount: number;
  triCount: number;
  positions: Float32Array;
  normals: Float32Array;
  colors: Uint8Array;
  layer: Float32Array; // byte-19 = compartment LAYER id (1..8)
  index: Uint32Array;
}

interface BodyV3Data {
  mesh: Mesh;
  conceptCount: number; // V3 substrate cardinality (CLASSID_FMA_V3 nodes)
  classid: number;      // 0x1000_0A01 expected
}

interface RenderState {
  enabled: Float32Array; // index 1..8 → 0/1
  alpha: number;
  transparent: boolean;
}

// SPM1 geometry block (same wire as torso.mesh) at byte offset `voff`:
//   header 40 B: magic "SPM1" | vert_count u32 | tri_count u32 | node_count u32
//                | bbox_min 3f | bbox_max 3f
//   vertex body  vert_count x 21 B: pos 3f | normal 3i8 | rgb 3u8 | opacity u8 | node_row u16
//   index body   tri_count x 12 B: 3x u32
// Orientation (x,y,z) -> (-x, z, y): a det-+1 rotation that stands the body head-up in
// three.js Y-up (model +Z superior -> world +Y; +Y anterior -> +Z toward viewer).
function decodeSpm1(dv: DataView, voff: number): Mesh {
  const magic = String.fromCharCode(
    dv.getUint8(voff), dv.getUint8(voff + 1), dv.getUint8(voff + 2), dv.getUint8(voff + 3),
  );
  if (magic !== 'SPM1') throw new Error(`bad SPM1 magic "${magic}" in body.soa geometry block`);
  const vertCount = dv.getUint32(voff + 4, true);
  const triCount = dv.getUint32(voff + 8, true);
  const vbase = voff + 40;
  const positions = new Float32Array(vertCount * 3);
  const normals = new Float32Array(vertCount * 3);
  const colors = new Uint8Array(vertCount * 3);
  const layer = new Float32Array(vertCount);
  for (let i = 0; i < vertCount; i++) {
    const b = vbase + i * 21;
    const x = dv.getFloat32(b, true), y = dv.getFloat32(b + 4, true), z = dv.getFloat32(b + 8, true);
    positions[i * 3] = -x; positions[i * 3 + 1] = z; positions[i * 3 + 2] = y;
    normals[i * 3] = -dv.getInt8(b + 12) / 127;
    normals[i * 3 + 1] = dv.getInt8(b + 14) / 127;
    normals[i * 3 + 2] = dv.getInt8(b + 13) / 127;
    colors[i * 3] = dv.getUint8(b + 15);
    colors[i * 3 + 1] = dv.getUint8(b + 16);
    colors[i * 3 + 2] = dv.getUint8(b + 17);
    layer[i] = dv.getUint8(b + 18); // byte-19 = compartment LAYER id (not opacity)
    // node_row u16 at b+19 → indexes the V3 node table (picker view; not needed to draw)
  }
  const ibase = vbase + vertCount * 21;
  const index = new Uint32Array(triCount * 3);
  for (let t = 0; t < triCount; t++) {
    const b = ibase + t * 12;
    index[t * 3] = dv.getUint32(b, true);
    index[t * 3 + 1] = dv.getUint32(b + 4, true);
    index[t * 3 + 2] = dv.getUint32(b + 8, true);
  }
  return { vertCount, triCount, positions, normals, colors, layer, index };
}

// BSO1 container: read the 18-byte header, skip the V3 node table, decode the SPM1 block.
function decodeBso1(buf: ArrayBuffer): BodyV3Data {
  const dv = new DataView(buf);
  const magic = String.fromCharCode(dv.getUint8(0), dv.getUint8(1), dv.getUint8(2), dv.getUint8(3));
  if (magic !== 'BSO1') throw new Error(`bad magic "${magic}" (expected BSO1)`);
  const conceptCount = dv.getUint32(6, true);
  const nodesLen = dv.getUint32(10, true);
  // classid of the first V3 node (little-endian u32 at the start of the node table)
  const classid = nodesLen > 0 ? dv.getUint32(18, true) : 0;
  const spm1Off = 18 + nodesLen;
  const mesh = decodeSpm1(dv, spm1Off);
  return { mesh, conceptCount, classid };
}

const VERT = `
attribute vec3 aNormal;
attribute vec3 aColor;
attribute float aLayer;
varying vec3 vNormal;
varying vec3 vColor;
varying float vLayer;
void main() {
  vNormal = aNormal;
  vColor = aColor;
  vLayer = aLayer;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}`;
const FRAG = `
precision mediump float;
uniform float uEnabled[9];   // [0] unused, [1..8] = layer on/off
uniform float uAlpha;
varying vec3 vNormal;
varying vec3 vColor;
varying float vLayer;
void main() {
  int li = int(vLayer + 0.5);
  if (li < 1 || li > 8 || uEnabled[li] < 0.5) discard;  // gate by compartment layer
  vec3 n = normalize(vNormal);
  if (!gl_FrontFacing) n = -n;                     // two-sided
  const vec3 L = vec3(-0.401, 0.783, 0.476);
  float ndl = max(dot(n, L), 0.0);
  float hemi = 0.34 + 0.20 * (n.y * 0.5 + 0.5);
  float fill = 0.12 * (-n.x * 0.5 + 0.5);
  float shade = min(hemi + fill + 0.92 * ndl, 1.3);
  gl_FragColor = vec4(vColor * shade, uAlpha);     // uAlpha=1 solid, <1 transparent
}`;

function mount(
  container: HTMLDivElement,
  mesh: Mesh,
  st: RenderState,
  onStats: (s: { fps: number }) => void,
): () => void {
  let w = container.clientWidth || window.innerWidth;
  let h = container.clientHeight || window.innerHeight;

  const scene = new THREE.Scene();
  scene.background = new THREE.Color(PAGE_BG);
  const camera = new THREE.PerspectiveCamera(45, w / h, 0.01, 100);
  camera.position.set(0, 0.05, 3.0);
  const renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setSize(w, h);
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  container.appendChild(renderer.domElement);

  const geom = new THREE.BufferGeometry();
  geom.setAttribute('position', new THREE.BufferAttribute(mesh.positions, 3));
  geom.setAttribute('aNormal', new THREE.BufferAttribute(mesh.normals, 3));
  geom.setAttribute('aColor', new THREE.BufferAttribute(mesh.colors, 3, true)); // u8 normalized
  geom.setAttribute('aLayer', new THREE.BufferAttribute(mesh.layer, 1));
  geom.setIndex(new THREE.BufferAttribute(mesh.index, 1));
  const mat = new THREE.ShaderMaterial({
    vertexShader: VERT,
    fragmentShader: FRAG,
    uniforms: { uEnabled: { value: st.enabled }, uAlpha: { value: st.alpha } },
    side: THREE.DoubleSide,
    transparent: st.transparent,
    depthTest: true,
    depthWrite: !st.transparent,
  });
  const obj = new THREE.Mesh(geom, mat);
  scene.add(obj);

  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.autoRotate = true;
  controls.autoRotateSpeed = 0.6;
  controls.target.set(0, 0, 0);
  controls.minDistance = 0.6;
  controls.maxDistance = 12;

  let raf = 0;
  let ema = 16.6;
  let last = performance.now();
  let sinceStat = 0;
  let wasT = st.transparent;
  const tick = () => {
    raf = requestAnimationFrame(tick);
    const now = performance.now();
    ema = ema * 0.9 + (now - last) * 0.1;
    last = now;
    const pr = ema > 30 ? 1 : Math.min(window.devicePixelRatio, 2);
    if (renderer.getPixelRatio() !== pr) renderer.setPixelRatio(pr);
    mat.uniforms.uEnabled.value = st.enabled;
    mat.uniforms.uAlpha.value = st.alpha;
    if (st.transparent !== wasT) {
      mat.transparent = st.transparent;
      mat.depthWrite = !st.transparent;
      mat.needsUpdate = true;
      wasT = st.transparent;
    }
    controls.update();
    renderer.render(scene, camera);
    if (++sinceStat >= 20) {
      sinceStat = 0;
      onStats({ fps: Math.round(1000 / Math.max(ema, 1)) });
    }
  };
  tick();

  const onResize = () => {
    w = container.clientWidth || window.innerWidth;
    h = container.clientHeight || window.innerHeight;
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
    renderer.setSize(w, h);
  };
  const ro = new ResizeObserver(onResize);
  ro.observe(container);

  return () => {
    cancelAnimationFrame(raf);
    ro.disconnect();
    controls.dispose();
    geom.dispose();
    mat.dispose();
    renderer.dispose();
    if (renderer.domElement.parentNode === container) {
      container.removeChild(renderer.domElement);
    }
  };
}

export function BodyV3() {
  const ref = useRef<HTMLDivElement>(null);
  const [data, setData] = useState<BodyV3Data | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [stats, setStats] = useState<{ fps: number } | null>(null);
  // compartment toggles — skin (1) off by default so the anatomy shows, like /fma-body.
  const [on, setOn] = useState<Record<number, boolean>>({ 1: false, 2: true, 3: true, 4: true, 5: true, 6: true, 7: true, 8: true });
  const [transparent, setTransparent] = useState(false);
  const stRef = useRef<RenderState>({ enabled: new Float32Array([0, 0, 1, 1, 1, 1, 1, 1, 1]), alpha: 1, transparent: false });

  useEffect(() => {
    const e = new Float32Array(9);
    for (let i = 1; i <= 8; i++) e[i] = on[i] ? 1 : 0;
    stRef.current.enabled = e;
    stRef.current.transparent = transparent;
    stRef.current.alpha = transparent ? 0.42 : 1.0;
  }, [on, transparent]);

  useEffect(() => {
    let cancelled = false;
    // body.soa is 168 MB full-res — too big for git, so it lives as a RELEASE asset
    // (kept out of the repo entirely). It ships gzipped (~80 MB); we fetch the .gz
    // from the GitHub release and inflate in the browser via DecompressionStream,
    // keeping ALL points. A same-origin /body.soa.gz (if a deploy chooses to copy
    // the asset into its static dir) is tried first so the page still works offline.
    const REL = 'https://github.com/AdaWorldAPI/q2/releases/download/fma-body-soa-v3-v1';
    const inflate = async (resp: Response): Promise<ArrayBuffer> => {
      if (resp.body && typeof DecompressionStream !== 'undefined') {
        const stream = resp.body.pipeThrough(new DecompressionStream('gzip'));
        return await new Response(stream).arrayBuffer();
      }
      // no DecompressionStream: fall back to the raw (uncompressed) wire
      const raw = await fetch(`${REL}/body.soa`);
      if (!raw.ok) throw new Error(`HTTP ${raw.status} fetching body.soa`);
      return await raw.arrayBuffer();
    };
    const load = async (): Promise<ArrayBuffer> => {
      const local = await fetch('/body.soa.gz').catch(() => null);
      if (local && local.ok) return inflate(local);
      const rel = await fetch(`${REL}/body.soa.gz`);
      if (!rel.ok) throw new Error(`HTTP ${rel.status} fetching body.soa.gz from release`);
      return inflate(rel);
    };
    load()
      .then((buf) => { if (!cancelled) setData(decodeBso1(buf)); })
      .catch((e) => { if (!cancelled) setError(String(e)); });
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    const container = ref.current;
    if (!container || !data) return;
    return mount(container, data.mesh, stRef.current, setStats);
  }, [data]);

  const btn = (active: boolean): React.CSSProperties => ({
    padding: '5px 11px', borderRadius: 6, border: `1px solid ${active ? '#5a7fa8' : '#2a3242'}`,
    background: active ? '#16202e' : '#0e1219', color: active ? '#cdd9e5' : '#6a7686',
    font: '12px ui-monospace, monospace', cursor: 'pointer',
  });

  return (
    <div style={{ position: 'fixed', inset: 0, background: '#0a0e17', overflow: 'hidden' }}>
      <div ref={ref} style={{ position: 'absolute', inset: 0 }} />

      <div style={{ position: 'absolute', top: 12, left: 16, color: '#cdd9e5', font: '13px ui-monospace, monospace', pointerEvents: 'none' }}>
        <div style={{ fontSize: 15, color: '#fff' }}>FMA body · full-res V3 substrate · compartments</div>
        <div style={{ opacity: 0.7 }}>
          {data
            ? `${data.mesh.triCount.toLocaleString()} triangles · ${data.mesh.vertCount.toLocaleString()} vertices — ALL points, drag to orbit`
            : error
              ? ''
              : 'loading body.soa (168 MB, full-res)…'}
        </div>
        {data && (
          <div style={{ opacity: 0.6, marginTop: 2 }}>
            {data.conceptCount.toLocaleString()} concepts on CLASSID_FMA_V3
            {' '}(0x{data.classid.toString(16).padStart(8, '0')})
          </div>
        )}
        {stats && (
          <div style={{ opacity: 0.5, marginTop: 2 }}>
            {stats.fps} fps · smooth Phong surface · toggle compartments →
          </div>
        )}
      </div>

      {error && (
        <div style={{ position: 'absolute', top: '46%', width: '100%', textAlign: 'center', color: '#ff8095', font: '13px ui-monospace, monospace' }}>
          {error}
          <div style={{ opacity: 0.7, marginTop: 6 }}>
            bake: <code>python3 crates/osint-bake/tools/bake_body_v3.py … &amp;&amp; cargo run -p osint-bake --bin body</code>
          </div>
        </div>
      )}

      {/* compartment layer toggles + solid/transparent (right) — same gating as /fma-body */}
      <div style={{ position: 'absolute', top: 12, right: 16, display: 'flex', flexDirection: 'column', gap: 8, alignItems: 'flex-end' }}>
        <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', justifyContent: 'flex-end', maxWidth: 360 }}>
          {LAYERS.map((l) => (
            <button key={l.id} style={btn(on[l.id])} onClick={() => setOn((p) => ({ ...p, [l.id]: !p[l.id] }))}>
              <span style={{ display: 'inline-block', width: 8, height: 8, borderRadius: 4, background: l.color, marginRight: 6, verticalAlign: 'middle' }} />
              {l.name}
            </button>
          ))}
        </div>
        <button style={btn(transparent)} onClick={() => setTransparent((v) => !v)}>{transparent ? 'transparent' : 'solid'} ⇄</button>
        <div style={{ display: 'flex', gap: 14, font: '12px ui-monospace, monospace', marginTop: 2 }}>
          <a href="/torso-live" style={{ color: '#7fa6c4', textDecoration: 'none' }}>decimated torso →</a>
          <a href="/fma-body" style={{ color: '#7fa6c4', textDecoration: 'none' }}>2k layered →</a>
        </div>
      </div>

      <div style={{ position: 'absolute', bottom: 10, left: 16, color: '#5a6b7e', font: '10px ui-monospace, monospace', maxWidth: '70%', pointerEvents: 'none' }}>
        BodyParts3D, (c) The Database Center for Life Science, licensed under CC Attribution 4.0 International
      </div>
    </div>
  );
}
