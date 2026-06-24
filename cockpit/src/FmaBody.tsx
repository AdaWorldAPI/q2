// /fma-body — MY full-body FMA viewer (additive to the other session's /torso*).
//
// Renders cockpit/public/fma_body.mesh (baked by `fma`'s cockpit_bake) — the same SPM1
// indexed triangle surface the cockpit already decodes, but the per-vertex `opacity`
// byte carries a clean LAYER id (1 skin · 2 muscle · 3 organ · 4 skeleton · 5 vessel ·
// 6 nerve · 7 connective · 8 other). So the viewer can TOGGLE each layer with a button,
// and switch the whole body between SOLID and TRANSPARENT. Color is the converged
// `tissue` byte (is_a); geometry is real BodyParts3D, vertex-cluster decimated.
//
// This does not touch /torso, /torso-live, /torso-splat, /torso-map (#57/#58).
//
// Geometry/data: BodyParts3D, (c) The Database Center for Life Science, CC-BY 4.0.
import { useEffect, useRef, useState } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

const PAGE_BG = 0x0a0e17;

// layer id (opacity byte) → label + swatch. id 0 unused; index = id.
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
  layer: Float32Array; // per-vertex layer id (from the opacity byte)
  index: Uint32Array;
}

// SPM1 (little-endian): header 40 B | vert 21 B [pos 3f|normal 3i8|rgb 3u8|opacity u8|
// node_row u16] | index 12 B. Orientation (x,y,z)->(-x,z,y): proper rotation (det +1),
// head-up in three.js Y-up — identical to TorsoMesh so both viewers agree.
function decodeSpm1(buf: ArrayBuffer): Mesh {
  const dv = new DataView(buf);
  const magic = String.fromCharCode(dv.getUint8(0), dv.getUint8(1), dv.getUint8(2), dv.getUint8(3));
  if (magic !== 'SPM1') throw new Error(`bad magic "${magic}" (expected SPM1)`);
  const vertCount = dv.getUint32(4, true);
  const triCount = dv.getUint32(8, true);
  const voff = 40;
  const positions = new Float32Array(vertCount * 3);
  const normals = new Float32Array(vertCount * 3);
  const colors = new Uint8Array(vertCount * 3);
  const layer = new Float32Array(vertCount);
  for (let i = 0; i < vertCount; i++) {
    const b = voff + i * 21;
    const x = dv.getFloat32(b, true), y = dv.getFloat32(b + 4, true), z = dv.getFloat32(b + 8, true);
    positions[i * 3] = -x; positions[i * 3 + 1] = z; positions[i * 3 + 2] = y;
    normals[i * 3] = -dv.getInt8(b + 12) / 127;
    normals[i * 3 + 1] = dv.getInt8(b + 14) / 127;
    normals[i * 3 + 2] = dv.getInt8(b + 13) / 127;
    colors[i * 3] = dv.getUint8(b + 15);
    colors[i * 3 + 1] = dv.getUint8(b + 16);
    colors[i * 3 + 2] = dv.getUint8(b + 17);
    layer[i] = dv.getUint8(b + 18); // opacity byte = LAYER id
  }
  const ioff = voff + vertCount * 21;
  const index = new Uint32Array(triCount * 3);
  for (let t = 0; t < triCount; t++) {
    const b = ioff + t * 12;
    index[t * 3] = dv.getUint32(b, true);
    index[t * 3 + 1] = dv.getUint32(b + 4, true);
    index[t * 3 + 2] = dv.getUint32(b + 8, true);
  }
  return { vertCount, triCount, positions, normals, colors, layer, index };
}

