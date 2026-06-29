// /helix — EXPERIMENTAL viewer. Parallel to /body (BodyV3); shares NOTHING with it so the
// working /body can never break. Shades from the per-vertex helix NORMAL.
//
// The normal is the canonical lance-graph::helix::Signed360 (6 bytes, place-coupled to the
// HHTL address): rim endpoint pair (Fisher-Z radial) + signed polar lift + golden azimuth.
// At LOAD (once) we invert the Fisher-Z rim → r=sinθ and bake each vertex to a normalized
// int8 NORMAL — the cheapest possible carrier. Per frame the GPU just normalize()s it and
// GOURAUD-shades per vertex; the fragment shader is trivial. That is the lever against the
// 12 s/frame cost: the quality is carried by per-vertex shading + interpolation, not by
// expensive per-fragment lighting. REQUIRES the canonical helix bake (helixbake →
// helix::encode_signed, BSO2 ver 6 + HXFL floor trailer); the old helix_orient artifact is
// a different codec and is NOT read here.
//
// Reads the stamped artifact named by `/body.manifest.json` (`helix_latest`), local first
// then the GitHub release — so a new bake is swapped in by bumping the manifest, never by
// deleting the working one.
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

// ── canonical lance-graph::helix::Signed360 (6 bytes, full sphere) ──
// Wire (LE): [rim.start, rim.end, rim.floor_version, polar, azimuth_lo, azimuth_hi].
//  rim.end → the Fisher-Z RADIAL: r = sinθ, quantised as arctanh(r) into the 256-palette
//            (densest at the equator — Δθ = cosθ·Δz → 0 at θ=90°). This is the STRENGTH the
//            place-blind transcoder used to zero; we decode it. palette256 = the angle.
//  polar   → hemisphere SIGN (partition) + a coarse |y|, used only on the r→1 saturation cliff.
//  azimuth → φ = az_u16 / 65536 · 2π.
// We decode the normal ONCE at load into a normalized int8 attribute (the rim inversion's
// only atanh/tanh runs 256× building the r-LUT — never per vertex, never per frame). The
// vertex itself is then the cheapest possible carrier: a 3-byte normal the GPU reads with
// one normalize(). Quality is carried by GOURAUD shading (lighting per-vertex, colour
// interpolated) — at 6.8 M sub-pixel tris that is visually identical to per-fragment
// lighting but leaves the fragment shader trivial. The HXFL trailer carries the exact
// RollingFloor (lo,hi) the bake used so this dequantiser matches the encoder.
const TAU = Math.PI * 2;
const STRIDE = 4, GAMMA = 0.5772156649015329, LN17 = 2.833213344056216;
const atanh = (s: number) => 0.5 * Math.log((1 + s) / (1 - s));
// aligned(r) = arctanh(r)·STRIDE + γ·(r² − ln17)  — helix::ResidueEncoder::aligned_for_residue
// with the rank u = r². Monotone in r → invert by bisection. r = sinθ.
function rFromAligned(aligned: number): number {
  let lo = 0, hi = 1 - 1e-9;
  for (let it = 0; it < 40; it++) {
    const m = 0.5 * (lo + hi);
    if (atanh(m) * STRIDE + GAMMA * (m * m - LN17) < aligned) lo = m; else hi = m;
  }
  return 0.5 * (lo + hi);
}
// 256-entry r-LUT from the bake's RollingFloor (lo,hi): bucket_center(e) → aligned → r=sinθ.
function buildRLut(flo: number, fhi: number): Float32Array {
  const t = new Float32Array(256);
  for (let e = 0; e < 256; e++) t[e] = rFromAligned(flo + ((e + 0.5) / 256) * (fhi - flo));
  return t;
}

interface Decoded {
  nVerts: number; nTris: number;
  positions: Float32Array; index: Uint32Array;
  colors: Uint8Array; normals: Int8Array; layer: Float32Array;
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

