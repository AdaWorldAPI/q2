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
}

// Decode the SPL1 wire (mirrors bake_torso_splat.py): little-endian
//   header 36 B: magic "SPL1" | count u32 | radius f32 | bbox_min 3f | bbox_max 3f
//   body count x 16 B: pos 3f (12) | rgb 3u8 (3) | opacity u8 (1)
function decodeSpl1(buf: ArrayBuffer): Spl1 {
  const dv = new DataView(buf);
  const magic = String.fromCharCode(dv.getUint8(0), dv.getUint8(1), dv.getUint8(2), dv.getUint8(3));
  if (magic !== 'SPL1') throw new Error(`bad magic "${magic}" (expected SPL1)`);
  const count = dv.getUint32(4, true);
  const radius = dv.getFloat32(8, true);
  const bboxMin: [number, number, number] = [
    dv.getFloat32(12, true), dv.getFloat32(16, true), dv.getFloat32(20, true),
  ];
  const bboxMax: [number, number, number] = [
    dv.getFloat32(24, true), dv.getFloat32(28, true), dv.getFloat32(32, true),
  ];
  const off = 36;
  const positions = new Float32Array(count * 3);
  const colors = new Uint8Array(count * 3);
  for (let i = 0; i < count; i++) {
    const b = off + i * 16;
    positions[i * 3] = dv.getFloat32(b, true);
    positions[i * 3 + 1] = dv.getFloat32(b + 4, true);
    positions[i * 3 + 2] = dv.getFloat32(b + 8, true);
    colors[i * 3] = dv.getUint8(b + 12);
    colors[i * 3 + 1] = dv.getUint8(b + 13);
    colors[i * 3 + 2] = dv.getUint8(b + 14);
  }
  return { count, radius, bboxMin, bboxMax, positions, colors };
}

interface Manifest {
  attribution: string;
  root_name: string;
  concepts: number;
  meshes: number;
  gaussians: number;
}

// A soft round sprite so each gaussian reads as a splat, not a hard square.
function gaussianSprite(): THREE.CanvasTexture {
  const s = 64;
  const cv = document.createElement('canvas');
  cv.width = cv.height = s;
  const ctx = cv.getContext('2d')!;
  const g = ctx.createRadialGradient(s / 2, s / 2, 0, s / 2, s / 2, s / 2);
  g.addColorStop(0, 'rgba(255,255,255,1)');
  g.addColorStop(0.5, 'rgba(255,255,255,0.85)');
  g.addColorStop(1, 'rgba(255,255,255,0)');
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, s, s);
  const tex = new THREE.CanvasTexture(cv);
  tex.needsUpdate = true;
  return tex;
}

function mount(container: HTMLDivElement, splat: Spl1): () => void {
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
  geom.setAttribute('position', new THREE.BufferAttribute(splat.positions, 3));
  geom.setAttribute('color', new THREE.BufferAttribute(splat.colors, 3, true)); // u8 normalized

  const sprite = gaussianSprite();
  const mat = new THREE.PointsMaterial({
    size: Math.max(splat.radius * 3.4, 0.012),
    sizeAttenuation: true,
    vertexColors: true,
    map: sprite,
    alphaTest: 0.28,
    transparent: true,
    depthWrite: true,
  });
  const points = new THREE.Points(geom, mat);
  // BodyParts3D's long axis (superior-inferior) is model +Z; stand it upright so
  // the torso's height maps to world +Y.
  points.rotation.x = -Math.PI / 2;
  scene.add(points);

  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.autoRotate = true;
  controls.autoRotateSpeed = 0.6;
  controls.target.set(0, 0, 0);
  controls.minDistance = 0.6;
  controls.maxDistance = 12;

  let raf = 0;
  const tick = () => {
    raf = requestAnimationFrame(tick);
    controls.update();
    renderer.render(scene, camera);
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
    sprite.dispose();
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
    return mount(container, splat);
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
      </div>

      {error && (
        <div style={{ position: 'absolute', top: '46%', width: '100%', textAlign: 'center', color: '#ff8095', font: '13px ui-monospace, monospace' }}>
          {error}
          <div style={{ opacity: 0.7, marginTop: 6 }}>
            run: <code>python3 crates/osint-bake/tools/bake_torso_splat.py …</code> → cockpit/public/torso.splat
          </div>
        </div>
      )}

      <a href="/fma" style={{ position: 'absolute', top: 14, right: 16, color: '#7fa6c4', font: '12px ui-monospace, monospace', textDecoration: 'none' }}>
        ← FMA heart
      </a>

      <div style={{ position: 'absolute', bottom: 10, left: 16, color: '#5a6b7e', font: '10px ui-monospace, monospace', maxWidth: '70%', pointerEvents: 'none' }}>
        {manifest?.attribution ?? 'BodyParts3D, (c) The Database Center for Life Science, licensed under CC Attribution 4.0 International'}
      </div>
    </div>
  );
}