// Per-layer visibility via uEnabled[9] (indexed by the vertex layer id) + a global alpha
// for the solid↔transparent switch. Phong smooth shade, two-sided.
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
uniform float uEnabled[9];
uniform float uAlpha;
varying vec3 vNormal;
varying vec3 vColor;
varying float vLayer;
void main() {
  int li = int(vLayer + 0.5);
  if (li < 0 || li > 8 || uEnabled[li] < 0.5) discard;   // layer toggled off
  vec3 n = normalize(vNormal);
  if (!gl_FrontFacing) n = -n;                            // two-sided
  const vec3 L = vec3(-0.401, 0.783, 0.476);
  float ndl = max(dot(n, L), 0.0);
  float hemi = 0.34 + 0.20 * (n.y * 0.5 + 0.5);
  float fill = 0.12 * (-n.x * 0.5 + 0.5);
  float shade = min(hemi + fill + 0.92 * ndl, 1.3);
  gl_FragColor = vec4(vColor * shade, uAlpha);
}`;

interface RenderState {
  enabled: Float32Array; // length 9, indexed by layer id
  alpha: number;
  transparent: boolean;
}

function mount(container: HTMLDivElement, mesh: Mesh, st: RenderState, onStats: (s: { fps: number }) => void): () => void {
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
  geom.setAttribute('aColor', new THREE.BufferAttribute(mesh.colors, 3, true));
  geom.setAttribute('aLayer', new THREE.BufferAttribute(mesh.layer, 1));
  geom.setIndex(new THREE.BufferAttribute(mesh.index, 1));
  const mat = new THREE.ShaderMaterial({
    vertexShader: VERT,
    fragmentShader: FRAG,
    uniforms: { uEnabled: { value: st.enabled }, uAlpha: { value: st.alpha } },
    side: THREE.DoubleSide,
    transparent: st.transparent,
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
  let wasTransparent = st.transparent;
  const tick = () => {
    raf = requestAnimationFrame(tick);
    const now = performance.now();
    ema = ema * 0.9 + (now - last) * 0.1;
    last = now;
    const pr = ema > 30 ? 1 : Math.min(window.devicePixelRatio, 2);
    if (renderer.getPixelRatio() !== pr) renderer.setPixelRatio(pr);
    mat.uniforms.uEnabled.value = st.enabled;
    mat.uniforms.uAlpha.value = st.alpha;
    if (st.transparent !== wasTransparent) {
      mat.transparent = st.transparent;
      mat.depthWrite = !st.transparent;
      mat.needsUpdate = true;
      wasTransparent = st.transparent;
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
    if (renderer.domElement.parentNode === container) container.removeChild(renderer.domElement);
  };
}

export function FmaBody() {
  const ref = useRef<HTMLDivElement>(null);
  const [mesh, setMesh] = useState<Mesh | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [stats, setStats] = useState<{ fps: number } | null>(null);
  // skin off by default (so the anatomy shows); everything else on.
  const [on, setOn] = useState<Record<number, boolean>>({ 1: false, 2: true, 3: true, 4: true, 5: true, 6: true, 7: true, 8: true });
  const [transparent, setTransparent] = useState(false);
  // shared, mutation-friendly render state (read every frame by the GL loop).
  const stRef = useRef<RenderState>({ enabled: new Float32Array([0, 0, 1, 1, 1, 1, 1, 1, 1]), alpha: 1, transparent: false });

  // push React state → the GL render state each change.
  useEffect(() => {
    const e = new Float32Array(9);
    for (let i = 1; i <= 8; i++) e[i] = on[i] ? 1 : 0;
    stRef.current.enabled = e;
    stRef.current.transparent = transparent;
    stRef.current.alpha = transparent ? 0.42 : 1.0;
  }, [on, transparent]);

  useEffect(() => {
    let cancelled = false;
    fetch('/fma_body.mesh')
      .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status} fetching fma_body.mesh`); return r.arrayBuffer(); })
      .then((buf) => { if (!cancelled) setMesh(decodeSpm1(buf)); })
      .catch((e) => { if (!cancelled) setError(String(e)); });
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    const container = ref.current;
    if (!container || !mesh) return;
    return mount(container, mesh, stRef.current, setStats);
  }, [mesh]);

  const btn = (active: boolean): React.CSSProperties => ({
    padding: '5px 11px',
    borderRadius: 6,
    border: `1px solid ${active ? '#5a7fa8' : '#2a3242'}`,
    background: active ? '#16202e' : '#0e1219',
    color: active ? '#cdd9e5' : '#6a7686',
    font: '12px ui-monospace, monospace',
    cursor: 'pointer',
  });

  return (
    <div style={{ position: 'fixed', inset: 0, background: '#0a0e17', overflow: 'hidden' }}>
      <div ref={ref} style={{ position: 'absolute', inset: 0 }} />

      <div style={{ position: 'absolute', top: 12, left: 16, color: '#cdd9e5', font: '13px ui-monospace, monospace', pointerEvents: 'none' }}>
        <div style={{ fontSize: 15, color: '#fff' }}>FMA body · (place:tissue) layers</div>
        <div style={{ opacity: 0.7 }}>
          {mesh ? `${mesh.triCount.toLocaleString()} triangles · drag to orbit` : error ? '' : 'loading fma_body.mesh…'}
        </div>
        {stats && <div style={{ opacity: 0.5, marginTop: 2 }}>{stats.fps} fps · solid surface, layer-gated by the converged key</div>}
      </div>

      {/* layer toggles + solid/transparent */}
      <div style={{ position: 'absolute', top: 12, right: 16, display: 'flex', flexDirection: 'column', gap: 8, alignItems: 'flex-end' }}>
        <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', justifyContent: 'flex-end', maxWidth: 360 }}>
          {LAYERS.map((l) => (
            <button key={l.id} style={btn(on[l.id])} onClick={() => setOn((p) => ({ ...p, [l.id]: !p[l.id] }))}>
              <span style={{ display: 'inline-block', width: 8, height: 8, borderRadius: 4, background: l.color, marginRight: 6, verticalAlign: 'middle' }} />
              {l.name}
            </button>
          ))}
        </div>
        <button style={btn(transparent)} onClick={() => setTransparent((v) => !v)}>
          {transparent ? 'transparent' : 'solid'} ⇄
        </button>
        <div style={{ display: 'flex', gap: 14, font: '12px ui-monospace, monospace', marginTop: 2 }}>
          <a href="/torso-live" style={{ color: '#7fa6c4', textDecoration: 'none' }}>/torso-live →</a>
          <a href="/fma" style={{ color: '#7fa6c4', textDecoration: 'none' }}>/fma graph →</a>
        </div>
      </div>

      {error && (
        <div style={{ position: 'absolute', top: '46%', width: '100%', textAlign: 'center', color: '#ff8095', font: '13px ui-monospace, monospace' }}>
          {error}
          <div style={{ opacity: 0.7, marginTop: 6 }}>
            bake: <code>cargo run -p fma --bin cockpit_bake -- &lt;parts&gt; &lt;element_parts&gt; &lt;converged.tsv&gt; cockpit/public/fma_body.mesh</code>
          </div>
        </div>
      )}

      <div style={{ position: 'absolute', bottom: 10, left: 16, color: '#5a6b7e', font: '10px ui-monospace, monospace', maxWidth: '70%', pointerEvents: 'none' }}>
        BodyParts3D, (c) The Database Center for Life Science, licensed under CC Attribution 4.0 International
      </div>
    </div>
  );
}
