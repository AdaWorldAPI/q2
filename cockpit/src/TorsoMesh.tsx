// FMA torso · live-orbit FILLED SMOOTH TRIANGLE MESH of REAL anatomy.
//
// Renders cockpit/public/torso.mesh — an indexed triangle surface vertex-cluster-
// decimated from the BodyParts3D is_a meshes, coloured per is_a tissue. Operator
// directive (2026-06-24): "connect to a triangle filled surface ... kurvenlineal over
// triangles (Quadro/AutoCAD)". So: filled THREE.Mesh, Gouraud/Phong smooth shading
// from the cell-averaged per-vertex normals — a solid CAD surface (ivory bone, red
// muscle, blue cartilage, ...), the Open 3D Man material-surface aesthetic. Triangle
// surfaces decisively beat the surfel splat (no sequins, no fog).
//
// Skin/flesh is cut away by default (uFloor) so the anatomy shows; press S to toggle
// the skin shell back (note: the BodyParts3D skin set is sparse, so the shell speckles).
//
// Geometry/data: BodyParts3D, (c) The Database Center for Life Science,
// licensed CC-BY 4.0. Attribution is shown in-view (required by the licence).
import { useEffect, useRef, useState } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

const PAGE_BG = 0x0a0e17;
const DEFAULT_FLOOR = 0.5; // gate skin (0.14) + flesh (0.45) out; keep muscle (0.55)+

interface Mesh {
  vertCount: number;
  triCount: number;
  positions: Float32Array;
  normals: Float32Array;
  colors: Uint8Array;
  opacity: Float32Array;
  index: Uint32Array;
}

// SPM1 wire (mirrors bake_torso_mesh.py): little-endian
//   header 40 B: magic "SPM1" | vert_count u32 | tri_count u32 | node_count u32
//                | bbox_min 3f | bbox_max 3f
//   vertex body  vert_count x 21 B: pos 3f | normal 3i8 | rgb 3u8 | opacity u8 | node_row u16
//   index body   tri_count x 12 B: 3x u32 (global vertex indices)
// Orientation (x,y,z) -> (-x, z, y): a proper rotation (det +1, so triangle winding
// and gl_FrontFacing stay correct) that stands the body head-up in three.js's Y-up
// world (model +Z superior -> world +Y up; +Y anterior -> +Z toward viewer). No
// screen-flip — three.js is Y-up, so the rotation lives entirely in the decode.
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
  const opacity = new Float32Array(vertCount);
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
    opacity[i] = dv.getUint8(b + 18) / 255;
    // node_row u16 at b+19 — used by the picker view, not here
  }
  const ioff = voff + vertCount * 21;
  const index = new Uint32Array(triCount * 3);
  for (let t = 0; t < triCount; t++) {
    const b = ioff + t * 12;
    index[t * 3] = dv.getUint32(b, true);
    index[t * 3 + 1] = dv.getUint32(b + 4, true);
    index[t * 3 + 2] = dv.getUint32(b + 8, true);
  }
  return { vertCount, triCount, positions, normals, colors, opacity, index };
}

interface Manifest {
  attribution: string;
  concepts: number;
  verts: number;
  tris: number;
}

// Phong smooth shading: interpolate the cell-averaged world normal across each filled
// face, light per-pixel (hemisphere + key + fill, light fixed in world so the orbit
// stays consistently lit). The mesh is static (camera orbits), so aNormal IS the world
// normal. gl_FrontFacing flips it for the back side (clustered winding is inconsistent).
const VERT = `
attribute vec3 aNormal;
attribute vec3 aColor;
attribute float aOpacity;
varying vec3 vNormal;
varying vec3 vColor;
varying float vOpacity;
void main() {
  vNormal = aNormal;
  vColor = aColor;
  vOpacity = aOpacity;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}`;
const FRAG = `
precision mediump float;
uniform float uFloor;
varying vec3 vNormal;
varying vec3 vColor;
varying float vOpacity;
void main() {
  if (vOpacity < uFloor) discard;                 // cut the skin/flesh shell
  vec3 n = normalize(vNormal);
  if (!gl_FrontFacing) n = -n;                     // two-sided
  const vec3 L = vec3(-0.401, 0.783, 0.476);
  float ndl = max(dot(n, L), 0.0);
  float hemi = 0.34 + 0.20 * (n.y * 0.5 + 0.5);
  float fill = 0.12 * (-n.x * 0.5 + 0.5);
  float shade = min(hemi + fill + 0.92 * ndl, 1.3);
  gl_FragColor = vec4(vColor * shade, 1.0);        // OPAQUE solid surface
}`;