  // HXFL trailer (last 12 B): the RollingFloor (lo,hi) the bake used → the rim dequantiser.
  let flo = -2.2567945, fhi = 11.535854;   // fallback = the 2026-06-29 bake's floor
  if (buf.byteLength >= 12) {
    const t0 = buf.byteLength - 12;
    const tag = String.fromCharCode(dv.getUint8(t0), dv.getUint8(t0 + 1), dv.getUint8(t0 + 2), dv.getUint8(t0 + 3));
    if (tag === 'HXFL') { flo = dv.getFloat32(t0 + 4, true); fhi = dv.getFloat32(t0 + 8, true); }
  }
  const rLut = buildRLut(flo, fhi);

  const positions = new Float32Array(nV * 3);
  const colors = new Uint8Array(nV * 3);
  const normals = new Int8Array(nV * 3);   // rim-decoded unit normal (display frame), cheap i8
  const layer = new Float32Array(nV);
  for (let i = 0; i < nV; i++) {
    positions[i * 3] = -srcPos[i * 3];
    positions[i * 3 + 1] = srcPos[i * 3 + 2];
    positions[i * 3 + 2] = srcPos[i * 3 + 1];
    const r0 = rowArr[i], li = cLayer[r0] || 8;
    const rgb = conceptColor(li, r0);
    colors[i * 3] = rgb[0]; colors[i * 3 + 1] = rgb[1]; colors[i * 3 + 2] = rgb[2];
    // Signed360 → unit normal: r=sinθ from the Fisher-Z RIM (its strength; saturated cliff
    // falls back to the polar partition), hemisphere sign from polar, φ from azimuth. Same
    // display remap as the position (-X, Z, yw). One-time at load; never per frame.
    const end = helix[i * 6 + 1], polar = helix[i * 6 + 3];
    const az16 = helix[i * 6 + 4] | (helix[i * 6 + 5] << 8);
    const sgn = polar >= 128 ? 1 : -1;
    const yp = polar >= 128 ? (polar - 128) / 127 : -(127 - polar) / 127;
    const rr = end >= 255 ? Math.sqrt(Math.max(0, 1 - yp * yp)) : rLut[end];
    const yw = sgn * Math.sqrt(Math.max(0, 1 - rr * rr));
    const az = (az16 / 65536) * TAU;
    normals[i * 3] = Math.max(-127, Math.min(127, Math.round(-rr * Math.sin(az) * 127)));
    normals[i * 3 + 1] = Math.max(-127, Math.min(127, Math.round(rr * Math.cos(az) * 127)));
    normals[i * 3 + 2] = Math.max(-127, Math.min(127, Math.round(yw * 127)));
    layer[i] = li;
  }
  // ── per-concept "maximum diameter" clamp (parity with /body's vessel sizing) ──
  // Decimation can orphan a few triangles far outside a concept's real extent — the
  // classic tell is an aorta splat that lands under the soles. /body hides such bulk
  // behind its translucent vessel pass; /helix draws everything opaque, so it shows.
  // Robust per concept: component-median centre + p95 radius × margin; any triangle
  // touching an out-of-bounds vertex is dropped. Adapts to each concept's true size.
  const byC: number[][] = Array.from({ length: nC }, () => []);
  for (let i = 0; i < nV; i++) { const c = rowArr[i]; if (c < nC) byC[c].push(i); }
  const median = (a: number[]) => { const s = a.slice().sort((x, y) => x - y); return s[s.length >> 1]; };
  const outlier = new Uint8Array(nV);
  let nOut = 0, worst = 0;
  for (let c = 0; c < nC; c++) {
    const vs = byC[c];
    if (vs.length < 8) continue;
    const cx = median(vs.map((i) => positions[i * 3]));
    const cy = median(vs.map((i) => positions[i * 3 + 1]));
    const cz = median(vs.map((i) => positions[i * 3 + 2]));
    const dist = vs.map((i) => Math.hypot(positions[i * 3] - cx, positions[i * 3 + 1] - cy, positions[i * 3 + 2] - cz));
    const ds = dist.slice().sort((a, b) => a - b);
    const p95 = ds[Math.min(ds.length - 1, Math.floor(ds.length * 0.95))];
    const thr = Math.max(p95 * 1.8, 1e-3);   // generous margin → only true far strays drop
    for (let k = 0; k < vs.length; k++) {
      if (dist[k] > thr) { outlier[vs[k]] = 1; nOut++; worst = Math.max(worst, dist[k] / Math.max(p95, 1e-4)); }
    }
  }
  // index: drop triangles touching an out-of-bounds vertex
  const raw = new Uint32Array(buf.slice(idxOff, idxOff + 12 * nT));
  const kept = new Uint32Array(raw.length);
  let w = 0;
  for (let t = 0; t < nT; t++) {
    const a = raw[t * 3], b = raw[t * 3 + 1], cc = raw[t * 3 + 2];
    if (outlier[a] || outlier[b] || outlier[cc]) continue;
    kept[w++] = a; kept[w++] = b; kept[w++] = cc;
  }
  const index = kept.slice(0, w);
  if (nOut) console.log(`/helix max-diameter clamp: ${nOut} stray verts (worst ${worst.toFixed(1)}× p95), dropped ${(nT - w / 3).toLocaleString()} tris`);
  return { nVerts: nV, nTris: w / 3, positions, index, colors, normals, layer, concepts: nC };
}

