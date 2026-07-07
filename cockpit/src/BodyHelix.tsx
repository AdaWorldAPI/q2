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

// The scene name implied by the PATH alone (no `?scene=` override). This is the SINGLE
// source of truth for path→scene mapping — fetchSoa() (which bake to load) and isGeoScene
// (whether to disable server LOD + light the geo beautification) BOTH read it, so they can
// never disagree. `?scene=<name>` still overrides in the callers. /geo → the OSM bake,
// /ice → the Iceland DEM bake; any other path → null (the anatomy body, helix_latest).
function pathScene(): string | null {
  const p = window.location.pathname;
  if (p === '/geo') return 'osm';
  if (p === '/ice') return 'iceland';
  return null;
}

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
  colors: Uint8Array; normals: Int8Array; layer: Float32Array; vrow: Uint32Array;
  concepts: number; conceptList: ConceptMeta[];
}
interface ConceptMeta { row: number; name: string; layer: number; cx: number; cy: number; cz: number; }

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
  const labelOff = o; o += 4 * nC;    // label idx (u32 → name in labels_json)
  const cenOff = o; o += 12 * nC;     // centroid 3f
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

  // per-concept metadata for the browser: name (label→labels_json), layer, display centroid.
  let to = idxOff + 12 * nT;
  const labLen = dv.getUint32(to, true); to += 4;
  let names: string[] = [];
  try { const lj = JSON.parse(new TextDecoder().decode(new Uint8Array(buf.slice(to, to + labLen)))); names = lj.names ?? lj; } catch { /* names optional */ }
  const labelIdx = new Uint32Array(buf.slice(labelOff, labelOff + 4 * nC));
  const cen = new Float32Array(buf.slice(cenOff, cenOff + 12 * nC));
  const conceptList: ConceptMeta[] = [];
  for (let c = 0; c < nC; c++) {
    conceptList.push({ row: c, name: names[labelIdx[c]] ?? `concept ${c}`, layer: cLayer[c] || 8,
      cx: -cen[c * 3], cy: cen[c * 3 + 2], cz: cen[c * 3 + 1] });   // source → display (-x,z,y)
  }
  return { nVerts: nV, nTris: w / 3, positions, index, colors, normals, layer, vrow: rowArr, concepts: nC, conceptList };
}

