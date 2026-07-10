// GeoHelix — the MAP/TERRAIN fork of BodyHelix, serving /geo, /ice and
// /garmin/:location. A verbatim fork (like BodyHelix itself was forked from
// BodyV3, #64) so map-shader work — hypsometric tint, kurvenlineal brightness,
// sunset lighting, Ice/Ocean specular — can NEVER regress the working anatomy
// body (/helix stays on BodyHelix, untouched). Shares the same BSO2 decoder and
// the per-vertex helix NORMAL shading; only the geo scene paths are live here.
//
// (Original BodyHelix header retained below for the shared decode/shading detail.)
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
  if (p === '/havel') return 'garmin:havel';   // the canoe map gets a first-class URL (like /ice)
  // Mod-rewrite style: /garmin/<location> → scene "garmin:<location>". The slug is
  // resolved SERVER-side (/api/garmin/:location → manifest garmin_scenes → dist file),
  // so a new scene is a manifest entry + a bake — no client change.
  const m = p.match(/^\/garmin\/([a-z0-9-]+)$/);
  if (m) return `garmin:${m[1]}`;
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
  /// ver-8 radix grid (vs an explicit mesh): the wire is a W×H heightfield, so it
  /// can be re-decoded at a coarser stride — the client-side terrain LOD.
  isGrid?: boolean;
  stride?: number;   // grid LOD stride actually decoded at (1 = full res)
  skin?: boolean;    // ver-9: vertex colours are the raw satellite photo (Diaprojektor skin)
}
interface ConceptMeta { row: number; name: string; layer: number; cx: number; cy: number; cz: number; }
/// The draped feature network (DRP1): segment-paired vertices in the terrain's
/// DISPLAY frame PRE-exaggeration (mount scales `y` by the same `uExag` the grid
/// shader applies) + a per-vertex KIND colour. Rendered as a `LineSegments` overlay.
/// Fetched INDEPENDENTLY of the terrain (never blocks the terrain render) and
/// attached to the live scene via a ref when it arrives.
interface DrapeData { positions: Float32Array; colors: Uint8Array; segCount: number; kindCount: number; }