const VERT = `
precision highp float;
attribute vec3 aColor; attribute vec3 aNormal; attribute float aLayer;
varying vec3 vColor; varying float vLayer;
void main(){
  vLayer = aLayer;
  // GOURAUD: shade per-vertex from the cheap rim normal, interpolate the COLOUR across the
  // face. At 6.8 M sub-pixel tris this matches per-fragment lighting visually but leaves the
  // fragment shader trivial — the lever that removes the 12 s/frame fragment cost. The two-
  // sided ambient (n.y term + floor) keeps back faces lit without a per-fragment flip.
  vec3 n = normalize(normalMatrix * aNormal);
  const vec3 L = vec3(-0.401, 0.783, 0.476);
  float ndl = max(abs(dot(n, L)), 0.0);
  float shade = min(0.34 + 0.20*(abs(n.y)*0.5+0.5) + 0.12*(-n.x*0.5+0.5) + 0.92*ndl, 1.3);
  vColor = aColor * shade;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}`;
const FRAG = `
precision mediump float;
uniform float uEnabled[9];
varying vec3 vColor; varying float vLayer;
void main(){
  int li = int(vLayer + 0.5);
  if(li < 1 || li > 8 || uEnabled[li] < 0.5) discard;
  gl_FragColor = vec4(vColor, 1.0);   // pre-shaded (Gouraud) — no per-fragment lighting.
}`;

