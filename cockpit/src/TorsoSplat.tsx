// FMA torso · live-orbit OPAQUE surfel render of REAL anatomy.
//
// Renders cockpit/public/torso.splat — per-triangle surfels baked from BodyParts3D
// (the FMA-keyed 3D mesh database; meshes live in one shared whole-body frame, so
// these are real anatomical coordinates). The bake is crates/osint-bake/tools/
// bake_torso_splat.py over the is_a TYPE tree (every structure classified to its
// tissue, coloured per tissue, sized per-structure by k-NN local spacing).
//
// OPAQUE, NOT a soft gaussian. Operator directive (2026-06-24): "rather a triangle
// without gaussian than a ghostbuster pretending to do gaussian splat." So each
// surfel is an opaque depth-tested disk — nearest surface wins, hard edge, NO alpha
// accumulation → no fog, no halo. The previous transparent soft-alpha point material
// was the ghost. `uFloor` gates the translucent skin/flesh shell out (press S to
// toggle) so the crisp anatomy underneath shows; see-inside is a select/clip move,
// never a fog.
//
// Geometry/data: BodyParts3D, (c) The Database Center for Life Science,
// licensed CC-BY 4.0. Attribution is shown in-view (required by the licence).
import { useEffect, useRef, useState } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

const PAGE_BG = 0x0a0e17;
const DEFAULT_FLOOR = 0.5; // gate skin (0.14) + flesh (0.45) out; keep muscle (0.55)+

interface Spl3 {
  count: number;
  scaleMax: number;
  bboxMin: [number, number, number];
  bboxMax: [number, number, number];
  positions: Float32Array;
  colors: Uint8Array;
  normals: Float32Array;
  opacity: Float32Array;
  scale: Float32Array; // per-surfel world disk radius (k-NN sized)
}

// Decode the SPL3 wire (mirrors bake_torso_splat.py): little-endian
//   header 40 B: magic "SPL3" | count u32 | node_count u32 | scale_max f32
//                | bbox_min 3f | bbox_max 3f
//   body count x 22 B: pos 3f (12) | normal 3i8 (3) | rgb 3u8 (3) | opacity u8 (1)
//                | scale u8 (1) | node_row u16 (2)
// scale dequantizes as scale_byte/255 * scale_max -> the surfel's world disk radius.
function decodeSpl3(buf: ArrayBuffer): Spl3 {
  const dv = new DataView(buf);
  const magic = String.fromCharCode(dv.getUint8(0), dv.getUint8(1), dv.getUint8(2), dv.getUint8(3));
  if (magic !== 'SPL3') throw new Error(`bad magic "${magic}" (expected SPL3)`);
  const count = dv.getUint32(4, true);
  const scaleMax = dv.getFloat32(12, true);
  const bboxMin: [number, number, number] = [
    dv.getFloat32(16, true), dv.getFloat32(20, true), dv.getFloat32(24, true),
  ];
  const bboxMax: [number, number, number] = [
    dv.getFloat32(28, true), dv.getFloat32(32, true), dv.getFloat32(36, true),
  ];
  const off = 40;
  const positions = new Float32Array(count * 3);
  const colors = new Uint8Array(count * 3);
  const normals = new Float32Array(count * 3);
  const opacity = new Float32Array(count);
  const scale = new Float32Array(count);
  for (let i = 0; i < count; i++) {
    const b = off + i * 22;
    // upright: +90 about X, (x,y,z) -> (x,-z,y) (matches bake/driver); normals too.
    const x = dv.getFloat32(b, true), y = dv.getFloat32(b + 4, true), z = dv.getFloat32(b + 8, true);
    positions[i * 3] = x; positions[i * 3 + 1] = -z; positions[i * 3 + 2] = y;
    normals[i * 3] = dv.getInt8(b + 12) / 127;
    normals[i * 3 + 1] = -dv.getInt8(b + 14) / 127;
    normals[i * 3 + 2] = dv.getInt8(b + 13) / 127;
    colors[i * 3] = dv.getUint8(b + 15);
    colors[i * 3 + 1] = dv.getUint8(b + 16);
    colors[i * 3 + 2] = dv.getUint8(b + 17);
    opacity[i] = dv.getUint8(b + 18) / 255;
    scale[i] = (dv.getUint8(b + 19) / 255) * scaleMax;
  }
  return { count, scaleMax, bboxMin, bboxMax, positions, colors, normals, opacity, scale };
}

