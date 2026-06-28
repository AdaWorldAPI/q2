// /helix — EXPERIMENTAL helix-orientation viewer. Parallel to /body (BodyV3); shares
// NOTHING with it so the working /body can never break. Same baked wire (`body.soa.gz`,
// BSO2) — but instead of computing float normals from the mesh, it shades from the
// per-vertex *helix normal* bytes that /body bakes and skips.
//
// The helix normal (offsets +3,+4 within each 6-byte `pos3|nrm3` helix block) is a
// Fisher-2z geodesic code: byte = index into a 256-dir golden-spiral codebook
// (`helix_orient` in ndarray; level-0 = codebook(π), level-1 refinement = codebook(0.40)).
// The helix normal is the canonical lance-graph::helix::Signed360 (6 bytes, place-coupled
// to the HHTL address): rim endpoint pair + signed polar lift + golden azimuth. We decode
// it ONCE at load and re-encode octahedrally to a normalized-i8 ×2 vertex attribute — the
// GPU then reads the normal natively (normalized fetch, ~4-ALU octa decode), no per-frame
// work, no texture. REQUIRES a canonical helix bake (soabake → helix::encode_signed); the
// old helix_orient artifact is a different codec and is NOT read here.
//
// Reads an optional stamped artifact first (`/body.helix.<stamp>.soa.gz` via
// `/body.manifest.json`) then falls back to the shared `/body.soa.gz`, so a future
// helix-tuned bake can be swapped in without deleting the working one.
import { useEffect, useRef, useState } from 'react';
import * as THREE from 'three';

const PAGE_BG = 0x0a0d12;
const REL = 'https://github.com/AdaWorldAPI/q2/releases/download/fma-body-soa-v3-v1';

const LAYERS = [
  { id: 1, name: 'skin', color: '#dba88a' }, { id: 2, name: 'muscle', color: '#bd5c57' },
  { id: 3, name: 'organ', color: '#cc9484' }, { id: 4, name: 'skeleton', color: '#ebe0c7' },
  { id: 5, name: 'vessel', color: '#cc3838' }, { id: 6, name: 'nervous', color: '#ebd152' },
  { id: 7, name: 'connective', color: '#e0dbcc' }, { id: 8, name: 'other', color: '#9696a0' },
];
const hexRgb = (h: string): [number, number, number] =>
  [parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16)];
const LAYER_RGB: Record<number, [number, number, number]> = Object.fromEntries(LAYERS.map((l) => [l.id, hexRgb(l.color)]));
const frac = (x: number) => x - Math.floor(x);

// per-concept tint (matches /body's non-vessel scheme so the two viewers read alike).
function conceptColor(layerId: number, row: number): [number, number, number] {
  const base = LAYER_RGB[layerId] ?? [150, 150, 160];
  const h = frac(Math.sin(row * 12.9898) * 43758.5453);
  const bright = 0.82 + 0.34 * h;
  const tilt = (s: number) => 1 + 0.13 * (frac(Math.sin(row * s) * 9711.13) - 0.5) * 2;
  return [
    Math.min(255, base[0] * bright * tilt(1.7)),
    Math.min(255, base[1] * bright * tilt(2.9)),
    Math.min(255, base[2] * bright * tilt(4.1)),
  ];
}

const HALF_LUT: Float32Array = (() => {
  const t = new Float32Array(65536);
  for (let h = 0; h < 65536; h++) {
    const s = (h & 0x8000) ? -1 : 1, e = (h & 0x7c00) >> 10, f = h & 0x03ff;
    t[h] = e === 0 ? s * Math.pow(2, -14) * (f / 1024)
      : e === 0x1f ? (f ? NaN : s * Infinity) : s * Math.pow(2, e - 15) * (1 + f / 1024);
  }
  return t;
})();