function mount(container: HTMLDivElement, d: Decoded, enabled: Float32Array, dirty: { current: boolean }): () => void {
  let w = container.clientWidth || window.innerWidth, h = container.clientHeight || window.innerHeight;
  const scene = new THREE.Scene(); scene.background = new THREE.Color(PAGE_BG);
  const camera = new THREE.PerspectiveCamera(45, w / h, 0.01, 100); camera.position.set(0, 0.05, 3.0);
  const renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setSize(w, h); renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  container.appendChild(renderer.domElement);

  const geom = new THREE.BufferGeometry();
  geom.setAttribute('position', new THREE.BufferAttribute(d.positions, 3));
  geom.setAttribute('aColor', new THREE.Uint8BufferAttribute(d.colors, 3, true));
  geom.setAttribute('aNormal', new THREE.Int8BufferAttribute(d.normals, 3, true)); // rim normal, normalized i8
  geom.setAttribute('aLayer', new THREE.BufferAttribute(d.layer, 1));
  geom.setIndex(new THREE.BufferAttribute(d.index, 1));

  const uniforms = { uEnabled: { value: enabled } };
  const mat = new THREE.ShaderMaterial({ uniforms, vertexShader: VERT, fragmentShader: FRAG, side: THREE.DoubleSide });
  const mesh = new THREE.Mesh(geom, mat); scene.add(mesh);

  // minimal orbit: drag = rotate, wheel = dolly.
  let az = 0, el = 0.1, dist = 3.0, dragging = false, px = 0, py = 0;
  const target = new THREE.Vector3(0, 0, 0);
  const onDown = (e: PointerEvent) => { dragging = true; px = e.clientX; py = e.clientY; dirty.current = true; };
  const onUp = () => { dragging = false; dirty.current = true; };
  const onMove = (e: PointerEvent) => {
    if (!dragging) return;
    az -= (e.clientX - px) * 0.005; el = Math.max(-1.5, Math.min(1.5, el + (e.clientY - py) * 0.005));
    px = e.clientX; py = e.clientY; dirty.current = true;
  };
  const onWheel = (e: WheelEvent) => { e.preventDefault(); dist = Math.max(0.3, Math.min(8, dist * (1 + Math.sign(e.deltaY) * 0.1))); dirty.current = true; };
  const el2 = renderer.domElement;
  el2.addEventListener('pointerdown', onDown); window.addEventListener('pointerup', onUp);
  window.addEventListener('pointermove', onMove); el2.addEventListener('wheel', onWheel, { passive: false });

  let raf = 0, ema = 16.6, last = performance.now();
  const onResize = () => {
    w = container.clientWidth || window.innerWidth; h = container.clientHeight || window.innerHeight;
    camera.aspect = w / h; camera.updateProjectionMatrix(); renderer.setSize(w, h); dirty.current = true;
  };
  window.addEventListener('resize', onResize);
  const tick = () => {
    raf = requestAnimationFrame(tick);
    // render ON DEMAND: a static body (no drag/zoom/toggle) costs nothing — 6.8 M tris are
    // only redrawn when something actually changes, which is what makes idle + heat sane.
    if (!dirty.current && !dragging) { last = performance.now(); return; }
    const now = performance.now(); ema = ema * 0.9 + (now - last) * 0.1; last = now;
    // adaptive DPR: drop to 1× when a frame is slow (>~30 fps budget). On a retina phone
    // this is the single biggest lever — quarters/ninths the fragment load while dragging.
    const pr = ema > 33 ? 1 : Math.min(window.devicePixelRatio, 2);
    if (renderer.getPixelRatio() !== pr) renderer.setPixelRatio(pr);
    uniforms.uEnabled.value = enabled;
    camera.position.set(target.x + dist * Math.cos(el) * Math.sin(az), target.y + dist * Math.sin(el), target.z + dist * Math.cos(el) * Math.cos(az));
    camera.lookAt(target);
    renderer.render(scene, camera);
    dirty.current = false;
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
  const dirtyRef = useRef(true);   // request a redraw (the render loop is on-demand)

  useEffect(() => {
    let cancelled = false;
    fetchSoa().then((b) => decode(b)).then((x) => { if (!cancelled) setD(x); }).catch((e) => { if (!cancelled) setError(String(e)); });
    return () => { cancelled = true; };
  }, []);
  useEffect(() => {
    // Mutate IN PLACE — mount captured THIS array; reassigning a new one leaves the
    // renderer reading the stale array (the dead-toggles bug). Then request a redraw.
    for (let i = 1; i <= 8; i++) enabledRef.current[i] = on[i] ? 1 : 0;
    dirtyRef.current = true;
  }, [on]);
  useEffect(() => { const c = ref.current; if (!c || !d) return; return mount(c, d, enabledRef.current, dirtyRef); }, [d]);

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
            : d ? `${d.nVerts.toLocaleString()} verts · ${d.concepts.toLocaleString()} concepts — canonical helix::Signed360 normals: Fisher-Z rim → r=sinθ decoded once into a normalized int8 normal; Gouraud shading (per-vertex), trivial fragment shader.`
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