interface Manifest {
  attribution: string;
  concepts: number;
  meshes: number;
  gaussians: number;
}

// Flat normal shading (hemisphere + diffuse + fill, light fixed in object space) —
// same recipe as the CPU driver so the views agree. The point size is the surfel's
// own world radius projected to pixels (perspective-correct), not a global size, so
// dense regions stay tight and the surface reads crisp. aOpacity drives the uFloor
// gate (skin/flesh dropped); the colour itself is opaque.
const VERT = `
attribute vec3 aColor;
attribute vec3 aNormal;
attribute float aOpacity;
attribute float aScale;
uniform float uFocalPx;   // viewportH / (2 tan(fov/2)) * pixelRatio
uniform float uFloor;     // gate surfels below this opacity (skin/flesh shell)
uniform float uGain;      // global size multiplier (user fine-tune)
varying vec3 vColor;
varying float vGate;
void main() {
  vGate = aOpacity < uFloor ? -1.0 : 1.0;
  vec3 n = normalize(aNormal);
  const vec3 L = vec3(-0.401, 0.783, 0.476);
  float ndl = max(dot(n, L), 0.0);
  float hemi = 0.34 + 0.20 * (n.y * 0.5 + 0.5);
  float fill = 0.12 * (-n.x * 0.5 + 0.5);
  float shade = min(hemi + fill + 0.92 * ndl, 1.3);
  vColor = aColor * shade;
  vec4 mv = modelViewMatrix * vec4(position, 1.0);
  gl_Position = projectionMatrix * mv;
  // world disk diameter -> screen px; >= 1.5 px so no sub-pixel holes.
  gl_PointSize = max(2.0 * aScale * uGain * uFocalPx / max(-mv.z, 0.02), 1.5);
}`;
// OPAQUE: hard-edged disk, alpha = 1. No transparency, no smoothstep tail. The depth
// buffer (depthTest + depthWrite) keeps the nearest surfel per pixel — that is the
// crispness. vGate < 0 discards the skin/flesh shell.
const FRAG = `
precision mediump float;
varying vec3 vColor;
varying float vGate;
void main() {
  if (vGate < 0.0) discard;
  vec2 c = gl_PointCoord - 0.5;
  if (dot(c, c) > 0.25) discard;
  gl_FragColor = vec4(vColor, 1.0);
}`;