const VERT = `
precision highp float;
attribute vec3 aColor; attribute vec3 aNormal;
uniform float uGeo;    // 1 = geo scene → height-profile terrain palette · 0 = anatomy → aColor (byte-identical)
uniform float uYMin;   // decoded height range (display.y), measured once at load from the position buffer
uniform float uYMax;
uniform float uExag;   // geo relief exaggeration: the Iceland bake is true-scale (span ~0.0074 in the
                       // [-1,1] frame), so raise the geometry to read as terrain. 1 for anatomy (untouched).
uniform float uTime;   // retained-but-0: the Kurvenlineal residue is baked into the mesh, not animated.
uniform float uRuler;  // retained-but-0: no shader ruler (the golden-spiral residue is baked in).
varying vec3 vColor;
// THE KURVENLINEAL is now baked into the mesh, not approximated here. The real
// helix::CurveRuler golden-spiral residue (stride-4-over-17) is applied at BAKE time in
// geo/src/bin/iceland_dem.rs (::ruler_phase) as per-vertex surface displacement + recomputed
// normals — so the residue is carried by the decoded position + Signed360 normal the same way
// the anatomy body carries it. The vertex shader below therefore only lifts the terrain by uExag
// and Gouraud-shades; the earlier GLSL float-hash approximation of the ruler has been removed.
// Height-profile terrain palette for the geo bakes (Iceland DEM, OSM). Elevation is display.y.
// In the Iceland bake it is TRUE-SCALE (not vertically exaggerated) → a tiny span (~[0, 0.0074])
// with ~39% ocean at EXACTLY 0 and a heavily-quantized lowland plateau holding ~58% of verts,
// then a thin highland tail to the peaks/glaciers. Hence we normalize by the ACTUAL measured
// [uYMin,uYMax] (never the [-1,1] convention — that would flatten everything to water) and apply
// a sqrt curve to spread the skewed tail. Colour is driven by HEIGHT ONLY: the decoded Signed360
// normal is BIMODAL for the geo bake (77% |n.y|≈0 / 23% ≈1), so it is NOT a usable continuous
// slope — classifying rock/scree/volcano from it would give a binary patchwork. The shade
// lighting term below still uses the normal for form. HONEST NOTE: true lava-field / glacier /
// volcano feature classification needs a DEM re-bake carrying feature layers; the warm "volcano"
// band here is a height-derived accent approximation, deliberately subtle.
vec3 terrainColor(float h){
  float hl = clamp((h - uYMin) / max(uYMax - uYMin, 1e-6), 0.0, 1.0);  // linear normalized height
  float hs = sqrt(hl);                                                  // spread the quantized tail
  vec3 ocean = vec3(0.05, 0.15, 0.30);
  vec3 coast = vec3(0.28, 0.42, 0.24);   // lowest green coastal fringe
  vec3 moss  = vec3(0.34, 0.36, 0.22);   // khaki moss/tundra — the dominant quantized lowland
  vec3 rock  = vec3(0.44, 0.38, 0.30);   // brown highland rock
  vec3 scree = vec3(0.56, 0.55, 0.55);   // grey scree
  vec3 ice   = vec3(0.90, 0.93, 0.98);   // snow / glacier cap
  vec3 land = coast;
  land = mix(land, moss,  smoothstep(0.18, 0.30, hs));
  land = mix(land, rock,  smoothstep(0.34, 0.48, hs));
  land = mix(land, scree, smoothstep(0.50, 0.64, hs));
  land = mix(land, ice,   smoothstep(0.66, 0.82, hs));
  vec3 lava = vec3(0.42, 0.16, 0.10);    // warm volcanic-highland accent (approximation — see note)
  float volc = smoothstep(0.42, 0.52, hs) * (1.0 - smoothstep(0.60, 0.72, hs));
  land = mix(land, lava, volc * 0.28);
  float water = 1.0 - smoothstep(0.002, 0.010, hl);   // ocean is EXACTLY 0; first land ≈ 0.0155 normalized
  return mix(land, ocean, water);
}
void main(){
  // GOURAUD: shade per-vertex from the cheap rim normal, interpolate the COLOUR across the
  // face. At 6.8 M sub-pixel tris this matches per-fragment lighting visually but leaves the
  // fragment shader trivial. Two-sided ambient keeps any back faces lit without a flip.
  vec3 n = normalize(normalMatrix * aNormal);
  const vec3 L = vec3(-0.401, 0.783, 0.476);
  float ndl = max(abs(dot(n, L)), 0.0);
  float shade = min(0.34 + 0.20*(abs(n.y)*0.5+0.5) + 0.12*(-n.x*0.5+0.5) + 0.92*ndl, 1.3);
  // uGeo is a dynamically-uniform branch (a uniform, not a varying): coherent, no divergence.
  // uGeo == 0 -> EXACTLY the pre-existing aColor*shade, so anatomy scenes are byte-identical.
  vColor = (uGeo > 0.5) ? terrainColor(position.y) * shade : aColor * shade;
  // Geo relief: raise the true-scale island by uExag. The Kurvenlineal golden-spiral residue is
  // already in position + normal (baked by the DEM baker), so the shader adds no ruler of its own.
  // Anatomy (uGeo==0): untouched.
  vec3 dpos = position;
  if (uGeo > 0.5) {
    dpos.y = position.y * uExag;
  }
  gl_Position = projectionMatrix * modelViewMatrix * vec4(dpos, 1.0);
}`;
const FRAG = `
precision mediump float;
uniform float uAlpha;                                // 1 = solid · <1 = x-ray (whole-body translucent)
varying vec3 vColor;
void main(){ gl_FragColor = vec4(vColor, uAlpha); }`;   // visible layers are pre-filtered into the
// draw range (NOT a discard) → early-Z survives; the GPU never touches hidden triangles.

