// FMA torso · live-orbit gaussian splat of REAL anatomy.
//
// Renders cockpit/public/torso.splat — a gaussian cloud baked from BodyParts3D
// (the FMA-keyed 3D mesh database; meshes live in one shared whole-body frame,
// so these are real anatomical coordinates, not a synthesized layout). The bake
// is crates/osint-bake/tools/bake_torso_splat.py over the FMA `part_of` subtree
// rooted at FMA7181 (trunk). Each of ~100 structures carries its own hue.
//
// This is the "live orbit" companion to the CPU splat3d render (/torso): same
// baked geometry, rendered live in WebGL with OrbitControls. Imperative three.js,
// modelled on OsintScene3D.tsx.
//
// Geometry/data: BodyParts3D, (c) The Database Center for Life Science,
// licensed CC-BY 4.0. Attribution is shown in-view (required by the licence).
import { useEffect, useRef, useState } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

const PAGE_BG = 0x0a0e17;

interface Spl1 {
  count: number;
  radius: number;
  bboxMin: [number, number, number];
  bboxMax: [number, number, number];
  positions: Float32Array;
  colors: Uint8Array;
  normals: Float32Array;
  opacity: Float32Array;
}

// Decode the SPL2 wire (mirrors bake_torso_splat.py): little-endian
//   header 40 B: magic "SPL2" | count u32 | node_count u32 | radius f32
//                | bbox_min 3f | bbox_max 3f
//   body count x 21 B: pos 3f (12) | normal 3i8 (3) | rgb 3u8 (3) | opacity u8 (1)
//                | node_row u16 (2)
// The live-orbit view renders points: it reads pos + rgb and ignores the normal
// and node-row tags (those drive the /torso splat3d render and the /torso-map view).
function decodeSpl1(buf: ArrayBuffer): Spl1 {
  const dv = new DataView(buf);
  const magic = String.fromCharCode(dv.getUint8(0), dv.getUint8(1), dv.getUint8(2), dv.getUint8(3));
  if (magic !== 'SPL2') throw new Error(`bad magic "${magic}" (expected SPL2)`);
  const count = dv.getUint32(4, true);
  const radius = dv.getFloat32(12, true);
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
  for (let i = 0; i < count; i++) {
    const b = off + i * 21;
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
  }
  return { count, radius, bboxMin, bboxMax, positions, colors, normals, opacity };
}

interface Manifest {
  attribution: string;
  root_name: string;
  concepts: number;
  meshes: number;
  gaussians: number;
}

// Lighting from the baked surface normal (hemisphere + diffuse + fill, light fixed
// in object space) + per-gaussian depth-peel opacity. Same recipe as the CPU driver
// and /torso-map, so all three views agree.
const VERT = `
attribute vec3 aColor;
attribute vec3 aNormal;
attribute float aOpacity;
uniform float uSize;
varying vec3 vColor;
varying float vAlpha;
void main() {
  vec3 n = normalize(aNormal);
  const vec3 L = vec3(-0.401, 0.783, 0.476);
  float ndl = max(dot(n, L), 0.0);
  float hemi = 0.34 + 0.20 * (n.y * 0.5 + 0.5);
  float fill = 0.12 * (-n.x * 0.5 + 0.5);
  float shade = min(hemi + fill + 0.92 * ndl, 1.3);
  vColor = aColor * shade;
  vAlpha = aOpacity;
  vec4 mv = modelViewMatrix * vec4(position, 1.0);
  gl_Position = projectionMatrix * mv;
  gl_PointSize = uSize * (1.0 / -mv.z);
}`;
const FRAG = `
precision mediump float;
varying vec3 vColor;
varying float vAlpha;
void main() {
  vec2 c = gl_PointCoord - 0.5;
  float d = dot(c, c);
  if (d > 0.25) discard;
  float a = smoothstep(0.25, 0.04, d) * vAlpha;
  gl_FragColor = vec4(vColor, a);
}`;

function mount(
  container: HTMLDivElement,
  splat: Spl1,
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
  const mat = new THREE.ShaderMaterial({
    vertexShader: VERT,
    fragmentShader: FRAG,
    uniforms: { uSize: { value: Math.max(splat.radius * 1400, 3.0) } },
    transparent: true,
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
  const [splat, setSplat] = useState<Spl1 | null>(null);
  const [manifest, setManifest] = useState<Manifest | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [stats, setStats] = useState<{ fps: number; frac: number } | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetch('/torso.splat')
      .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status} fetching torso.splat`); return r.arrayBuffer(); })
      .then((buf) => { if (!cancelled) setSplat(decodeSpl1(buf)); })
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
    return mount(container, splat, setStats);
  }, [splat]);

  return (
    <div style={{ position: 'fixed', inset: 0, background: '#0a0e17', overflow: 'hidden' }}>
      <div ref={ref} style={{ position: 'absolute', inset: 0 }} />

      <div style={{ position: 'absolute', top: 12, left: 16, color: '#cdd9e5', font: '13px ui-monospace, monospace', pointerEvents: 'none' }}>
        <div style={{ fontSize: 15, color: '#fff' }}>FMA torso · gaussian splat</div>
        <div style={{ opacity: 0.7 }}>
          {manifest
            ? `${manifest.gaussians.toLocaleString()} gaussians · ${manifest.meshes} meshes · ${manifest.concepts} structures — real BodyParts3D geometry, drag to orbit`
            : splat
              ? `${splat.count.toLocaleString()} gaussians — drag to orbit`
              : error
                ? ''
                : 'loading torso.splat…'}
        </div>
        {stats && (
          <div style={{ opacity: 0.5, marginTop: 2 }}>
            {stats.fps} fps · {Math.round(stats.frac * 100)}% drawn (adaptive)
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