function mount(
  container: HTMLDivElement,
  splat: Spl3,
  floorRef: { value: number },
  gainRef: { value: number },
  onStats: (s: { fps: number; frac: number }) => void,
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

  // LazyLock: build the geometry ONCE; a shuffled index makes an adaptive drawRange
  // prefix a uniform spatial subsample. Orientation is baked into the decode, so no
  // points.rotation is needed (the body already stands head-up).
  const geom = new THREE.BufferGeometry();
  geom.setAttribute('position', new THREE.BufferAttribute(splat.positions, 3));
  geom.setAttribute('aColor', new THREE.BufferAttribute(splat.colors, 3, true)); // u8 normalized
  geom.setAttribute('aNormal', new THREE.BufferAttribute(splat.normals, 3));
  geom.setAttribute('aOpacity', new THREE.BufferAttribute(splat.opacity, 1));
  geom.setAttribute('aScale', new THREE.BufferAttribute(splat.scale, 1));
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
    uniforms: {
      uFocalPx: { value: focalPx() },
      uFloor: { value: floorRef.value },
      uGain: { value: gainRef.value },
    },
    transparent: false, // OPAQUE — the ghost was the soft-alpha blend
    depthTest: true,
    depthWrite: true,
  });
  const points = new THREE.Points(geom, mat);
  scene.add(points);

  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.autoRotate = true;
  controls.autoRotateSpeed = 0.6;
  controls.target.set(0, 0, 0);
  controls.minDistance = 0.6;
  controls.maxDistance = 12;

  // Adaptive-FPS: EMA of the frame time thins the draw range (uniform subsample) +
  // drops pixelRatio before a stutter lands, recovering when frames are cheap. The
  // fixed autorotate is deterministic, so this also stays smooth and predictable.
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
    mat.uniforms.uFocalPx.value = focalPx(); // tracks pixelRatio + resize
    mat.uniforms.uFloor.value = floorRef.value;
    mat.uniforms.uGain.value = gainRef.value;
    controls.update();
    renderer.render(scene, camera);
    if (++sinceStat >= 20) {
      sinceStat = 0;
      onStats({ fps: Math.round(1000 / Math.max(ema, 1)), frac: active / splat.count });
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

export function TorsoSplat() {
  const ref = useRef<HTMLDivElement>(null);
  const [splat, setSplat] = useState<Spl3 | null>(null);
  const [manifest, setManifest] = useState<Manifest | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [stats, setStats] = useState<{ fps: number; frac: number } | null>(null);
  const [skin, setSkin] = useState(false);
  const floorRef = useRef({ value: DEFAULT_FLOOR });
  const gainRef = useRef({ value: 1.0 });

  // S toggles the skin/flesh shell (uFloor 0 <-> DEFAULT_FLOOR) without re-decoding.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 's' || e.key === 'S') {
        setSkin((v) => {
          const nv = !v;
          floorRef.current.value = nv ? 0.0 : DEFAULT_FLOOR;
          return nv;
        });
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  useEffect(() => {
    let cancelled = false;
    fetch('/torso.splat')
      .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status} fetching torso.splat`); return r.arrayBuffer(); })
      .then((buf) => { if (!cancelled) setSplat(decodeSpl3(buf)); })
      .catch((e) => { if (!cancelled) setError(String(e)); });
    fetch('/torso.manifest.json')
      .then((r) => (r.ok ? r.json() : null))
      .then((m) => { if (!cancelled && m) setManifest(m as Manifest); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    const container = ref.current;
    if (!container || !splat) return;
    return mount(container, splat, floorRef.current, gainRef.current, setStats);
  }, [splat]);

  return (
    <div style={{ position: 'fixed', inset: 0, background: '#0a0e17', overflow: 'hidden' }}>
      <div ref={ref} style={{ position: 'absolute', inset: 0 }} />

      <div style={{ position: 'absolute', top: 12, left: 16, color: '#cdd9e5', font: '13px ui-monospace, monospace', pointerEvents: 'none' }}>
        <div style={{ fontSize: 15, color: '#fff' }}>FMA torso · opaque surfels</div>
        <div style={{ opacity: 0.7 }}>
          {manifest
            ? `${manifest.gaussians.toLocaleString()} surfels · ${manifest.meshes} meshes · ${manifest.concepts} structures — real BodyParts3D geometry, drag to orbit`
            : splat
              ? `${splat.count.toLocaleString()} surfels — drag to orbit`
              : error
                ? ''
                : 'loading torso.splat…'}
        </div>
        {stats && (
          <div style={{ opacity: 0.5, marginTop: 2 }}>
            {stats.fps} fps · {Math.round(stats.frac * 100)}% drawn (adaptive) · S = skin {skin ? 'on' : 'off'}
          </div>
        )}
      </div>

      {error && (
        <div style={{ position: 'absolute', top: '46%', width: '100%', textAlign: 'center', color: '#ff8095', font: '13px ui-monospace, monospace' }}>
          {error}
          <div style={{ opacity: 0.7, marginTop: 6 }}>
            run: <code>python3 crates/osint-bake/tools/bake_torso_splat.py …</code> → cockpit/public/torso.splat
          </div>
        </div>
      )}

      <div style={{ position: 'absolute', top: 14, right: 16, font: '12px ui-monospace, monospace', display: 'flex', gap: 14 }}>
        <a href="/torso" style={{ color: '#7fa6c4', textDecoration: 'none' }}>splat render →</a>
        <a href="/fma" style={{ color: '#7fa6c4', textDecoration: 'none' }}>← FMA heart</a>
      </div>

      <div style={{ position: 'absolute', bottom: 10, left: 16, color: '#5a6b7e', font: '10px ui-monospace, monospace', maxWidth: '70%', pointerEvents: 'none' }}>
        {manifest?.attribution ?? 'BodyParts3D, (c) The Database Center for Life Science, licensed under CC Attribution 4.0 International'}
      </div>
    </div>
  );
}