function mount(
  container: HTMLDivElement,
  mesh: Mesh,
  floorRef: { value: number },
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
  geom.setAttribute('aOpacity', new THREE.BufferAttribute(mesh.opacity, 1));
  geom.setIndex(new THREE.BufferAttribute(mesh.index, 1));
  const mat = new THREE.ShaderMaterial({
    vertexShader: VERT,
    fragmentShader: FRAG,
    uniforms: { uFloor: { value: floorRef.value } },
    side: THREE.DoubleSide, // clustered winding can be inconsistent
    transparent: false,
    depthTest: true,
    depthWrite: true,
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

  // Adaptive-FPS: one indexed mesh is a single draw, so we can't subsample triangles
  // like the point cloud — instead drop pixelRatio when the EMA frame-time slips, and
  // recover when frames are cheap. 831K tris is one draw; this keeps it smooth.
  let raf = 0;
  let ema = 16.6;
  let last = performance.now();
  let sinceStat = 0;
  const tick = () => {
    raf = requestAnimationFrame(tick);
    const now = performance.now();
    ema = ema * 0.9 + (now - last) * 0.1;
    last = now;
    const pr = ema > 30 ? 1 : Math.min(window.devicePixelRatio, 2);
    if (renderer.getPixelRatio() !== pr) renderer.setPixelRatio(pr);
    mat.uniforms.uFloor.value = floorRef.value;
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

export function TorsoMesh() {
  const ref = useRef<HTMLDivElement>(null);
  const [mesh, setMesh] = useState<Mesh | null>(null);
  const [manifest, setManifest] = useState<Manifest | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [stats, setStats] = useState<{ fps: number } | null>(null);
  const [skin, setSkin] = useState(false);
  const floorRef = useRef({ value: DEFAULT_FLOOR });

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
    fetch('/torso.mesh')
      .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status} fetching torso.mesh`); return r.arrayBuffer(); })
      .then((buf) => { if (!cancelled) setMesh(decodeSpm1(buf)); })
      .catch((e) => { if (!cancelled) setError(String(e)); });
    fetch('/torso.mesh.manifest.json')
      .then((r) => (r.ok ? r.json() : null))
      .then((m) => { if (!cancelled && m) setManifest(m as Manifest); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    const container = ref.current;
    if (!container || !mesh) return;
    return mount(container, mesh, floorRef.current, setStats);
  }, [mesh]);

  return (
    <div style={{ position: 'fixed', inset: 0, background: '#0a0e17', overflow: 'hidden' }}>
      <div ref={ref} style={{ position: 'absolute', inset: 0 }} />

      <div style={{ position: 'absolute', top: 12, left: 16, color: '#cdd9e5', font: '13px ui-monospace, monospace', pointerEvents: 'none' }}>
        <div style={{ fontSize: 15, color: '#fff' }}>FMA torso · solid surface</div>
        <div style={{ opacity: 0.7 }}>
          {manifest
            ? `${manifest.tris.toLocaleString()} triangles · ${manifest.concepts} structures — real BodyParts3D geometry, drag to orbit`
            : mesh
              ? `${mesh.triCount.toLocaleString()} triangles — drag to orbit`
              : error
                ? ''
                : 'loading torso.mesh…'}
        </div>
        {stats && (
          <div style={{ opacity: 0.5, marginTop: 2 }}>
            {stats.fps} fps · smooth Gouraud surface · S = skin {skin ? 'on' : 'off'}
          </div>
        )}
      </div>

      {error && (
        <div style={{ position: 'absolute', top: '46%', width: '100%', textAlign: 'center', color: '#ff8095', font: '13px ui-monospace, monospace' }}>
          {error}
          <div style={{ opacity: 0.7, marginTop: 6 }}>
            run: <code>python3 crates/osint-bake/tools/bake_torso_mesh.py …</code> → cockpit/public/torso.mesh
          </div>
        </div>
      )}

      <div style={{ position: 'absolute', top: 14, right: 16, font: '12px ui-monospace, monospace', display: 'flex', gap: 14 }}>
        <a href="/torso" style={{ color: '#7fa6c4', textDecoration: 'none' }}>turntable →</a>
        <a href="/torso-splat" style={{ color: '#7fa6c4', textDecoration: 'none' }}>surfels →</a>
        <a href="/torso-map" style={{ color: '#7fa6c4', textDecoration: 'none' }}>map →</a>
      </div>

      <div style={{ position: 'absolute', bottom: 10, left: 16, color: '#5a6b7e', font: '10px ui-monospace, monospace', maxWidth: '70%', pointerEvents: 'none' }}>
        {manifest?.attribution ?? 'BodyParts3D, (c) The Database Center for Life Science, licensed under CC Attribution 4.0 International'}
      </div>
    </div>
  );
}