// ── ver-8 radix-grid decode ─────────────────────────────────────────────────
// The wire stores ONLY height (F16) + kind (u8 → header palette). Everything
// else is DETERMINISTIC from the address and reconstructed here: position by
// radix (i → (row, col) → x0 + col·dx, zrow[row]), the triangle index by the
// grid loop, normals by one gradient pass (true-scale, so display slope =
// real-world slope), and colour by palette[kind] × the CurveRuler golden-spiral
// residue (stride-4-over-17, bit-exact 64-bit integer — "phase is convention,
// not data"). 710 MB (ver-7) → ~50 MB raw for the same 16.5 M-vert Iceland.
const MIX_X = 0x9E3779B97F4A7C15n, MIX_Y = 0xC2B2AE3D27D4EB4Fn, MIX_Z = 0x165667B19E3779F9n;
const U64 = (v: bigint) => BigInt.asUintN(64, v);
/// mix(cell) % 17 — the only reading of the mix the ruler needs (start / k).
function mix17(cx: number, cy: number, cz: number): number {
  const m = U64(
    U64(BigInt.asUintN(64, BigInt(cx)) * MIX_X)
    ^ U64(BigInt.asUintN(64, BigInt(cy)) * MIX_Y)
    ^ U64(BigInt.asUintN(64, BigInt(cz)) * MIX_Z));
  return Number(m % 17n);
}
function decodeGrid(buf: ArrayBuffer, stride = 1): Decoded {
  const dv = new DataView(buf);
  const ver = dv.getUint16(4, true);                 // 8 = palette grid; 9 = + per-vertex satellite skin
  const nC = dv.getUint32(6, true), Wf = dv.getUint32(10, true), Hf = dv.getUint32(14, true);
  // Terrain LOD: sub-sample the radix grid by `stride` at DECODE time. Grid cells are
  // ADDRESSES, so skipping is exact selection (never resampling) — stride 2 keeps every
  // other height sample untouched and yields ¼ the verts/tris AND ¼ the decode work
  // (the mobile fast path; the LOD toggle re-decodes live). Dims are sized so the LAST
  // full-grid row/col is always reachable — ceil((N-1)/stride)+1 — and the final index
  // clamps onto it, so the tile rim genuinely survives (plain ceil(N/stride) silently
  // dropped the rim row/col whenever N was even).
  const W = stride > 1 ? Math.ceil((Wf - 1) / stride) + 1 : Wf;
  const H = stride > 1 ? Math.ceil((Hf - 1) / stride) + 1 : Hf;
  const colF = new Uint32Array(W), rowF = new Uint32Array(H);
  for (let c = 0; c < W; c++) colF[c] = Math.min(c * stride, Wf - 1);
  for (let r = 0; r < H; r++) rowF[r] = Math.min(r * stride, Hf - 1);
  const nVf = Wf * Hf;                                // wire counts (offsets/slices)
  const nV = W * H, nT = (W - 1) * (H - 1) * 2;       // display counts
  let o = 18;
  o += 16 * nC;                     // guid
  o += nC;                          // material (unused)
  const layerOff = o; o += nC;      // LAYER u8
  const labelOff = o; o += 4 * nC;  // label idx
  const cenOff = o; o += 12 * nC;   // centroid 3f (pre-remapped like ver-6/7)
  o += 8 * nC;                      // vrange
  const x0 = dv.getFloat32(o, true), dx = dv.getFloat32(o + 4, true), yscale = dv.getFloat32(o + 8, true);
  o += 12;
  const zrow = new Float32Array(buf.slice(o, o + 4 * Hf)); o += 4 * Hf;
  // NORTH-UP FIX: the bake stores z = (lat − lat0)·M (north = +z), but the default
  // aerial camera sits on the +z side (screen-up = −z), which rendered every grid
  // scene N–S MIRRORED against a real map (caught on the Havel bake: Kölpinsee
  // appeared SOUTH of the Müritz). Negate z at decode — positions, normals and the
  // row table all derive from this one table, and the winding below is emitted in
  // the flipped order so faces keep pointing +y. Wire + HHTL keys are untouched.
  for (let i = 0; i < Hf; i++) zrow[i] = -zrow[i];
  const nK = dv.getUint8(o); o += 1;
  const pal = new Uint8Array(buf.slice(o, o + 3 * nK)); o += 3 * nK;
  const hfFull = new Uint16Array(buf.slice(o, o + 2 * nVf)); o += 2 * nVf;
  const kindsFull = new Uint8Array(buf.slice(o, o + nVf)); o += nVf;
  // ver-9: the raw per-vertex satellite drape (the "Diaprojektor" colour sunk into
  // the grid once). Row-major, same order as height/kind → the stride sub-samples it too.
  const rgbFull = ver >= 9 ? new Uint8Array(buf.slice(o, o + 3 * nVf)) : null;
  if (ver >= 9) o += 3 * nVf;
  const labLen = dv.getUint32(o, true); o += 4;
  let names: string[] = [];
  try { const lj = JSON.parse(new TextDecoder().decode(new Uint8Array(buf.slice(o, o + labLen)))); names = lj.names ?? lj; } catch { /* names optional */ }
  const cLayer = new Uint8Array(buf.slice(layerOff, layerOff + nC));

  // positions: radix x, tabulated z, decoded F16 height. Display frame DIRECT —
  // ver-8 synthesizes display coords; no (-x, z, y) source remap round-trip.
  const heights = new Float32Array(nV);
  const kinds = new Uint8Array(nV);
  const skin = rgbFull ? new Uint8Array(nV * 3) : null;   // ver-9: sunk satellite colour, stride-selected
  for (let r = 0, i = 0; r < H; r++) {
    const ro = rowF[r] * Wf;
    for (let c = 0; c < W; c++, i++) {
      const fi = ro + colF[c];
      heights[i] = HALF_LUT[hfFull[fi]] * yscale;
      kinds[i] = kindsFull[fi];
      if (skin) { skin[i * 3] = rgbFull![fi * 3]; skin[i * 3 + 1] = rgbFull![fi * 3 + 1]; skin[i * 3 + 2] = rgbFull![fi * 3 + 2]; }
    }
  }
  const positions = new Float32Array(nV * 3);
  const rowArr = new Uint32Array(nV);
  const layer = new Float32Array(nV);
  for (let r = 0, i = 0; r < H; r++) {
    const z = zrow[rowF[r]], cr = Math.min(rowF[r], nC - 1), li = cLayer[cr] || 8;
    for (let c = 0; c < W; c++, i++) {
      positions[i * 3] = x0 + colF[c] * dx;
      positions[i * 3 + 1] = heights[i];
      positions[i * 3 + 2] = z;
      rowArr[i] = cr;
      layer[i] = li;
    }
  }
  // normals: one central-difference gradient pass (one-sided at edges). True-scale
  // heights → true-world slopes, byte-parity with the ver-7 baker's terrain_normals.
  // normals: central-difference gradient over a WIDER (±2) stencil → a smoothed
  // surfel normal so the shading stops catching every one-cell quantization stair
  // (the "light on every numeric step" needle look), WITHOUT touching the geometry.
  const normals = new Int8Array(nV * 3);
  for (let r = 0; r < H; r++) {
    const rm = Math.max(r - 2, 0), rp = Math.min(r + 2, H - 1);
    const dzr = (zrow[rowF[rp]] - zrow[rowF[rm]]) || 1e-6;
    for (let c = 0; c < W; c++) {
      const i = r * W + c;
      const cm = Math.max(c - 2, 0), cp = Math.min(c + 2, W - 1);
      const gx = (heights[r * W + cp] - heights[r * W + cm]) / (((colF[cp] - colF[cm]) * dx) || 1e-6);
      const gz = (heights[rp * W + c] - heights[rm * W + c]) / dzr;
      const il = 127 / Math.hypot(gx, 1, gz);
      normals[i * 3] = Math.round(-gx * il);
      normals[i * 3 + 1] = Math.round(il);
      normals[i * 3 + 2] = Math.round(-gz * il);
    }
  }
  // colour: palette[kind] × the CONTINUOUS inter-family CurveRuler residue — the
  // ±18% within-kind /helix surfel texture. The stride-4-over-17 golden-spiral
  // value is sampled at integer lattice CORNERS and smoothstep-interpolated
  // between them (value-noise). This is the inter-family fix from
  // geo/src/kurvenlineal.rs: the old per-cell version STEPPED at every cell
  // boundary (intra-family discontinuity → blocky texture); this flows
  // continuously across cells — the surfel the body has. Corner values cached.
  const colors = new Uint8Array(nV * 3);
  if (skin) {
    // ver-9: the satellite photo IS the colour — copy it straight in (Diaprojektor
    // sunk once), no palette/CurveRuler recolour. The shader applies only a gentle
    // relief hillshade so the 3-D reads without muddying the photo.
    colors.set(skin);
  } else {
  const DETAIL = 48;
  const cornerCache = new Map<number, number>();
  const corner = (cx: number, cy: number, cz: number): number => {
    const key = (cx + 512) + (cy + 512) * 1024 + (cz + 512) * 1048576;
    let v = cornerCache.get(key);
    if (v === undefined) {
      const s = mix17(cx, cy, cz), k = mix17(cx + 7, cy + 13, cz + 29);
      v = (((s + 4 * k) % 17) / 16) * 2 - 1;
      cornerCache.set(key, v);
    }
    return v;
  };
  const smoothstep = (t: number) => t * t * (3 - 2 * t);
  for (let i = 0; i < nV; i++) {
    const X = positions[i * 3] * DETAIL, Y = positions[i * 3 + 1] * DETAIL, Z = positions[i * 3 + 2] * DETAIL;
    const bx = Math.floor(X), by = Math.floor(Y), bz = Math.floor(Z);
    const wx = smoothstep(X - bx), wy = smoothstep(Y - by), wz = smoothstep(Z - bz);
    let res = 0;
    for (let dz = 0; dz < 2; dz++)
      for (let dy = 0; dy < 2; dy++)
        for (let dx = 0; dx < 2; dx++)
          res += corner(bx + dx, by + dy, bz + dz) * (dx ? wx : 1 - wx) * (dy ? wy : 1 - wy) * (dz ? wz : 1 - wz);
    const sweet = 0.90 + 0.18 * res;
    const kb = kinds[i] * 3;
    colors[i * 3] = Math.max(0, Math.min(255, Math.round(pal[kb] * sweet)));
    colors[i * 3 + 1] = Math.max(0, Math.min(255, Math.round(pal[kb + 1] * sweet)));
    colors[i * 3 + 2] = Math.max(0, Math.min(255, Math.round(pal[kb + 2] * sweet)));
  }
  }
  // index: the grid loop — connectedness IS the address structure (baker winding).
  const index = new Uint32Array(nT * 3);
  let wI = 0;
  for (let r = 0; r < H - 1; r++) {
    for (let c = 0; c < W - 1; c++) {
      const a = r * W + c, b = a + 1, d2 = a + W, e = d2 + 1;
      // Winding for the NEGATED-z frame (north-up fix above): row r+1 now has
      // GREATER z, so emit (a,d2,b)/(b,d2,e) to keep the face normal +y (up).
      index[wI++] = a; index[wI++] = d2; index[wI++] = b;
      index[wI++] = b; index[wI++] = d2; index[wI++] = e;
    }
  }
  const labelIdx = new Uint32Array(buf.slice(labelOff, labelOff + 4 * nC));
  const cen = new Float32Array(buf.slice(cenOff, cenOff + 12 * nC));
  const conceptList: ConceptMeta[] = [];
  for (let c = 0; c < nC; c++) {
    conceptList.push({ row: c, name: names[labelIdx[c]] ?? `concept ${c}`, layer: cLayer[c] || 8,
      cx: -cen[c * 3], cy: cen[c * 3 + 2], cz: -cen[c * 3 + 1] });  // source → display (-x,-z,y): z negated by the north-up fix
  }
  return { nVerts: nV, nTris: nT, positions, index, colors, normals, layer, vrow: rowArr, concepts: nC, conceptList, isGrid: true, stride, skin: !!skin };
}

// Terrain LOD is a VERTEX BUDGET, not a blind ratio. A phone's per-frame ceiling is
// a vertex count, not a fraction — so decimate only enough to fit the budget, and
// leave any grid already under it at FULL resolution. 4.2M is what a 6-year-old phone
// renders smoothly (Iceland ½-res ≈ 4.1M), so: canyon (864k) → stride 1 (untouched);
// Iceland (16.5M) → stride 2 (≈4.1M). The old fixed stride-2 needlessly gutted the
// canyon to 216k — blocky low-poly for zero perf gain.
const GRID_VERT_BUDGET = 4_200_000;
function gridBudgetStride(buf: ArrayBuffer): number {
  const dv = new DataView(buf);
  const v = dv.getUint16(4, true);
  if (dv.getUint8(0) !== 0x42 || (v !== 8 && v !== 9)) return 1;          // ver-8/9 radix grid only
  const nv = dv.getUint32(10, true) * dv.getUint32(14, true);             // Wf × Hf from the header
  return Math.max(1, Math.round(Math.sqrt(nv / GRID_VERT_BUDGET)));
}