// ── canonical lance-graph::helix::Signed360 decode (6 bytes, full sphere) ──
// Wire (LE): [rim.start, rim.end, rim.floor_version, polar, azimuth_lo, azimuth_hi].
//  polar   → signed lift y: |y| in 7 bits, sign in the PARTITION (≥128 upper, <128 lower).
//  azimuth → φ = az_u16 / 65536 · 2π   (golden angle n·φ over the full 360°).
//  r = √(1−y²);  world (X,Z,Y) = (r·sinφ, r·cosφ, y)  [placement.rs cartesian; Y = lift].
// The rim endpoint pair is the place-anchored metric carrier (start_idx = HHTL place);
// for the RENDER normal only (polar, azimuth) are needed — r follows from the unit sphere.
type V3 = [number, number, number];
const TAU = Math.PI * 2;
function decodeSigned360Display(b: Uint8Array, off: number): V3 {
  const polar = b[off + 3];
  const y = polar >= 128 ? (polar - 128) / 127 : -(127 - polar) / 127;
  const az = ((b[off + 4] | (b[off + 5] << 8)) / 65536) * TAU;
  const r = Math.sqrt(Math.max(0, 1 - y * y));
  const X = r * Math.sin(az), Z = r * Math.cos(az);   // world; up-axis = y (lift)
  return [-X, Z, y];   // (x,y,z)->(-x,z,y) display remap of world normal (X, y, Z)
}
// octahedral encode of a unit vector → 2 signed components in [-1,1] → i8. ~0.2° at int8,
// below the 0.3° Fisher-2z precision (no degradation); GPU reads it as a normalized fetch.
function octEncode(n: V3): [number, number] {
  const l1 = Math.abs(n[0]) + Math.abs(n[1]) + Math.abs(n[2]) || 1e-12;
  let ox = n[0] / l1, oy = n[1] / l1;
  if (n[2] < 0) {
    const x = ox, y = oy;
    ox = (1 - Math.abs(y)) * (x >= 0 ? 1 : -1);
    oy = (1 - Math.abs(x)) * (y >= 0 ? 1 : -1);
  }
  const q = (v: number) => Math.max(-127, Math.min(127, Math.round(v * 127)));
  return [q(ox), q(oy)];
}

interface Decoded {
  nVerts: number; nTris: number;
  positions: Float32Array; index: Uint32Array;
  colors: Uint8Array; nrmOct: Int8Array; layer: Float32Array;
  concepts: number;
}

function decode(buf: ArrayBuffer): Decoded {
  const dv = new DataView(buf);
  const magic = String.fromCharCode(dv.getUint8(0), dv.getUint8(1), dv.getUint8(2), dv.getUint8(3));
  if (magic !== 'BSO2') throw new Error(`bad magic "${magic}"`);
  const ver = dv.getUint16(4, true);
  const posBytes = ver >= 4 ? 6 : 12;
  const nC = dv.getUint32(6, true), nV = dv.getUint32(10, true), nT = dv.getUint32(14, true);
  let o = 18;
  o += 16 * nC;                       // guid
  const matOff = o; o += nC;          // material u8 (unused here)
  const layerOff = o; o += nC;        // LAYER u8
  o += 4 * nC;                        // label idx
  o += 12 * nC;                       // centroid
  o += 8 * nC;                        // vrange
  const posOff = o; o += posBytes * nV;
  const helixOff = o; o += 6 * nV;    // pos3 | nrm3 — we read the nrm half
  const rowOff = o; o += 4 * nV;
  const idxOff = o; o += 12 * nT;
  void matOff;

  const cLayer = new Uint8Array(buf.slice(layerOff, layerOff + nC));

  // positions (ver 5 = F16 via LUT) → display remap (-x, z, y)
  let srcPos: Float32Array;
  if (ver >= 5) {
    const hf = new Uint16Array(buf.slice(posOff, posOff + nV * 6));
    srcPos = new Float32Array(nV * 3);
    for (let k = 0; k < hf.length; k++) srcPos[k] = HALF_LUT[hf[k]];
  } else if (ver === 4) {
    const bf = new Uint16Array(buf.slice(posOff, posOff + nV * 6));
    const w = new Uint32Array(nV * 3);
    for (let k = 0; k < bf.length; k++) w[k] = bf[k] << 16;
    srcPos = new Float32Array(w.buffer);
  } else {
    srcPos = new Float32Array(buf.slice(posOff, posOff + nV * 12));
  }
  const helix = new Uint8Array(buf.slice(helixOff, helixOff + 6 * nV));
  const rowArr = new Uint32Array(buf.slice(rowOff, rowOff + 4 * nV));

  const positions = new Float32Array(nV * 3);
  const colors = new Uint8Array(nV * 3);
  const nrmOct = new Int8Array(nV * 2);    // octahedral i8 of the decoded helix normal
  const layer = new Float32Array(nV);
  for (let i = 0; i < nV; i++) {
    positions[i * 3] = -srcPos[i * 3];
    positions[i * 3 + 1] = srcPos[i * 3 + 2];
    positions[i * 3 + 2] = srcPos[i * 3 + 1];
    const r = rowArr[i], li = cLayer[r] || 8;
    const rgb = conceptColor(li, r);
    colors[i * 3] = rgb[0]; colors[i * 3 + 1] = rgb[1]; colors[i * 3 + 2] = rgb[2];
    // canonical Signed360 (6-byte) decode of this vertex's helix normal → octahedral i8.
    const oct = octEncode(decodeSigned360Display(helix, i * 6));
    nrmOct[i * 2] = oct[0]; nrmOct[i * 2 + 1] = oct[1];
    layer[i] = li;
  }
  const raw = new Uint32Array(buf.slice(idxOff, idxOff + 12 * nT));
  const index = new Uint32Array(raw);     // straight copy (no opaque/transparent split)
  return { nVerts: nV, nTris: nT, positions, index, colors, nrmOct, layer, concepts: nC };
}