// ── procedural sky dome (geo scenes only) ──────────────────────────────────────────────
// A large BackSide sphere with a vertical gradient (pale horizon → blue zenith), rendered
// first (renderOrder −1, depthWrite off) so terrain always draws over it. No external asset
// (CSP blocks CDNs) — the gradient is computed in the fragment shader from the view direction.
// Added to the scene ONLY for geo scenes; anatomy scenes keep the flat PAGE_BG background.
const SKY_HORIZON = new THREE.Color(0.74, 0.80, 0.85);
const SKY_ZENITH = new THREE.Color(0.24, 0.42, 0.64);
const SKY_VERT = `
precision highp float;
varying vec3 vDir;
void main(){ vDir = position; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }`;
const SKY_FRAG = `
precision mediump float;
varying vec3 vDir;
uniform vec3 uHorizon; uniform vec3 uZenith;
void main(){
  float t = clamp(normalize(vDir).y * 0.5 + 0.5, 0.0, 1.0);
  gl_FragColor = vec4(mix(uHorizon, uZenith, smoothstep(0.12, 0.92, t)), 1.0);
}`;
type Focus = { x: number; y: number; z: number; d: number };

function mount(container: HTMLDivElement, d: Decoded, enabled: Float32Array, dirty: { current: boolean }, focus: { current: Focus | null }, xray: { current: boolean }, lod: { current: boolean }): () => void {
  let w = container.clientWidth || window.innerWidth, h = container.clientHeight || window.innerHeight;
  const scene = new THREE.Scene(); scene.background = new THREE.Color(PAGE_BG);
  const camera = new THREE.PerspectiveCamera(45, w / h, 0.01, 100); camera.position.set(0, 0.05, 3.0);
  const renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setSize(w, h); renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  container.appendChild(renderer.domElement);

  // Geo scene? (scene=osm, scene=iceland, /geo, /ice, …). Resolved via pathScene() so this
  // matches fetchSoa() EXACTLY (they share the one helper). Gates three things: server LOD is
  // disabled (the /api/body/lod cascade runs over the BODY's block-bounds; a geo scene's
  // concepts must render in full), the height-profile terrain palette is lit (uGeo), and the
  // sky dome is added. An empty `?scene=` resolves falsy → anatomy body → all three stay off.
  const sceneParam = new URLSearchParams(window.location.search).get('scene');
  const isGeoScene = Boolean(sceneParam ?? pathScene());
  // TERRAIN vs BUILDINGS. A terrain scene (Iceland DEM) is a dense, watertight heightfield —
  // the same kind of surface as the anatomy body, so it earns the height-recolour + relief
  // exaggeration + breathing that make the body read in 3D. A building scene (OSM/Berlin) is
  // thin extruded footprints: exaggerating those 10x and recolouring them by height turns each
  // building into a black needle (the #88 regression the operator flagged). So beautification is
  // gated on TERRAIN, never on "any geo" — buildings render plain baked colours (uGeo/uExag/uRuler
  // all neutral, exactly as anatomy), which is a solid city, not a needle field.
  const isTerrainScene = (sceneParam ?? pathScene()) === 'iceland';

  // Height range for the geo palette: measured ONCE from the decoded position buffer (display.y).
  // The Iceland bake is true-scale so the span is tiny — normalizing against [-1,1] would flatten
  // it to all-water; the actual [min,max] is what the shader needs. Only consumed when uGeo == 1.
  let yMin = Infinity, yMax = -Infinity;
  for (let i = 0; i < d.nVerts; i++) { const y = d.positions[i * 3 + 1]; if (y < yMin) yMin = y; if (y > yMax) yMax = y; }
  if (!(yMax > yMin)) { yMin = 0; yMax = 1; }   // degenerate/flat guard → avoid divide-by-zero

  const geom = new THREE.BufferGeometry();
  geom.setAttribute('position', new THREE.BufferAttribute(d.positions, 3));
  geom.setAttribute('aColor', new THREE.Uint8BufferAttribute(d.colors, 3, true));
  geom.setAttribute('aNormal', new THREE.Int8BufferAttribute(d.normals, 3, true)); // rim normal, normalized i8

  // Draw ONLY enabled layers, as GEOMETRY (rebuild the index on toggle) — never a
  // fragment discard. A discard still rasterises every triangle, then throws the pixels
  // away (kills early-Z); excluding them from the index means the GPU never touches them.
  // Default skin+muscle-off removes the body's largest surfaces — the real lever against
  // "won't rotate", with backface culling (FrontSide) halving the rest.
  const fullIdx = d.index;
  const nTriAll = fullIdx.length / 3;
  const triLayer = new Uint8Array(nTriAll);
  const triConcept = new Uint32Array(nTriAll);   // concept (row) of each triangle → server-LOD gate
  for (let t = 0; t < nTriAll; t++) { triLayer[t] = d.layer[fullIdx[t * 3]]; triConcept[t] = d.vrow[fullIdx[t * 3]]; }
  // server-LOD action per concept: 255 = show (the default until the cascade answers), 0 = the
  // HHTL depth-cascade rejected this concept as off-frustum. Folded into the index rebuild below.
  const lodAction = new Uint8Array(d.concepts).fill(255);
  const active = new Uint32Array(fullIdx.length);
  const rebuild = (): number => {
    let n = 0;
    for (let t = 0; t < nTriAll; t++) {
      if (enabled[triLayer[t]] >= 0.5 && lodAction[triConcept[t]] !== 0) { const o = t * 3; active[n++] = fullIdx[o]; active[n++] = fullIdx[o + 1]; active[n++] = fullIdx[o + 2]; }
    }
    return n;
  };
  const idxAttr = new THREE.BufferAttribute(active, 1);
  idxAttr.setUsage(THREE.DynamicDrawUsage);
  geom.setIndex(idxAttr);
  const applyIndex = () => { geom.setDrawRange(0, rebuild()); idxAttr.needsUpdate = true; };
  geom.setDrawRange(0, rebuild());

  // uGeo (height-recolour) + uExag (relief) are TERRAIN-only; buildings/anatomy keep uGeo=0 (baked
  // aColor) + uExag=1. uRuler/uTime are retained-but-0: the Kurvenlineal golden-spiral residue is
  // now baked into the mesh (geometry + normals), so the shader applies no ruler — the terrain is a
  // static surface, not a shader-animated one.
  const uniforms = { uAlpha: { value: 1 }, uGeo: { value: isTerrainScene ? 1 : 0 }, uYMin: { value: yMin }, uYMax: { value: yMax }, uExag: { value: isTerrainScene ? 15 : 1 }, uTime: { value: 0 }, uRuler: { value: 0 } };
  const mat = new THREE.ShaderMaterial({ uniforms, vertexShader: VERT, fragmentShader: FRAG, side: THREE.FrontSide });
  const mesh = new THREE.Mesh(geom, mat); scene.add(mesh);

  // Sky dome — geo scenes only (anatomy keeps the flat PAGE_BG set above, byte-identical).
  let skyGeom: THREE.SphereGeometry | null = null;
  let skyMat: THREE.ShaderMaterial | null = null;
  if (isGeoScene) {
    scene.background = SKY_HORIZON.clone();   // horizon tint behind the dome (any gap reads as sky)
    skyGeom = new THREE.SphereGeometry(40, 32, 16);   // radius 40 < camera far (100), surrounds the orbit
    skyMat = new THREE.ShaderMaterial({
      side: THREE.BackSide, depthWrite: false,
      uniforms: { uHorizon: { value: SKY_HORIZON.clone() }, uZenith: { value: SKY_ZENITH.clone() } },
      vertexShader: SKY_VERT, fragmentShader: SKY_FRAG,
    });
    const sky = new THREE.Mesh(skyGeom, skyMat);
    sky.renderOrder = -1;   // draw first → terrain (with depth) always paints over it
    scene.add(sky);
  }

  // minimal orbit: drag = rotate, wheel = dolly.
  // Geo scenes open on an AERIAL oblique view (el ~35°) so the terrain reads as a landscape under
  // the sky dome, not edge-on; anatomy keeps the near-level body orbit. Target lifts slightly so
  // the raised relief sits in frame. Drag/wheel still free-orbit from there.
  let az = 0, el = isGeoScene ? 0.62 : 0.1, dist = isGeoScene ? 2.6 : 3.0, dragging = false, px = 0, py = 0;
  const target = new THREE.Vector3(0, isGeoScene ? 0.08 : 0, 0);
  const onDown = (e: PointerEvent) => { dragging = true; px = e.clientX; py = e.clientY; focus.current = null; dirty.current = true; };
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

  // server HHTL LOD (opt-in): post the live camera to /api/body/lod; the depth-cascade returns
  // a per-concept action (0 = off-frustum reject). We fold the cull into the SAME geometry index
  // rebuild as the layer toggles — NOT a fragment discard — so early-Z survives and the GPU draws
  // strictly fewer triangles when zoomed in (the mobile lever, working WITH the database). Absent
  // endpoint (old deploy) → silently keep the full render. This is the living DB reasoning the view.
  let lodNext = 0, lodInflight = false, lodFail = false, lodDirty = false, lodWasOn = false;
  // Any geo scene (scene=osm, scene=iceland, /geo, /ice, …) shares the /api/body/lod
  // endpoint, but that cascade runs over the BODY's compile-time block-bounds — it would
  // cull the geo concepts with anatomy bounds. Server LOD is therefore disabled for EVERY
  // geo scene (full render); a geo LOD that reads the scene's own .blocks sidecar is future
  // work. The gate is `isGeoScene`, computed once near the top of mount() via the SAME
  // pathScene() helper fetchSoa() uses, so the artifact resolver and the LOD gate can never
  // disagree (an empty `?scene=` → anatomy body → helix_latest → LOD stays ON).
  const postLod = (now: number) => {
    if (isGeoScene || lodFail || lodInflight || now < lodNext) return;
    lodInflight = true; lodNext = now + 220;
    camera.updateMatrixWorld();
    const e = camera.matrixWorldInverse.elements;   // column-major → row-major view rows
    const view = [
      [e[0], e[4], e[8], e[12]], [e[1], e[5], e[9], e[13]],
      [e[2], e[6], e[10], e[14]], [e[3], e[7], e[11], e[15]],
    ];
    const fy = (h / 2) / Math.tan((camera.fov * Math.PI) / 360);
    const body = { view, fx: fy, fy, cx: w / 2, cy: h / 2, near: camera.near, far: camera.far, width: w, height: h, position: [camera.position.x, camera.position.y, camera.position.z] };
    fetch('/api/body/lod', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
      .then((j: { actions: number[]; n_concepts?: number; tally?: number[] }) => {
        const a = j.actions;
        const visible = (j.n_concepts ?? a.length) - (j.tally?.[0] ?? 0);
        const degenerate = visible <= Math.max(1, a.length * 0.02);   // cascade culled ~all ⇒ camera map suspect → show all
        for (let i = 0; i < d.concepts && i < a.length; i++) lodAction[i] = degenerate ? 255 : a[i];
        lodDirty = true; dirty.current = true;
      })
      .catch(() => { lodFail = true; })   // endpoint absent (old deploy) → keep full render
      .finally(() => { lodInflight = false; });
  };

  let raf = 0, ema = 16.6, last = performance.now(), sig = enabled.join(',');
  const onResize = () => {
    w = container.clientWidth || window.innerWidth; h = container.clientHeight || window.innerHeight;
    camera.aspect = w / h; camera.updateProjectionMatrix(); renderer.setSize(w, h); dirty.current = true;
  };
  window.addEventListener('resize', onResize);
  const tick = () => {
    raf = requestAnimationFrame(tick);
    // Every scene renders on-demand (dirty gating) — no idle redraw. The terrain's Kurvenlineal
    // residue is baked into the mesh (static surface), so there is no per-frame shader animation.
    // server-LOD lifecycle runs even on idle frames (the cascade tracks the static view too);
    // turning LOD off restores the full geometry. Both are cheap and bounded by the 220 ms poll.
    const tnow = performance.now();
    if (lod.current) { postLod(tnow); lodWasOn = true; }
    else if (lodWasOn) { lodWasOn = false; lodFail = false; lodAction.fill(255); lodDirty = true; dirty.current = true; }
    // render ON DEMAND: a static body (no drag/zoom/toggle) costs nothing — 6.8 M tris are
    // only redrawn when something actually changes, which is what makes idle + heat sane.
    if (!dirty.current && !dragging) { last = performance.now(); return; }
    const now = performance.now(); ema = ema * 0.9 + (now - last) * 0.1; last = now;
    // adaptive DPR: drop to 1× when a frame is slow (>~30 fps budget). On a retina phone
    // this is the single biggest lever — quarters/ninths the fragment load while dragging.
    const pr = ema > 33 ? 1 : Math.min(window.devicePixelRatio, 2);
    if (renderer.getPixelRatio() !== pr) renderer.setPixelRatio(pr);
    // layer toggled OR server-LOD answered → rebuild the active index (geometry exclusion, not
    // discard): one linear pass folds both the layer mask and the per-concept LOD action.
    const ns = enabled.join(',');
    if (ns !== sig || lodDirty) { sig = ns; lodDirty = false; applyIndex(); }
    // x-ray: whole-body translucency (depthWrite off; cheap, unsorted-blend is fine here)
    const wantX = xray.current;
    if (mat.transparent !== wantX) { mat.transparent = wantX; mat.depthWrite = !wantX; mat.needsUpdate = true; }
    uniforms.uAlpha.value = wantX ? 0.4 : 1.0;
    // browser pick → glide the orbit target + dolly onto the chosen concept
    if (focus.current) {
      const f = focus.current;
      target.lerp(new THREE.Vector3(f.x, f.y, f.z), 0.12);
      dist += (f.d - dist) * 0.12;
      if (Math.abs(dist - f.d) < 0.02) focus.current = null;
      dirty.current = true;
    }
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
    geom.dispose(); mat.dispose();
    if (skyGeom) skyGeom.dispose();
    if (skyMat) skyMat.dispose();
    renderer.dispose();
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
  // /helix → anatomy body (helix_latest). /helix?scene=<name> → that scene's
  // bake (<name>_latest, e.g. osm_latest, iceland_latest) through the SAME
  // Signed360 decoder. /geo is shorthand for scene=osm, /ice for scene=iceland
  // (via pathScene() — the SAME helper isGeoScene reads, so the bake choice and
  // the LOD/beautification gate can never disagree). The body's data slot and the
  // separate /osm slippy-map page are both untouched.
  const scene =
    new URLSearchParams(window.location.search).get('scene') ?? pathScene();
  const key = scene ? `${scene}_latest` : 'helix_latest';
  const stamped: string | undefined = man?.[key];
  if (!stamped) {
    throw new Error(`no bake for scene="${scene ?? 'body'}" — set ${key} in /body.manifest.json (soabake → helix::encode_signed; osm → geo/osm_helix)`);
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
  const [xray, setXray] = useState(false);
  const [lod, setLod] = useState(false);   // server HHTL LOD — opt-in (off = full render)
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState<Record<number, boolean>>({ 4: true });  // expanded layer groups
  const enabledRef = useRef(new Float32Array([0, 0, 1, 1, 1, 1, 1, 1, 1]));
  const dirtyRef = useRef(true);   // request a redraw (the render loop is on-demand)
  const focusRef = useRef<Focus | null>(null);
  const xrayRef = useRef(false);
  const lodRef = useRef(false);

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
  useEffect(() => { xrayRef.current = xray; dirtyRef.current = true; }, [xray]);
  useEffect(() => { lodRef.current = lod; dirtyRef.current = true; }, [lod]);
  useEffect(() => { const c = ref.current; if (!c || !d) return; return mount(c, d, enabledRef.current, dirtyRef, focusRef, xrayRef, lodRef); }, [d]);

  const focusOn = (c: ConceptMeta) => {
    focusRef.current = { x: c.cx, y: c.cy, z: c.cz, d: 0.6 };
    if (!enabledRef.current[c.layer]) setOn((p) => ({ ...p, [c.layer]: true }));  // reveal its layer
    dirtyRef.current = true;
  };

  const btn = (active: boolean): React.CSSProperties => ({
    padding: '5px 10px', borderRadius: 6, cursor: 'pointer', border: '1px solid #2a3242',
    background: active ? '#1c2738' : '#0e1219', color: active ? '#cdd9e5' : '#6b7686', font: '12px ui-monospace, monospace',
  });
  const q = query.trim().toLowerCase();
  // Geo scenes: the terrain bake stamps a single layer — present it as "terrain" and drop the
  // empty anatomy layers, so a map never reads as skin/muscle/skeleton (the layer *id* is kept, so
  // the show/hide toggle still filters the real geometry). Anatomy keeps the full LAYERS taxonomy.
  const geoScene = new URLSearchParams(window.location.search).get('scene') ?? pathScene();
  const geoUI = Boolean(geoScene);
  // Geo bakes stamp a single layer; present it with a domain-true name (a MAP must never read as
  // skin/muscle/skeleton) and drop the empty anatomy layers. Terrain vs buildings gets its own name.
  const geoLayerName = geoScene === 'iceland' ? 'terrain' : 'buildings';
  const geoLayerColor = geoScene === 'iceland' ? '#7c8f5c' : '#9aa7b4';
  const activeLayers =
    geoUI && d
      ? LAYERS.filter((l) => d.conceptList.some((c) => c.layer === l.id)).map((l) => ({ ...l, name: geoLayerName, color: geoLayerColor }))
      : LAYERS;
  const groups = activeLayers.map((l) => ({
    l, items: d ? d.conceptList.filter((c) => c.layer === l.id && (!q || c.name.toLowerCase().includes(q))) : [],
  })).filter((g) => g.items.length > 0 || !q);

  // Scene label (same resolution as fetchSoa/isGeoScene). Anatomy keeps the exact original
  // title/subtitle; geo scenes get a scene-appropriate heading so /ice doesn't read "anatomy".
  const scene = new URLSearchParams(window.location.search).get('scene') ?? pathScene();
  const title =
    scene === 'iceland' ? '/ice — Iceland height-profile terrain'
    : scene === 'osm' ? '/geo — OSM buildings'
    : scene ? `/helix?scene=${scene}`
    : '/helix — living anatomy browser';
  const subtitle = d
    ? scene
      ? `${d.nVerts.toLocaleString()} verts · ${d.concepts.toLocaleString()} structures · ${scene === 'iceland' ? 'height-profile palette + sky' : 'baked colours + sky'}`
      : `${d.nVerts.toLocaleString()} verts · ${d.concepts.toLocaleString()} structures · helix::Signed360 normals (Fisher-Z rim)`
    : 'loading canonical helix bake…';

  return (
    <div style={{ position: 'fixed', inset: 0, background: `#${PAGE_BG.toString(16).padStart(6, '0')}` }}>
      <div ref={ref} style={{ position: 'absolute', inset: 0 }} />
      <div style={{ position: 'absolute', top: 12, left: 16, color: '#cdd9e5', font: '13px ui-monospace, monospace', pointerEvents: 'none' }}>
        <div style={{ color: '#fff', fontSize: 15 }}>{title}</div>
        <div style={{ opacity: 0.6, marginTop: 2, maxWidth: 300 }}>
          {error ? <span style={{ color: '#e06c6c' }}>{error}</span> : subtitle}
        </div>
      </div>
      {d && (
        <div style={{ position: 'absolute', top: 12, right: 16, display: 'flex', gap: 6, flexWrap: 'wrap', maxWidth: 380, justifyContent: 'flex-end' }}>
          <button style={btn(xray)} onClick={() => setXray((x) => !x)} title="x-ray: make the whole body translucent so deeper structures show through">x-ray</button>
          <button style={btn(lod)} onClick={() => setLod((v) => !v)} title="LOD: the HHTL depth-cascade culls off-frustum structures as you zoom in — the living database deciding what's worth drawing">LOD {lod ? 'on' : 'off'}</button>
          {activeLayers.map((l) => (
            <button key={l.id} style={btn(on[l.id])} onClick={() => setOn((p) => ({ ...p, [l.id]: !p[l.id] }))}>
              <span style={{ display: 'inline-block', width: 8, height: 8, borderRadius: 4, background: l.color, marginRight: 5, verticalAlign: 'middle' }} />{l.name}
            </button>
          ))}
        </div>
      )}
      {d && (
        <div style={{ position: 'absolute', left: 16, top: 66, bottom: 16, width: 290, display: 'flex', flexDirection: 'column', background: 'rgba(11,15,22,0.92)', border: '1px solid #1c2530', borderRadius: 10, overflow: 'hidden' }}>
          <input value={query} onChange={(e) => setQuery(e.target.value)} placeholder={`search ${d.concepts.toLocaleString()} structures…`}
            style={{ margin: 10, padding: '8px 10px', borderRadius: 7, border: '1px solid #243244', background: '#0e1219', color: '#cdd9e5', font: '13px ui-monospace, monospace', outline: 'none' }} />
          <div style={{ overflowY: 'auto', padding: '0 6px 8px' }}>
            {groups.map(({ l, items }) => {
              const expanded = !!open[l.id] || !!q;
              return (
                <div key={l.id}>
                  <div onClick={() => setOpen((p) => ({ ...p, [l.id]: !expanded }))}
                    style={{ display: 'flex', alignItems: 'center', gap: 7, padding: '7px 8px', cursor: 'pointer', color: '#cdd9e5', font: '12px ui-monospace, monospace', userSelect: 'none' }}>
                    <span style={{ width: 8, opacity: 0.7 }}>{expanded ? '▾' : '▸'}</span>
                    <span style={{ display: 'inline-block', width: 9, height: 9, borderRadius: 5, background: l.color }} />
                    <span style={{ flex: 1 }}>{l.name}</span>
                    <span style={{ opacity: 0.45 }}>{items.length}</span>
                  </div>
                  {expanded && items.slice(0, 500).map((c) => (
                    <div key={c.row} onClick={() => focusOn(c)} title={c.name}
                      style={{ padding: '4px 8px 4px 30px', cursor: 'pointer', color: '#9fb0c2', font: '12px ui-monospace, monospace', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis', borderRadius: 5 }}
                      onMouseEnter={(e) => { e.currentTarget.style.background = '#152030'; e.currentTarget.style.color = '#dce6f0'; }}
                      onMouseLeave={(e) => { e.currentTarget.style.background = 'transparent'; e.currentTarget.style.color = '#9fb0c2'; }}>
                      {c.name}
                    </div>
                  ))}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