function decode(buf: ArrayBuffer, stride = 1): Decoded {
  const dv = new DataView(buf);
  const magic = String.fromCharCode(dv.getUint8(0), dv.getUint8(1), dv.getUint8(2), dv.getUint8(3));
  if (magic !== 'BSO2') throw new Error(`bad magic "${magic}"`);
  const ver = dv.getUint16(4, true);
  if (ver === 8 || ver === 9) return decodeGrid(buf, stride);   // radix-grid wire: height + kind (+ ver-9 satellite skin); stride = terrain LOD
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
  const colorOff = ver >= 7 ? o : -1;   // ver-7: per-vertex RGB drape (real kind/imagery colour)
  if (ver >= 7) o += 3 * nV;
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
  // ver-7 carries a real per-vertex colour drape (the DEM baker's kind × imagery × helix texture);
  // older bakes synthesize the colour from the concept layer. When present, the wire colour wins.
  const wireColors = ver >= 7 ? new Uint8Array(buf.slice(colorOff, colorOff + nV * 3)) : null;
  for (let i = 0; i < nV; i++) {
    positions[i * 3] = -srcPos[i * 3];
    positions[i * 3 + 1] = srcPos[i * 3 + 2];
    positions[i * 3 + 2] = srcPos[i * 3 + 1];
    const r0 = rowArr[i], li = cLayer[r0] || 8;
    const rgb = wireColors
      ? ([wireColors[i * 3], wireColors[i * 3 + 1], wireColors[i * 3 + 2]] as [number, number, number])
      : conceptColor(li, r0);
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
uniform float uMoss;   // 1 on vegetated scenes (Iceland) → aspect-based moss; 0 on the desert canyon.
uniform float uArid;   // 1 on arid/desert scenes → NO glacial turquoise (water stays plain river-blue);
                       // 0 on the glacial Iceland scene → meltwater teal. Drainage-brown is baked, not here.
uniform float uTopo;   // 1 = TOPO/OTM cartographic mode (tied to the contour overlay): swap the vivid
                       // surfel grade for pale beige-green topo paper so the contour lines live on a
                       // map, not on the skin of the world. 0 = the beauty look (default).
uniform float uSkin;   // 1 = ver-9 SATELLITE SKIN: aColor IS the raw photo → skip all hypsometric /
                       // water / moss / topo recolour, apply only a soft relief hillshade.
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
  vec3 rock  = vec3(0.49, 0.34, 0.23);   // COPPER volcanic/desert rock (was neutral brown)
  vec3 scree = vec3(0.61, 0.47, 0.34);   // light copper highlight (was grey scree)
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
  // fragment shader trivial.
  vec3 n = normalize(normalMatrix * aNormal);
  // Relief FIRST — the specular sun-glint needs the view vector to the final
  // (exaggerated) vertex position, so compute dpos + the view-space position up front.
  vec3 dpos = position;
  if (uGeo > 0.5) { dpos.y = position.y * uExag; }
  vec4 mvp = modelViewMatrix * vec4(dpos, 1.0);
  vec3 lit;
  if (uGeo > 0.5 && uSkin > 0.5) {
    // ── ver-9 SATELLITE SKIN — the photo IS the truth (Diaprojektor sunk into the
    //    grid). It already carries the sun's own shadows, so add ONLY a soft
    //    directional relief lift so the 3-D form reads, plus a whisper of saturation.
    //    No hypsometric ramp, no water re-tint, no topo paper — those would fight the
    //    real imagery. ──
    vec3 SUN = normalize(vec3(-0.55, 0.42, 0.72));
    float ndl = max(dot(n, SUN), 0.0);
    lit = aColor * (0.80 + 0.32 * ndl);
    float lum = dot(lit, vec3(0.299, 0.587, 0.114));
    lit = mix(vec3(lum), lit, 1.08);                    // +8% saturation, keep it natural
  } else if (uGeo > 0.5) {
    // (1) HYPSOMETRIC tint — blend the baked KIND colour (aColor) with the height ramp.
    vec3 base = mix(aColor, terrainColor(position.y), 0.55);
    // KIND masks from the RAW aColor (not the render): water = blue-dominant,
    // snow/ice = near-white. Scene-safe — the canyon's grey terrain matches neither.
    float wet  = smoothstep(0.06, 0.22, aColor.b - max(aColor.r, aColor.g));
    float snow = smoothstep(0.74, 0.88, min(aColor.r, min(aColor.g, aColor.b)));
    // (2) TURQUOISE water — push blue-KIND cells to a vivid glacial-meltwater teal
    //     (as turquoise as it gets). KIND-gated (only blue cells) AND scene-gated by
    //     uArid: glacial teal is Iceland's look; on the arid canyon the Water KIND
    //     (the Colorado) stays a natural deep river-blue, never tropical turquoise.
    base = mix(base, vec3(0.07, 0.62, 0.66), wet * 0.9 * (1.0 - uArid));
    // (2b) ARID scenes: the actual Water bodies (the Colorado) are the blue FOCAL
    //      POINT the user asked to reserve — but the 55% terrain blend + warm sunset
    //      key wash the thin river cells to neutral copper. Re-assert a clean deep
    //      river-blue for the (blue-KIND) water cells so the Colorado reads as water,
    //      not warm rock. Drainage (Stream) is baked rust-brown → wet=0 → untouched.
    base = mix(base, vec3(0.16, 0.44, 0.66), wet * uArid * 0.92);
    vec3 SUN = normalize(vec3(-0.55, 0.42, 0.72));       // low azimuth, ~25° elevation
    // (3) MOSS by ASPECT — moss/lichen favours the shaded, WEATHER-facing slopes (the
    //     "north-face holds the moisture" rule; ~ aspect × insolation). Tint vivid moss
    //     onto the vegetated mid-band where the face turns AWAY from the sun; sun-baked
    //     faces stay bare copper rock — the mottled green/copper of the reference. uMoss
    //     gates it to Iceland; the desert canyon (uMoss=0) stays bare rock.
    float hl  = clamp((position.y - uYMin) / max(uYMax - uYMin, 1e-6), 0.0, 1.0);
    float veg = smoothstep(0.02, 0.10, hl) * (1.0 - smoothstep(0.34, 0.66, sqrt(hl))); // low-mid, not ocean/ice
    float lee = smoothstep(-0.05, 0.65, dot(n, -SUN));   // 1 = weather/shaded (lee) side
    base = mix(base, vec3(0.22, 0.47, 0.12), uMoss * veg * (0.5 + 0.5 * lee) * (1.0 - wet) * 0.95);
    // (4) SUNSET lighting — a low WARM key + a cool SKY fill (golden lit slopes,
    //     cool-blue shadows) — the depth cue that makes a range feel alive.
    float ndl = max(dot(n, SUN), 0.0);
    float sky = 0.5 + 0.5 * n.y;                          // hemispheric fill, more from above
    vec3 light = vec3(1.26, 0.95, 0.66) * (0.16 + 0.95 * ndl)  // warm golden key
               + vec3(0.40, 0.50, 0.66) * (0.34 * sky);        // cool sky fill
    lit = base * light;
    // (5) VIVID GRADE — boost saturation + a touch of contrast for the electric
    //     Icelandic-highland look (moss greens pop, volcanic rock deepens, turquoise
    //     glows). Scene palettes live in aColor, so each scene punches its OWN colours —
    //     the desert canyon deepens to richer browns, it does NOT go green.
    float lum = dot(lit, vec3(0.299, 0.587, 0.114));
    lit = mix(vec3(lum), lit, 1.22);                     // +22% saturation (rich, not radioactive)
    lit = (lit - 0.5) * 1.06 + 0.5;                      // gentle contrast
    // (6) MAGIC HIGHLIGHTS — specular sun-glint on water (now turquoise) + ice sheen,
    //     KIND-gated as above so the canyon's peaks stay matte (no false glacier shine).
    vec3 V = normalize(-mvp.xyz);
    float nh = max(dot(n, normalize(SUN + V)), 0.0);
    float glint = pow(nh, 220.0) * wet;   // sharp sun-glitter on the turquoise water
    float sheen = pow(nh, 26.0) * snow;   // soft glacier sheen
    lit += vec3(1.5, 1.28, 0.92) * (glint * 1.4 + sheen * 0.45);
    // (7) LUMINOUS RIVER — on arid scenes the warm key would still mute the Colorado;
    //     lift the water cells (post-grade) toward a bright silvery river-blue so the
    //     river catches the light and stays the FOCAL POINT against the dark rock, the
    //     way it dominates the real canyon. Iceland (uArid=0) is untouched.
    lit = mix(lit, vec3(0.34, 0.58, 0.76), wet * uArid * 0.55);
    // (8) TOPO/OTM cartographic mode — the contour overlay belongs on classic topo
    //     PAPER (pale beige-white relief, green vegetation, light carto-blue water,
    //     gentle hillshade), not on the vivid surfel skin where dense lines flatten
    //     the image. uTopo swaps the whole grade; 0 = the beauty look untouched.
    vec3 paper = mix(vec3(0.92, 0.88, 0.79), vec3(0.99, 0.99, 0.97), smoothstep(0.30, 0.75, hl));
    float vegk = smoothstep(0.04, 0.16, aColor.g - max(aColor.r, aColor.b));   // Park/Woods KIND cells
    paper = mix(paper, vec3(0.78, 0.88, 0.62), vegk * 0.85);
    paper = mix(paper, vec3(0.52, 0.71, 0.88), wet);                            // water: light carto blue
    lit = mix(lit, paper * (0.62 + 0.38 * ndl), uTopo);
  } else {
    // Anatomy path — DEAD in GeoHelix (only /geo /ice /garmin route here), kept
    // byte-identical to BodyHelix so the fork stays a faithful copy.
    const vec3 L = vec3(-0.401, 0.783, 0.476);
    float ndl = max(abs(dot(n, L)), 0.0);
    float shade = min(0.34 + 0.20*(abs(n.y)*0.5+0.5) + 0.12*(-n.x*0.5+0.5) + 0.92*ndl, 1.3);
    lit = aColor * shade;
  }
  vColor = min(lit, vec3(1.8));   // headroom so the sun-glint can pop toward white
  gl_Position = projectionMatrix * mvp;
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
precision highp float;
varying vec3 vDir;
uniform vec3 uHorizon; uniform vec3 uZenith;
uniform float uTime;
// value-noise
float h21(vec2 p){ p = fract(p * vec2(123.34, 345.45)); p += dot(p, p + 34.345); return fract(p.x * p.y); }
float vn(vec2 p){
  vec2 i = floor(p), f = fract(p);
  vec2 u = f * f * (3.0 - 2.0 * f);
  float a = h21(i), b = h21(i + vec2(1,0)), c = h21(i + vec2(0,1)), d = h21(i + vec2(1,1));
  return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}
// TRIBONACCI constant (Σ of the previous three ≈ 1.8393). Using it as the octave
// frequency ratio (lacunarity) means successive octaves never land on a repeating
// lattice → the drifting clouds read ORGANIC and non-repeating — the number-theoretic
// cousin of the workspace's golden-spiral kurvenlineal.
const float TRIB = 1.8392867552;
float fbm(vec2 p){
  float s = 0.0, a = 0.5, norm = 0.0;
  for (int i = 0; i < 5; i++){
    s += a * vn(p);
    norm += a;
    p = p * TRIB + vec2(1.7, -2.9);   // tribonacci lacunarity + offset breaks octave alignment
    a *= 0.5437;                        // 1/TRIB amplitude gain
  }
  return s / norm;
}
void main(){
  vec3 dir = normalize(vDir);
  float t = clamp(dir.y * 0.5 + 0.5, 0.0, 1.0);
  vec3 sky = mix(uHorizon, uZenith, smoothstep(0.05, 0.95, t));
  // project the view direction onto a virtual cloud layer, drift it over time
  vec2 uv = dir.xz / (abs(dir.y) + 0.30) * 0.9 + vec2(uTime * 0.006, uTime * 0.0042);
  float c = smoothstep(0.28, 0.86, fbm(uv));                    // billowy structure
  float cover = mix(0.32, 0.96, smoothstep(0.04, 0.66, t));     // broad cover, thicker higher up
  float cloud = c * cover;
  vec3 cloudCol = mix(vec3(0.55, 0.57, 0.62), vec3(0.98, 0.99, 1.0), smoothstep(0.30, 0.95, c)); // shaded → lit tops
  gl_FragColor = vec4(mix(sky, cloudCol, cloud), 1.0);
}`;
type Focus = { x: number; y: number; z: number; d: number };

// ── DRP1 drape decode — the OSM ⊕ Garmin vector overlay ──────────────────────
// b"DRP1" | ver u16 | nLines u32 | nKind u8 | palette(nK×3) | scale f32
//         | per line: kind u8 | nPts u16 | pts(nPts × 3 i16, coord = i16/scale)
// Each polyline expands to LineSegments pairs; each vertex is coloured by its
// KIND from the palette. Positions are the terrain surface point PRE-exag (the
// bake bilinear-sampled the ver-8 pos grid), so mount lifts y by the same uExag.
function decodeDrape(buf: ArrayBuffer): DrapeData {
  const dv = new DataView(buf);
  if (!(dv.getUint8(0) === 0x44 && dv.getUint8(1) === 0x52 && dv.getUint8(2) === 0x50 && dv.getUint8(3) === 0x31)) {
    throw new Error('not a DRP1 drape');
  }
  const nLines = dv.getUint32(6, true);
  const nK = dv.getUint8(10);
  let o = 11;
  const pal = new Uint8Array(buf.slice(o, o + 3 * nK)); o += 3 * nK;
  const inv = 1 / dv.getFloat32(o, true); o += 4;
  const body = o;
  // Pass 1 — count segments (Σ nPts−1) for exact typed allocation.
  let segs = 0;
  for (let l = 0; l < nLines; l++) {
    const nPts = dv.getUint16(o + 1, true); o += 3 + nPts * 6;
    if (nPts >= 2) segs += nPts - 1;
  }
  const positions = new Float32Array(segs * 2 * 3);
  const colors = new Uint8Array(segs * 2 * 3);
  // Pass 2 — fill segment pairs + per-vertex KIND colour.
  o = body;
  let w = 0;
  for (let l = 0; l < nLines; l++) {
    const kind = dv.getUint8(o); const nPts = dv.getUint16(o + 1, true); o += 3;
    const cb = (kind * 3) % (nK * 3);
    const r = pal[cb], g = pal[cb + 1], b = pal[cb + 2];
    let px = 0, py = 0, pz = 0;
    for (let p = 0; p < nPts; p++, o += 6) {
      const x = dv.getInt16(o, true) * inv, y = dv.getInt16(o + 2, true) * inv, z = dv.getInt16(o + 4, true) * inv;
      if (p > 0) {
        positions[w] = px; positions[w + 1] = py; positions[w + 2] = pz;
        positions[w + 3] = x; positions[w + 4] = y; positions[w + 5] = z;
        colors[w] = r; colors[w + 1] = g; colors[w + 2] = b;
        colors[w + 3] = r; colors[w + 4] = g; colors[w + 5] = b;
        w += 6;
      }
      px = x; py = y; pz = z;
    }
  }
  return { positions, colors, segCount: segs, kindCount: nK };
}

function mount(container: HTMLDivElement, d: Decoded, enabled: Float32Array, dirty: { current: boolean }, focus: { current: Focus | null }, xray: { current: boolean }, lod: { current: boolean }, features: { current: boolean }, drape: { current: DrapeData | null }, contours: { current: DrapeData | null }, showContours: { current: boolean }): () => void {
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
  const sceneName = sceneParam ?? pathScene();
  const isTerrainScene = sceneName === 'iceland' || Boolean(sceneName?.startsWith('garmin:'));

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
  // Relief exaggeration AUTO-SCALED by the measured height span so every terrain
  // reads with a consistent vertical presence (~0.11 of the frame) regardless of
  // its true-scale span. Iceland (span ~0.0074) → ~15 (matches the hand-tuned
  // value); the Grand Canyon (span ~0.05, far deeper relative to its extent) →
  // ~2.2, so its steep walls read as WALLS, not an over-exaggerated needle
  // curtain (caught on the first /garmin/grand-canyon screenshot). Clamped.
  //
  // The cap matters MORE than the target for very-flat-span terrain: Iceland's
  // true-scale span is tiny (~0.0067 in the frame), so 0.11/span ≈ 16× — which
  // over-verticalizes its genuinely rugged ~100 m fjord/ridge detail into a
  // needle field (the DEM is NOT quantized — 16.5 M verts, ~zero isolated spikes;
  // the needles are real terrain × too much exaggeration). Capping at a MODEST
  // value tames Iceland to read as a clean island; the canyon (0.11/0.05 ≈ 2.2×)
  // sits below the cap, so it is UNCHANGED.
  const EXAG_CAP = 4.2;
  const uExagVal = isTerrainScene ? Math.min(EXAG_CAP, Math.max(1.5, 0.11 / Math.max(yMax - yMin, 1e-6))) : 1;
  const isIcelandScene = sceneName === 'iceland' || sceneName === 'garmin:iceland';   // moss = green Iceland, not the desert canyon
  // Glacial turquoise is Iceland's look; every other terrain scene keeps plain river-blue
  // water (the canyon's Colorado). uArid = "not the glacial Iceland scene".
  const uAridVal = isTerrainScene && !isIcelandScene ? 1 : 0;
  const uniforms = { uAlpha: { value: 1 }, uGeo: { value: isTerrainScene ? 1 : 0 }, uYMin: { value: yMin }, uYMax: { value: yMax }, uExag: { value: uExagVal }, uTime: { value: 0 }, uRuler: { value: 0 }, uMoss: { value: isIcelandScene ? 1 : 0 }, uArid: { value: uAridVal }, uTopo: { value: 0 }, uSkin: { value: d.skin ? 1 : 0 } };
  const mat = new THREE.ShaderMaterial({ uniforms, vertexShader: VERT, fragmentShader: FRAG, side: THREE.FrontSide });
  const mesh = new THREE.Mesh(geom, mat); scene.add(mesh);

  // ── OSM ⊕ Garmin drape overlay — the semantic vector network (roads / trails /
  //    rivers) lifted onto the SAME surface. The bake stored the display-frame
  //    surface point PRE-exaggeration; here we lift y by the identical uExag the
  //    terrain shader applies (dpos.y = position.y * uExag), plus a hair of offset
  //    so the lines ride just above the surface instead of z-fighting it. Rendered
  //    as vertex-coloured LineSegments; toggled live via `features`. ──
  let drapeGeom: THREE.BufferGeometry | null = null;
  let drapeMat: THREE.LineBasicMaterial | null = null;
  let drapeLines: THREE.LineSegments | null = null;
  // Built LAZILY the first frame the drape ref is populated — so the terrain
  // renders immediately and the overlay pops in when its (optional) fetch lands,
  // without blocking the scene or forcing a remount.
  const buildDrape = (dd: DrapeData) => {
    if (drapeLines || dd.segCount <= 0) return;
    const src = dd.positions;
    const dp = new Float32Array(src.length);
    const lift = 0.0025;   // display units above the surface (post-exag)
    for (let i = 0; i < src.length; i += 3) {
      dp[i] = src[i];
      dp[i + 1] = src[i + 1] * uExagVal + lift;
      dp[i + 2] = src[i + 2];
    }
    drapeGeom = new THREE.BufferGeometry();
    drapeGeom.setAttribute('position', new THREE.BufferAttribute(dp, 3));
    drapeGeom.setAttribute('color', new THREE.Uint8BufferAttribute(dd.colors, 3, true));
    drapeMat = new THREE.LineBasicMaterial({ vertexColors: true });
    drapeLines = new THREE.LineSegments(drapeGeom, drapeMat);
    drapeLines.visible = features.current;
    scene.add(drapeLines);
    dirty.current = true;
  };
  if (drape.current) buildDrape(drape.current);   // already arrived (warm cache / fast net)

  // ── Contour-line overlay — the topo lines lifted onto the surface (same DRP1 wire
  //    as the drape, its own optional fetch). Rendered as thin, semi-transparent
  //    vertex-coloured lines riding JUST below the road/river drape so the network
  //    reads over the contours (the OpenTopoMap look). Toggled via `showContours`. ──
  let contourGeom: THREE.BufferGeometry | null = null;
  let contourMat: THREE.LineBasicMaterial | null = null;
  let contourLines: THREE.LineSegments | null = null;
  const buildContours = (dd: DrapeData) => {
    if (contourLines || dd.segCount <= 0) return;
    const src = dd.positions;
    const dp = new Float32Array(src.length);
    const lift = 0.0015;   // just under the drape's 0.0025 so roads/rivers draw on top
    for (let i = 0; i < src.length; i += 3) {
      dp[i] = src[i];
      dp[i + 1] = src[i + 1] * uExagVal + lift;
      dp[i + 2] = src[i + 2];
    }
    contourGeom = new THREE.BufferGeometry();
    contourGeom.setAttribute('position', new THREE.BufferAttribute(dp, 3));
    contourGeom.setAttribute('color', new THREE.Uint8BufferAttribute(dd.colors, 3, true));
    contourMat = new THREE.LineBasicMaterial({ vertexColors: true, transparent: true, opacity: 0.55 });
    contourLines = new THREE.LineSegments(contourGeom, contourMat);
    contourLines.visible = showContours.current;
    scene.add(contourLines);
    dirty.current = true;
  };
  if (contours.current) buildContours(contours.current);

  // Sky dome — geo scenes only (anatomy keeps the flat PAGE_BG set above, byte-identical).
  let skyGeom: THREE.SphereGeometry | null = null;
  let skyMat: THREE.ShaderMaterial | null = null;
  if (isGeoScene) {
    scene.background = SKY_HORIZON.clone();   // horizon tint behind the dome (any gap reads as sky)
    skyGeom = new THREE.SphereGeometry(40, 32, 16);   // radius 40 < camera far (100), surrounds the orbit
    skyMat = new THREE.ShaderMaterial({
      side: THREE.BackSide, depthWrite: false,
      uniforms: { uHorizon: { value: SKY_HORIZON.clone() }, uZenith: { value: SKY_ZENITH.clone() }, uTime: { value: 0 } },
      vertexShader: SKY_VERT, fragmentShader: SKY_FRAG,
    });
    const sky = new THREE.Mesh(skyGeom, skyMat);
    sky.renderOrder = -1;   // draw first → terrain (with depth) always paints over it
    scene.add(sky);
  }

  // minimal orbit+pan: left-drag = rotate · RIGHT-drag or TWO-FINGER touch = pan the
  // map · wheel / pinch = dolly. `touch-action: none` is load-bearing: without it the
  // browser claims two-finger gestures for page zoom/scroll before pointer events ever
  // fire — which is exactly why two-finger pan "did nothing" on mobile.
  // Geo scenes open on an AERIAL oblique view (el ~35°) so the terrain reads as a landscape under
  // the sky dome, not edge-on; anatomy keeps the near-level body orbit. Target lifts slightly so
  // the raised relief sits in frame. Drag/wheel still free-orbit from there.
  let az = 0, el = isGeoScene ? 0.62 : 0.1, dist = isGeoScene ? 2.6 : 3.0, dragging = false, panning = false, px = 0, py = 0;
  const target = new THREE.Vector3(0, isGeoScene ? 0.08 : 0, 0);
  const pointers = new Map<number, { x: number; y: number }>();
  let pinchD = 0, gcx = 0, gcy = 0;   // two-finger gesture state (centroid + pinch span)
  const panBy = (dx: number, dy: number) => {
    // world units per screen pixel at the orbit target's depth, along the camera basis
    const s = (2 * dist * Math.tan((camera.fov * Math.PI) / 360)) / h;
    const right = new THREE.Vector3().setFromMatrixColumn(camera.matrixWorld, 0);
    const up = new THREE.Vector3().setFromMatrixColumn(camera.matrixWorld, 1);
    target.addScaledVector(right, -dx * s).addScaledVector(up, dy * s);
    dirty.current = true;
  };
  const onDown = (e: PointerEvent) => {
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (pointers.size === 2) {   // second finger down → pan+pinch gesture, init state
      const [a, b] = [...pointers.values()];
      gcx = (a.x + b.x) / 2; gcy = (a.y + b.y) / 2; pinchD = Math.hypot(a.x - b.x, a.y - b.y);
      dragging = panning = false;
    } else {
      panning = e.button === 2; dragging = !panning;
      px = e.clientX; py = e.clientY;
    }
    focus.current = null; dirty.current = true;
  };
  const onUp = (e: PointerEvent) => {
    pointers.delete(e.pointerId);
    if (pointers.size < 2) pinchD = 0;
    if (pointers.size === 0) { dragging = false; panning = false; }
    dirty.current = true;
  };
  const onMove = (e: PointerEvent) => {
    const p = pointers.get(e.pointerId);
    if (p) { p.x = e.clientX; p.y = e.clientY; }
    if (pointers.size === 2) {   // two-finger: centroid delta pans, span change dollies
      const [a, b] = [...pointers.values()];
      const ncx = (a.x + b.x) / 2, ncy = (a.y + b.y) / 2, nd = Math.hypot(a.x - b.x, a.y - b.y);
      panBy(ncx - gcx, ncy - gcy);
      if (pinchD > 0 && nd > 0) dist = Math.max(0.3, Math.min(8, dist * (pinchD / nd)));
      gcx = ncx; gcy = ncy; pinchD = nd; dirty.current = true;
      return;
    }
    if (panning) { panBy(e.clientX - px, e.clientY - py); px = e.clientX; py = e.clientY; return; }
    if (!dragging) return;
    az -= (e.clientX - px) * 0.005; el = Math.max(-1.5, Math.min(1.5, el + (e.clientY - py) * 0.005));
    px = e.clientX; py = e.clientY; dirty.current = true;
  };
  const onWheel = (e: WheelEvent) => { e.preventDefault(); dist = Math.max(0.3, Math.min(8, dist * (1 + Math.sign(e.deltaY) * 0.1))); dirty.current = true; };
  const onCtx = (e: MouseEvent) => e.preventDefault();   // right-drag pans; no context menu
  const el2 = renderer.domElement;
  el2.style.touchAction = 'none';
  el2.addEventListener('pointerdown', onDown); window.addEventListener('pointerup', onUp);
  window.addEventListener('pointermove', onMove); el2.addEventListener('wheel', onWheel, { passive: false });
  el2.addEventListener('contextmenu', onCtx);
  window.addEventListener('pointercancel', onUp);   // browser-stolen touches must not leave stale gesture state

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

  // ── Stats HUD — the "is it doing real work?" readout: rendered triangles / draw
  //    calls straight from renderer.info (ground truth, not UI state), total decoded
  //    verts, and the honest LOD status. On geo scenes LOD reads `n/a`: postLod()
  //    early-returns for isGeoScene (the /api/body/lod cascade culls by the ANATOMY
  //    body's block-bounds — running it here would cull terrain wrongly; a per-scene
  //    .blocks cascade is future work), so the numbers make the inertness visible
  //    instead of leaving a toggle that silently does nothing. ──
  if (getComputedStyle(container).position === 'static') container.style.position = 'relative';
  const hud = document.createElement('div');
  hud.style.cssText = 'position:absolute;right:12px;bottom:10px;font:11px ui-monospace,SFMono-Regular,Menlo,monospace;color:rgba(255,255,255,.78);background:rgba(10,14,20,.5);padding:4px 9px;border-radius:6px;pointer-events:none;white-space:pre;z-index:5';
  container.appendChild(hud);
  let hudNext = 0;
  const fmtM = (n: number) => n >= 1e6 ? `${(n / 1e6).toFixed(2)}M` : n >= 1e3 ? `${(n / 1e3).toFixed(0)}k` : `${n}`;

  let raf = 0, ema = 16.6, last = performance.now(), sig = enabled.join(','), t0 = performance.now(), lastCloud = 0;
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
    // Drifting clouds: a TERRAIN scene advances uTime and asks for a redraw so the
    // cloud shadows keep moving — THROTTLED to ~15 fps (slow clouds need no more; caps
    // the continuous redraw of a heavy DEM so it's cheap on battery). Anatomy/buildings
    // never enter this branch and stay fully on-demand (no idle cost).
    if (isTerrainScene && skyMat && tnow - lastCloud >= 66) { lastCloud = tnow; skyMat.uniforms.uTime.value = (tnow - t0) * 0.001; dirty.current = true; }
    // render ON DEMAND (non-terrain): a static body (no drag/zoom/toggle) costs nothing —
    // 6.8 M tris are only redrawn when something actually changes (idle + heat sane).
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
    // drape overlay (roads/trails/rivers): build it the frame its optional fetch lands
    // (never blocked the terrain), then on/off is a cheap visibility flag — no rebuild.
    if (!drapeLines && drape.current) buildDrape(drape.current);
    if (drapeLines && drapeLines.visible !== features.current) drapeLines.visible = features.current;
    // contour overlay: same lazy-build-then-toggle as the drape (own optional fetch).
    // The toggle IS the topo-mode switch: lines visible ⇒ the shader swaps to the
    // cartographic paper palette (uTopo) — contours are a map layer, not the skin
    // of the world, so they never draw over the vivid beauty grade.
    if (!contourLines && contours.current) buildContours(contours.current);
    if (contourLines && contourLines.visible !== showContours.current) contourLines.visible = showContours.current;
    // Topo activates from the button on a ver-9 photoreal scene EVEN without contour
    // data (a cropped canyon has none) — it flips the satellite skin off for the
    // cartographic paper look; contour LINES draw on top only if the sidecar exists.
    // Non-skin scenes still require contour data to enter topo mode (unchanged).
    const topoOn = (d.skin || contourLines) && showContours.current ? 1 : 0;
    uniforms.uTopo.value = topoOn;
    if (d.skin) uniforms.uSkin.value = topoOn ? 0 : 1;
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
    // HUD refresh (≤2 Hz, only on frames that actually rendered): live renderer.info
    // numbers — if these change while zooming/toggling, the system is doing real work.
    if (tnow >= hudNext) {
      hudNext = tnow + 500;
      const ri = renderer.info.render;
      // Grid scenes have REAL client LOD (stride re-decode); mesh geo scenes don't
      // (yet); the body scene has the server HHTL cascade. Say which, honestly.
      const st = d.stride || 1;
      const lodTxt = d.isGrid
        ? `${lod.current ? 'on' : 'off'} · ${st > 1 ? `1/${st} grid` : 'full grid'}`
        : isGeoScene ? 'n/a (mesh scene)' : lod.current ? 'on' : 'off';
      hud.textContent = `tris ${fmtM(ri.triangles)} · lines ${fmtM(ri.lines)} · calls ${ri.calls} · verts ${fmtM(d.nVerts)} · LOD ${lodTxt}`;
    }
  };
  tick();
  return () => {
    cancelAnimationFrame(raf);
    el2.removeEventListener('pointerdown', onDown); window.removeEventListener('pointerup', onUp);
    window.removeEventListener('pointermove', onMove); el2.removeEventListener('wheel', onWheel);
    el2.removeEventListener('contextmenu', onCtx);
    window.removeEventListener('pointercancel', onUp);
    window.removeEventListener('resize', onResize);
    geom.dispose(); mat.dispose();
    if (drapeGeom) drapeGeom.dispose();
    if (drapeMat) drapeMat.dispose();
    if (contourGeom) contourGeom.dispose();
    if (contourMat) contourMat.dispose();
    if (skyGeom) skyGeom.dispose();
    if (skyMat) skyMat.dispose();
    renderer.dispose();
    if (hud.parentElement === container) container.removeChild(hud);
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
  // /garmin/<loc> — mod-rewrite style: the SERVER resolves the slug through the
  // manifest's garmin_scenes registry (/api/garmin/:location) and serves the bake.
  // A 404 body lists the available slugs, so surface it verbatim.
  if (scene?.startsWith('garmin:')) {
    const loc = scene.slice('garmin:'.length);
    const r = await fetch(`/api/garmin/${loc}`);
    if (!r.ok) {
      const detail = await r.json().then((j) => `${j.error}${j.available ? ` — available: ${j.available.join(', ')}` : ''}`).catch(() => `HTTP ${r.status}`);
      throw new Error(`garmin scene "${loc}": ${detail}`);
    }
    return inflate(r);
  }
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

export default function GeoHelix() {
  const ref = useRef<HTMLDivElement>(null);
  // Scene shape decides the UI up-front (same resolution as fetchSoa/mount): terrain
  // scenes hide the structure sidebar (a grid's rows are strips, not named anatomy);
  // ver-8 GRID scenes get real client LOD (stride re-decode) — auto-ON on mobile,
  // where full-res decode of the 16.5M-vert Iceland grid is the "35 seconds".
  const sceneStr = new URLSearchParams(window.location.search).get('scene') ?? pathScene();
  const isGeoUi = Boolean(sceneStr);
  const isTerrainUi = sceneStr === 'iceland' || Boolean(sceneStr?.startsWith('garmin:'));
  const isGridScene = Boolean(sceneStr?.startsWith('garmin:'));   // ver-8 radix-grid wires
  const isMobile = typeof matchMedia !== 'undefined' && matchMedia('(pointer: coarse)').matches && navigator.maxTouchPoints > 0;
  const [d, setD] = useState<Decoded | null>(null);
  const [error, setError] = useState('');
  const [on, setOn] = useState<Record<number, boolean>>({ 1: false, 2: false, 3: true, 4: true, 5: true, 6: true, 7: true, 8: true });
  const [xray, setXray] = useState(false);
  // LOD: on the anatomy body = the server HHTL cascade (opt-in). On ver-8 GRID
  // terrain = client stride re-decode (½-res grid, ¼ verts/tris/decode-time) —
  // auto-ON for mobile so a phone never pays the full-grid decode on first load.
  const [lod, setLod] = useState(isGridScene && isMobile);
  const [features, setFeatures] = useState(true);   // OSM ⊕ Garmin drape overlay on/off
  const [hasDrape, setHasDrape] = useState(false);  // a drape overlay loaded → show its toggle
  const [showContours, setShowContours] = useState(false);  // topo mode (contours + carto palette) — OFF by default: beauty mode is the skin of the world
  const [hasContours, setHasContours] = useState(false);    // contours loaded → show its toggle
  const [query, setQuery] = useState('');
  const [open, setOpen] = useState<Record<number, boolean>>({ 4: true });  // expanded layer groups
  const enabledRef = useRef(new Float32Array([0, 0, 1, 1, 1, 1, 1, 1, 1]));
  const dirtyRef = useRef(true);   // request a redraw (the render loop is on-demand)
  const focusRef = useRef<Focus | null>(null);
  const xrayRef = useRef(false);
  const lodRef = useRef(isGridScene && isMobile);
  const featuresRef = useRef(true);
  const drapeRef = useRef<DrapeData | null>(null);
  const showContoursRef = useRef(false);
  const contourRef = useRef<DrapeData | null>(null);
  // Grid terrain LOD keeps the inflated wire around so toggling re-decodes live at
  // the other stride (no refetch). decodedStrideRef = what stride `d` was built at.
  const rawRef = useRef<ArrayBuffer | null>(null);
  const decodedStrideRef = useRef(1);

  useEffect(() => {
    let cancelled = false;
    // Terrain: render as soon as it decodes — NEVER gated on the optional drape.
    // Grid scenes decode at the LOD stride (mobile auto-½-res) and KEEP the wire
    // so the LOD toggle re-decodes live without refetching.
    fetchSoa().then((b) => {
      const stride = lodRef.current ? gridBudgetStride(b) : 1;
      const x = decode(b, stride);
      if (x.isGrid) rawRef.current = b;
      decodedStrideRef.current = stride;
      if (!cancelled) setD(x);
    }).catch((e) => { if (!cancelled) setError(String(e)); });
    // OSM ⊕ Garmin drape: fetched INDEPENDENTLY for garmin terrain scenes and
    // attached to the live scene via drapeRef when it lands (the overlay is purely
    // additive — a 404 or a slow fetch never blocks or delays the terrain render).
    const scene = new URLSearchParams(window.location.search).get('scene') ?? pathScene();
    if (scene?.startsWith('garmin:')) {
      (async () => {
        try {
          const dr = await fetch(`/api/garmin-drape/${scene.slice('garmin:'.length)}`);
          if (dr.ok && !cancelled) {
            drapeRef.current = decodeDrape(await inflate(dr));
            setHasDrape(true);        // reveal the `features` toggle
            dirtyRef.current = true;  // wake the render loop → mount lazily builds the overlay
          }
        } catch { /* no drape overlay → bare terrain */ }
      })();
      // Contour overlay — same independent, additive, non-blocking fetch as the drape.
      (async () => {
        try {
          const cr = await fetch(`/api/garmin-contours/${scene.slice('garmin:'.length)}`);
          if (cr.ok && !cancelled) {
            contourRef.current = decodeDrape(await inflate(cr));
            setHasContours(true);     // reveal the `contours` toggle
            dirtyRef.current = true;  // wake the render loop → mount lazily builds the overlay
          }
        } catch { /* no contour overlay → terrain without topo lines */ }
      })();
    }
    return () => { cancelled = true; };
  }, []);
  useEffect(() => {
    // Mutate IN PLACE — mount captured THIS array; reassigning a new one leaves the
    // renderer reading the stale array (the dead-toggles bug). Then request a redraw.
    for (let i = 1; i <= 8; i++) enabledRef.current[i] = on[i] ? 1 : 0;
    dirtyRef.current = true;
  }, [on]);
  useEffect(() => { xrayRef.current = xray; dirtyRef.current = true; }, [xray]);
  useEffect(() => {
    lodRef.current = lod; dirtyRef.current = true;
    // Grid terrain LOD: re-decode the kept wire at the new stride (deferred a tick so
    // the button repaints before the decode blocks). The mount effect remounts on the
    // new `d`. Body scenes have no rawRef → the server-cascade path is untouched.
    const want = lod && rawRef.current ? gridBudgetStride(rawRef.current) : 1;
    if (rawRef.current && decodedStrideRef.current !== want) {
      decodedStrideRef.current = want;
      const raw = rawRef.current;
      setTimeout(() => { try { setD(decode(raw, want)); } catch { /* keep the current mesh */ } }, 30);
    }
  }, [lod]);
  useEffect(() => { featuresRef.current = features; dirtyRef.current = true; }, [features]);
  useEffect(() => { showContoursRef.current = showContours; dirtyRef.current = true; }, [showContours]);
  useEffect(() => { const c = ref.current; if (!c || !d) return; return mount(c, d, enabledRef.current, dirtyRef, focusRef, xrayRef, lodRef, featuresRef, drapeRef, contourRef, showContoursRef); }, [d]);

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
  const geoTerrain = geoScene === 'iceland' || Boolean(geoScene?.startsWith('garmin:'));
  const geoLayerName = geoTerrain ? 'terrain' : 'buildings';
  const geoLayerColor = geoTerrain ? '#7c8f5c' : '#9aa7b4';
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
    : scene?.startsWith('garmin:') ? `/garmin/${scene.slice(7)} — Garmin terrain`
    : scene ? `/helix?scene=${scene}`
    : '/helix — living anatomy browser';
  const subtitle = d
    ? scene
      ? `${d.nVerts.toLocaleString()} verts · ${d.concepts.toLocaleString()} structures · ${scene === 'iceland' || scene.startsWith('garmin:') ? 'height-profile palette + sky' : 'baked colours + sky'}`
      : `${d.nVerts.toLocaleString()} verts · ${d.concepts.toLocaleString()} structures · helix::Signed360 normals (Fisher-Z rim)`
    : 'loading canonical helix bake…';
  // Scene menu — the "menu item" leg of the /garmin arc: built-ins + every slug the
  // manifest's garmin_scenes registry names. Plain navigation (each scene is a page).
  const [garminScenes, setGarminScenes] = useState<string[]>([]);
  useEffect(() => {
    fetch('/body.manifest.json').then((r) => (r.ok ? r.json() : null))
      .then((m) => { if (m?.garmin_scenes) setGarminScenes(Object.keys(m.garmin_scenes)); })
      .catch(() => {});
  }, []);
  const sceneOptions = [
    { label: 'body (/helix)', path: '/helix' },
    { label: 'berlin (/geo)', path: '/geo' },
    { label: 'iceland (/ice)', path: '/ice' },
    ...garminScenes.map((s) => ({ label: `garmin: ${s}`, path: `/garmin/${s}` })),
  ];
  // /havel is a first-class alias of /garmin/havel — the menu should show it selected.
  const herePath = window.location.pathname === '/havel' ? '/garmin/havel' : window.location.pathname;

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
          <select
            value={sceneOptions.some((o) => o.path === herePath) ? herePath : '/helix'}
            onChange={(e) => { window.location.href = e.target.value; }}
            title="scene: every bake this deploy serves — built-ins plus the manifest's garmin_scenes registry"
            style={{ background: '#0e1219', color: '#cdd9e5', border: '1px solid #243244', borderRadius: 7, font: '12px ui-monospace, monospace', padding: '4px 6px' }}>
            {sceneOptions.map((o) => <option key={o.path} value={o.path}>{o.label}</option>)}
          </select>
          {hasDrape && (
            <button style={btn(features)} onClick={() => setFeatures((v) => !v)} title="features: the OSM ⊕ Garmin vector overlay — roads, trails and rivers draped onto the terrain surface (fused ↔ Garmin-only)">features {features ? 'on' : 'off'}</button>
          )}
          {hasContours && (
            <button style={btn(showContours)} onClick={() => setShowContours((v) => !v)} title="topo: OpenTopoMap-style cartographic mode — contour lines over a pale beige-green relief palette. Off = the beauty surfel look (default); the contours are a map layer, not the skin of the world.">topo {showContours ? 'on' : 'off'}</button>
          )}
          <button style={btn(xray)} onClick={() => setXray((x) => !x)} title="x-ray: make the whole body translucent so deeper structures show through">x-ray</button>
          <button
            style={{ ...btn(lod), ...(isGeoUi && !isGridScene ? { opacity: 0.45, cursor: 'not-allowed' } : {}) }}
            disabled={isGeoUi && !isGridScene}
            onClick={() => { if (!(isGeoUi && !isGridScene)) setLod((v) => !v); }}
            title={isGridScene
              ? 'terrain LOD: re-decode the ver-8 grid at half resolution — ¼ verts/tris and ¼ decode time (auto-ON on mobile). The HUD shows the live counts.'
              : isGeoUi
                ? 'LOD is body-only on mesh scenes: the /api/body/lod depth-cascade culls by the anatomy body’s block-bounds, so it is deliberately inert here (full mesh always renders — see the HUD tris count).'
                : 'LOD: the HHTL depth-cascade culls off-frustum structures as you zoom in — the living database deciding what’s worth drawing'}
          >{isGeoUi && !isGridScene ? 'LOD n/a' : `LOD ${lod ? 'on' : 'off'}`}</button>
          {activeLayers.map((l) => (
            <button key={l.id} style={btn(on[l.id])} onClick={() => setOn((p) => ({ ...p, [l.id]: !p[l.id] }))}>
              <span style={{ display: 'inline-block', width: 8, height: 8, borderRadius: 4, background: l.color, marginRight: 5, verticalAlign: 'middle' }} />{l.name}
            </button>
          ))}
        </div>
      )}
      {d && !isTerrainUi && (
        // Structure sidebar — anatomy/building scenes only. A terrain grid's
        // "structures" are its row strips (thousands of identical unnamed entries),
        // so the list is noise there; terrain scenes get the full viewport instead.
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