const VERT = `
precision highp float;
attribute vec3 aColor; attribute vec2 aOct; attribute float aLayer;
varying vec3 vNormal; varying vec3 vColor; varying float vLayer;
void main(){
  vColor = aColor; vLayer = aLayer;
  // octahedral decode of the normalized-i8 helix normal — GPU-native, no texture.
  vec2 f = aOct;                                   // [-1,1] from the normalized i8 attr
  vec3 n = vec3(f.x, f.y, 1.0 - abs(f.x) - abs(f.y));
  float t = max(-n.z, 0.0);
  n.x += n.x >= 0.0 ? -t : t;
  n.y += n.y >= 0.0 ? -t : t;
  vNormal = normalMatrix * normalize(n);
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}`;
const FRAG = `
precision mediump float;
uniform float uEnabled[9];
varying vec3 vNormal; varying vec3 vColor; varying float vLayer;
void main(){
  int li = int(vLayer + 0.5);
  if(li < 1 || li > 8 || uEnabled[li] < 0.5) discard;
  vec3 n = normalize(vNormal); if(!gl_FrontFacing) n = -n;
  const vec3 L = vec3(-0.401, 0.783, 0.476);
  float ndl = max(dot(n, L), 0.0);
  float shade = min(0.34 + 0.20*(n.y*0.5+0.5) + 0.12*(-n.x*0.5+0.5) + 0.92*ndl, 1.3);
  gl_FragColor = vec4(vColor * shade, 1.0);
}`;

function mount(container: HTMLDivElement, d: Decoded, enabled: Float32Array): () => void {
  let w = container.clientWidth || window.innerWidth, h = container.clientHeight || window.innerHeight;
  const scene = new THREE.Scene(); scene.background = new THREE.Color(PAGE_BG);
  const camera = new THREE.PerspectiveCamera(45, w / h, 0.01, 100); camera.position.set(0, 0.05, 3.0);
  const renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setSize(w, h); renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  container.appendChild(renderer.domElement);

  const geom = new THREE.BufferGeometry();
  geom.setAttribute('position', new THREE.BufferAttribute(d.positions, 3));
  geom.setAttribute('aColor', new THREE.Uint8BufferAttribute(d.colors, 3, true));
  geom.setAttribute('aOct', new THREE.Int8BufferAttribute(d.nrmOct, 2, true)); // normalized i8 → [-1,1]
  geom.setAttribute('aLayer', new THREE.BufferAttribute(d.layer, 1));
  geom.setIndex(new THREE.BufferAttribute(d.index, 1));

  const uniforms = { uEnabled: { value: enabled } };
  const mat = new THREE.ShaderMaterial({ uniforms, vertexShader: VERT, fragmentShader: FRAG, side: THREE.DoubleSide });
  const mesh = new THREE.Mesh(geom, mat); scene.add(mesh);

  // minimal orbit: drag = rotate, wheel = dolly.
  let az = 0, el = 0.1, dist = 3.0, dragging = false, px = 0, py = 0;
  const target = new THREE.Vector3(0, 0, 0);
  const onDown = (e: PointerEvent) => { dragging = true; px = e.clientX; py = e.clientY; };
  const onUp = () => { dragging = false; };
  const onMove = (e: PointerEvent) => {
    if (!dragging) return;
    az -= (e.clientX - px) * 0.005; el = Math.max(-1.5, Math.min(1.5, el + (e.clientY - py) * 0.005));
    px = e.clientX; py = e.clientY;
  };
  const onWheel = (e: WheelEvent) => { e.preventDefault(); dist = Math.max(0.3, Math.min(8, dist * (1 + Math.sign(e.deltaY) * 0.1))); };
  const el2 = renderer.domElement;
  el2.addEventListener('pointerdown', onDown); window.addEventListener('pointerup', onUp);
  window.addEventListener('pointermove', onMove); el2.addEventListener('wheel', onWheel, { passive: false });

  let raf = 0;
  const onResize = () => {
    w = container.clientWidth || window.innerWidth; h = container.clientHeight || window.innerHeight;
    camera.aspect = w / h; camera.updateProjectionMatrix(); renderer.setSize(w, h);
  };
  window.addEventListener('resize', onResize);
  const tick = () => {
    uniforms.uEnabled.value = enabled;
    camera.position.set(target.x + dist * Math.cos(el) * Math.sin(az), target.y + dist * Math.sin(el), target.z + dist * Math.cos(el) * Math.cos(az));
    camera.lookAt(target);
    renderer.render(scene, camera);
    raf = requestAnimationFrame(tick);
  };
  tick();
  return () => {
    cancelAnimationFrame(raf);
    el2.removeEventListener('pointerdown', onDown); window.removeEventListener('pointerup', onUp);
    window.removeEventListener('pointermove', onMove); el2.removeEventListener('wheel', onWheel);
    window.removeEventListener('resize', onResize);
    geom.dispose(); mat.dispose(); renderer.dispose();
    if (el2.parentElement === container) container.removeChild(el2);
  };
}

const inflate = async (r: Response): Promise<ArrayBuffer> => {
  const buf = await r.arrayBuffer();
  const u8 = new Uint8Array(buf);
  if (!(u8.length > 1 && u8[0] === 0x1f && u8[1] === 0x8b)) return buf;
  if (typeof DecompressionStream === 'undefined') throw new Error('gzip but no DecompressionStream');
  return new Response(new Blob([buf]).stream().pipeThrough(new DecompressionStream('gzip'))).arrayBuffer();
};

// CANONICAL-ONLY: read the stamped helix bake (Signed360 normals) named by the manifest.
// We deliberately do NOT fall back to the shared body.soa.gz — that artifact carries the
// OLD helix_orient codec (a different, place-blind encoding), and reading its bytes as
// Signed360 would render garbage. Until a canonical bake is published, /helix says so.
async function fetchSoa(): Promise<ArrayBuffer> {
  const man = await fetch('/body.manifest.json').then((r) => (r.ok ? r.json() : null)).catch(() => null);
  const stamped: string | undefined = man?.helix_latest;
  if (!stamped) {
    throw new Error('no canonical helix bake yet — set helix_latest in /body.manifest.json (soabake → helix::encode_signed)');
  }
  const s = await fetch(`/${stamped}`).catch(() => null);
  if (s && s.ok) return inflate(s);
  const rel = await fetch(`${REL}/${stamped}`);
  if (!rel.ok) throw new Error(`HTTP ${rel.status} fetching ${stamped}`);
  return inflate(rel);
}

export default function BodyHelix() {
  const ref = useRef<HTMLDivElement>(null);
  const [d, setD] = useState<Decoded | null>(null);
  const [error, setError] = useState('');
  const [on, setOn] = useState<Record<number, boolean>>({ 1: false, 2: false, 3: true, 4: true, 5: true, 6: true, 7: true, 8: true });
  const enabledRef = useRef(new Float32Array([0, 0, 1, 1, 1, 1, 1, 1, 1]));

  useEffect(() => {
    let cancelled = false;
    fetchSoa().then((b) => decode(b)).then((x) => { if (!cancelled) setD(x); }).catch((e) => { if (!cancelled) setError(String(e)); });
    return () => { cancelled = true; };
  }, []);
  useEffect(() => {
    const e = new Float32Array(9);
    for (let i = 1; i <= 8; i++) e[i] = on[i] ? 1 : 0;
    enabledRef.current = e;
  }, [on]);
  useEffect(() => { const c = ref.current; if (!c || !d) return; return mount(c, d, enabledRef.current); }, [d]);

  const btn = (active: boolean): React.CSSProperties => ({
    padding: '5px 10px', borderRadius: 6, cursor: 'pointer', border: '1px solid #2a3242',
    background: active ? '#1c2738' : '#0e1219', color: active ? '#cdd9e5' : '#6b7686', font: '12px ui-monospace, monospace',
  });

  return (
    <div style={{ position: 'fixed', inset: 0, background: `#${PAGE_BG.toString(16).padStart(6, '0')}` }}>
      <div ref={ref} style={{ position: 'absolute', inset: 0 }} />
      <div style={{ position: 'absolute', top: 12, left: 16, color: '#cdd9e5', font: '13px ui-monospace, monospace' }}>
        <div style={{ color: '#fff', fontSize: 15 }}>/helix — surfel-normal viewer (experimental)</div>
        <div style={{ opacity: 0.65, marginTop: 2, maxWidth: 440 }}>
          {error ? <span style={{ color: '#e06c6c' }}>{error}</span>
            : d ? `${d.nVerts.toLocaleString()} verts · ${d.concepts.toLocaleString()} concepts — shaded from the canonical helix::Signed360 normal (place-coupled to HHTL) decoded once → octahedral i8, GPU-native normalized fetch.`
              : 'loading canonical helix bake (Signed360 normals)…'}
        </div>
      </div>
      {d && (
        <div style={{ position: 'absolute', top: 12, right: 16, display: 'flex', gap: 6, flexWrap: 'wrap', maxWidth: 360, justifyContent: 'flex-end' }}>
          {LAYERS.map((l) => (
            <button key={l.id} style={btn(on[l.id])} onClick={() => setOn((p) => ({ ...p, [l.id]: !p[l.id] }))}>
              <span style={{ display: 'inline-block', width: 8, height: 8, borderRadius: 4, background: l.color, marginRight: 5, verticalAlign: 'middle' }} />{l.name}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
