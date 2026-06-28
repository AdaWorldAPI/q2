// FMA body · full-resolution SoA (BSO2 v3) · Gouraud · #17-palette alpha · compartments.
//
// Reads the option-2 rebake wire `body.soa` (BSO2): SoA columns, the two-GUID design
// (address = classid 0x1000 + (part_of:is_a) 8:8 + identity; location = XYZ; helix
// server-side). material + label + LAYER are codebook indices (never raw text).
// Renders the full 6.68 M-triangle body: Gouraud, #17-palette alpha (Doppler material),
// and an 8-LAYER compartment menu (skin/muscle/organ/skeleton/vessel/nerve/connective/
// other) so the skin shell can be toggled away to reveal the anatomy.
//
// BSO2 (LE): magic "BSO2" | ver u16 | nC u32 | nV u32 | nT u32
//   | GUID[16·nC] | material u8[nC] | LAYER u8[nC] | label u32[nC] | centroid 3f[nC]
//   | (vstart,vcount) 2u32[nC] | pos[nV] | helix 6B[nV] | row u32[nV] | idx 3u32[nT]
//   | labels_json (u32 len + utf8) | materials_json (u32 len + utf8)
// pos column: ver 3 = 3×f32 (12 B/vertex); ver 4 = 3×BF16; ver 5 = 3×F16 / IEEE
// half (6 B/vertex, widened to f32 client-side via a 64K LUT). F16's 10-bit mantissa
// avoids the BF16 staircase (~3 mm steps near the head). This decoder reads all three.
//
// Data: BodyParts3D, (c) The Database Center for Life Science, CC-BY 4.0.
import { useEffect, useMemo, useRef, useState } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

const PAGE_BG = 0x0a0e17;
const FMA_V3_CLASSID = 0x10000a01;

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

interface Material { id: number; name: string; doppler: string; rgb: [number, number, number] }

// bgz17 / Base17 "#17" palette → alpha: transparency quantized to 17 levels (0..16)/16.
const P17 = (lvl: number) => Math.max(0, Math.min(16, lvl)) / 16;
const MATERIAL_ALPHA_LEVEL: Record<string, number> = {
  low_resistance_artery: 13, high_resistance_artery: 13, portal_venous: 11,
  systemic_venous: 11, solid_tissue: 16, neural: 15,
};

const hexRgb = (h: string): [number, number, number] =>
  [parseInt(h.slice(1, 3), 16), parseInt(h.slice(3, 5), 16), parseInt(h.slice(5, 7), 16)];
const LAYER_RGB: Record<number, [number, number, number]> = Object.fromEntries(LAYERS.map((l) => [l.id, hexRgb(l.color)]));
const frac = (x: number) => x - Math.floor(x);
// deterministic per-concept colour: vessels keep the Doppler material rgb (artery red /
// vein blue); every other layer is tinted from its LAYER_RGB base (skeleton = bone, organ
// = warm, …) with a per-concept brightness + slight per-channel tilt so adjacent organs /
// bones read as distinct shades instead of one flat colour.
function conceptColor(layerId: number, matRgb: [number, number, number], row: number): [number, number, number] {
  if (layerId === 5) return matRgb;                       // vessels → Doppler material colour
  const base = LAYER_RGB[layerId] ?? [150, 150, 160];
  const h = frac(Math.sin(row * 12.9898) * 43758.5453);   // stable hash in [0,1)
  const bright = 0.82 + 0.34 * h;
  const tilt = (s: number) => 1 + 0.13 * (frac(Math.sin(row * s) * 9711.13) - 0.5) * 2;
  const out: [number, number, number] = [base[0] * bright * tilt(1.7), base[1] * bright * tilt(2.9), base[2] * bright * tilt(4.1)];
  return [Math.min(255, out[0]), Math.min(255, out[1]), Math.min(255, out[2])];
}

interface ConceptInfo { row: number; name: string; centroid: [number, number, number]; layer: number; material: string }

// 64K IEEE-half → f32 lookup (built once): ver-5 wire stores positions as F16
// (10-bit mantissa, ~0.2 mm here — no BF16 staircase). LUT keeps decode O(1)/vertex.
const HALF_LUT: Float32Array = (() => {
  const t = new Float32Array(65536);
  for (let h = 0; h < 65536; h++) {
    const s = (h & 0x8000) ? -1 : 1, e = (h & 0x7c00) >> 10, f = h & 0x03ff;
    t[h] = e === 0 ? s * Math.pow(2, -14) * (f / 1024)
      : e === 0x1f ? (f ? NaN : s * Infinity)
        : s * Math.pow(2, e - 15) * (1 + f / 1024);
  }
  return t;
})();

interface Decoded {
  nConcepts: number; nVerts: number; nTris: number; classid: number;
  positions: Float32Array; index: Uint32Array; opaqueTris: number;
  colors: Uint8Array; alpha: Float32Array; layer: Float32Array; row: Float32Array;
  materials: Material[]; labels: string[]; concepts: ConceptInfo[];
}
interface RenderState { enabled: Float32Array; alpha: number; transparent: boolean; lodOn: boolean; focus: { t: [number, number, number]; d: number } | null }

function decodeBso2(buf: ArrayBuffer): Decoded {
  const dv = new DataView(buf);
  const magic = String.fromCharCode(dv.getUint8(0), dv.getUint8(1), dv.getUint8(2), dv.getUint8(3));
  if (magic !== 'BSO2') throw new Error(`bad magic "${magic}" (expected BSO2)`);
  const ver = dv.getUint16(4, true);
  const posBytes = ver >= 4 ? 6 : 12;   // ver 4 (BF16) / ver 5 (F16) = 2 B/comp; ver 3 = f32
  const nC = dv.getUint32(6, true), nV = dv.getUint32(10, true), nT = dv.getUint32(14, true);
  let o = 18;
  const guidOff = o; o += 16 * nC;
  const matOff = o; o += nC;            // material codebook index u8
  const layerOff = o; o += nC;          // LAYER index u8 (1..8)
  const labOff = o; o += 4 * nC;        // label codebook index u32
  const cenOff = o; o += 12 * nC;       // per-concept centroid 3×f32 (search zoom + server LOD)
  o += 8 * nC;                          // (vstart,vcount)
  const posOff = o; o += posBytes * nV;
  o += 6 * nV;                          // helix (server-side)
  const rowOff = o; o += 4 * nV;
  const idxOff = o; o += 12 * nT;
  const labLen = dv.getUint32(o, true); o += 4;
  const labels: string[] = JSON.parse(new TextDecoder().decode(new Uint8Array(buf, o, labLen))); o += labLen;
  const matLen = dv.getUint32(o, true); o += 4;
  const materials: Material[] = JSON.parse(new TextDecoder().decode(new Uint8Array(buf, o, matLen)));

  const classid = dv.getUint32(guidOff, true);
  const cMat = new Uint8Array(buf.slice(matOff, matOff + nC));
  const cLayer = new Uint8Array(buf.slice(layerOff, layerOff + nC));
  const cNameIdx = new Uint32Array(buf.slice(labOff, labOff + 4 * nC));
  const cCen = new Float32Array(buf.slice(cenOff, cenOff + 12 * nC));  // bake-space; remap below

  // per-concept colour (precompute once) + searchable concept table (name + centroid).
  const conceptRgb: [number, number, number][] = new Array(nC);
  const concepts: ConceptInfo[] = new Array(nC);
  for (let cI = 0; cI < nC; cI++) {
    const mat = materials[cMat[cI]] ?? materials[materials.length - 1];
    const li = cLayer[cI] || 8;
    conceptRgb[cI] = conceptColor(li, mat.rgb, cI);
    concepts[cI] = {
      row: cI, name: labels[cNameIdx[cI]] ?? `concept ${cI}`, layer: li, material: mat.name,
      centroid: [-cCen[cI * 3], cCen[cI * 3 + 2], cCen[cI * 3 + 1]],  // (x,y,z)->(-x,z,y) display
    };
  }

  // pos → f32 working array. ver 5 = F16 (IEEE half) widened via the 64K LUT; ver 4 =
  // BF16 widened via bits<<16 (BF16 is the top 16 bits of an f32 — exact widening);
  // ver 3 = raw f32. buf.slice → a fresh 4-aligned buffer (posOff isn't guaranteed
  // 4-aligned, and a Float32Array *view* requires 4-alignment; slice copies + aligns).
  let srcPos: Float32Array;
  if (ver >= 5) {
    const hf = new Uint16Array(buf.slice(posOff, posOff + nV * 6));
    srcPos = new Float32Array(nV * 3);
    for (let k = 0; k < hf.length; k++) srcPos[k] = HALF_LUT[hf[k]];
  } else if (ver === 4) {
    const bf = new Uint16Array(buf.slice(posOff, posOff + nV * 6));
    const widened = new Uint32Array(nV * 3);
    for (let k = 0; k < bf.length; k++) widened[k] = bf[k] << 16;
    srcPos = new Float32Array(widened.buffer);
  } else {
    srcPos = new Float32Array(buf.slice(posOff, posOff + nV * 12));
  }
  const rowArr = new Uint32Array(buf.slice(rowOff, rowOff + 4 * nV));
  const positions = new Float32Array(nV * 3);
  const colors = new Uint8Array(nV * 3);
  const alpha = new Float32Array(nV);
  const layer = new Float32Array(nV);
  const row = new Float32Array(nV);             // concept index per vertex → server-LOD gate
  for (let i = 0; i < nV; i++) {
    positions[i * 3] = -srcPos[i * 3];               // (x,y,z)->(-x,z,y) head-up
    positions[i * 3 + 1] = srcPos[i * 3 + 2];
    positions[i * 3 + 2] = srcPos[i * 3 + 1];
    const r = rowArr[i];
    const m = materials[cMat[r]] ?? materials[materials.length - 1];
    const rgb = conceptRgb[r];
    colors[i * 3] = rgb[0]; colors[i * 3 + 1] = rgb[1]; colors[i * 3 + 2] = rgb[2];
    alpha[i] = P17(MATERIAL_ALPHA_LEVEL[m.name] ?? 16);
    layer[i] = cLayer[r] || 8;
    row[i] = r;
  }
  // partition triangles: opaque (all 3 verts α≈1 — solids) first, then transparent
  // (vessels/#17 α<1). Lets the renderer draw opaque fast (early-Z, no blend) and
  // only blend the ~translucent minority — ~2× fps without dropping any triangle.
  const raw = new Uint32Array(buf.slice(idxOff, idxOff + 12 * nT));
  const index = new Uint32Array(nT * 3);
  let op = 0, tr = nT * 3;
  for (let t = 0; t < nT; t++) {
    const a = t * 3, x = raw[a], y = raw[a + 1], z = raw[a + 2];
    const opaque = alpha[x] >= 0.999 && alpha[y] >= 0.999 && alpha[z] >= 0.999;
    if (opaque) { index[op] = x; index[op + 1] = y; index[op + 2] = z; op += 3; }
    else { tr -= 3; index[tr] = x; index[tr + 1] = y; index[tr + 2] = z; }
  }
  const opaqueTris = op / 3;
  return { nConcepts: nC, nVerts: nV, nTris: nT, classid, positions, index, opaqueTris, colors, alpha, layer, row, materials, labels, concepts };
}

const VERT = `
attribute vec3 aColor; attribute float aAlpha; attribute float aLayer; attribute float aRow;
varying vec3 vNormal; varying vec3 vColor; varying float vAlpha; varying float vLayer; varying float vRow;
void main(){ vNormal = normalMatrix * normal; vColor = aColor; vAlpha = aAlpha; vLayer = aLayer; vRow = aRow;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position,1.0); }`;
const FRAG = `
precision mediump float;
uniform float uEnabled[9]; uniform float uGlobalAlpha;
uniform sampler2D uLod; uniform float uLodN; uniform float uLodOn;  // server HHTL LOD gate
varying vec3 vNormal; varying vec3 vColor; varying float vAlpha; varying float vLayer; varying float vRow;
void main(){
  int li = int(vLayer + 0.5);
  if(li < 1 || li > 8 || uEnabled[li] < 0.5) discard;   // compartment gate
  if(uLodOn > 0.5){                                      // server LOD: HhtlAction 0=Reject ⇒ cull
    float act = texture2D(uLod, vec2((vRow + 0.5) / uLodN, 0.5)).r;
    if(act < 0.002) discard;                             // 0/255 = Reject
  }
  vec3 n = normalize(vNormal); if(!gl_FrontFacing) n = -n;
  const vec3 L = vec3(-0.401,0.783,0.476);
  float ndl = max(dot(n,L),0.0);
  float shade = min(0.34 + 0.20*(n.y*0.5+0.5) + 0.12*(-n.x*0.5+0.5) + 0.92*ndl, 1.3);
  gl_FragColor = vec4(vColor*shade, vAlpha * uGlobalAlpha);   // #17 alpha × solid/transparent
}`;

function mount(container: HTMLDivElement, d: Decoded, st: RenderState, onStats: (s: { fps: number }) => void): () => void {
  let w = container.clientWidth || window.innerWidth, h = container.clientHeight || window.innerHeight;
  const scene = new THREE.Scene(); scene.background = new THREE.Color(PAGE_BG);
  const camera = new THREE.PerspectiveCamera(45, w / h, 0.01, 100); camera.position.set(0, 0.05, 3.0);
  const renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setSize(w, h); renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  container.appendChild(renderer.domElement);

  const geom = new THREE.BufferGeometry();
  geom.setAttribute('position', new THREE.BufferAttribute(d.positions, 3));
  geom.setAttribute('aColor', new THREE.BufferAttribute(d.colors, 3, true));
  geom.setAttribute('aAlpha', new THREE.BufferAttribute(d.alpha, 1));
  geom.setAttribute('aLayer', new THREE.BufferAttribute(d.layer, 1));
  geom.setAttribute('aRow', new THREE.BufferAttribute(d.row, 1));
  geom.setIndex(new THREE.BufferAttribute(d.index, 1));
  geom.computeVertexNormals();

  // server-LOD action texture: 1 px per concept, init 255 (= show all). RGBA8 (not R8 /
  // RedFormat) so it's a complete, universally-supported sampler — an incomplete R8
  // texture samples as 0, which the shader reads as Reject ⇒ EVERYTHING discarded ⇒
  // black screen. Action byte lives in .r; nearest filter (no interpolation across
  // concept cells).
  const lodData = new Uint8Array(d.nConcepts * 4).fill(255);
  const lodTex = new THREE.DataTexture(lodData, d.nConcepts, 1, THREE.RGBAFormat, THREE.UnsignedByteType);
  lodTex.magFilter = THREE.NearestFilter; lodTex.minFilter = THREE.NearestFilter;
  lodTex.needsUpdate = true;
  // two draw groups over the partitioned index: [0]=opaque solids (fast),
  // [1]=translucent vessels (blended, drawn after).
  geom.clearGroups();
  geom.addGroup(0, d.opaqueTris * 3, 0);
  geom.addGroup(d.opaqueTris * 3, (d.nTris - d.opaqueTris) * 3, 1);
  const uniforms = {
    uEnabled: { value: st.enabled }, uGlobalAlpha: { value: st.alpha },
    uLod: { value: lodTex }, uLodN: { value: d.nConcepts }, uLodOn: { value: 0 },
  };
  // solid mode: opaque solids draw fast (transparent:false), #17 vessels blend over
  // them. transparent mode (port of /fma-body's uniform-uAlpha x-ray): BOTH groups go
  // translucent — uGlobalAlpha (0.42) drops the whole body to see-through, not just
  // the #17-alpha organs. depthWrite off when translucent so interior shows through.
  const opaqueMat = new THREE.ShaderMaterial({
    vertexShader: VERT, fragmentShader: FRAG, uniforms,
    side: THREE.DoubleSide, transparent: st.transparent, depthWrite: !st.transparent,
  });
  const transMat = new THREE.ShaderMaterial({
    vertexShader: VERT, fragmentShader: FRAG, uniforms,
    side: THREE.DoubleSide, transparent: true, depthWrite: false,
  });
  scene.add(new THREE.Mesh(geom, [opaqueMat, transMat]));

  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true; controls.dampingFactor = 0.08;
  controls.autoRotate = true; controls.autoRotateSpeed = 0.6;
  controls.minDistance = 0.6; controls.maxDistance = 12;

  // throttled server-LOD poll: post the live camera (in display space — block bounds
  // are baked in the same space), write the per-concept action bytes into lodTex.
  let lodNext = 0, lodInflight = false, lodFail = false, lodReady = false, lodWarned = false;
  const postLod = (now: number) => {
    if (!st.lodOn || lodFail || lodInflight || now < lodNext) return;
    lodInflight = true; lodNext = now + 220;
    camera.updateMatrixWorld();
    const e = camera.matrixWorldInverse.elements;   // column-major → row-major view
    const view = [
      [e[0], e[4], e[8], e[12]], [e[1], e[5], e[9], e[13]],
      [e[2], e[6], e[10], e[14]], [e[3], e[7], e[11], e[15]],
    ];
    const fy = (h / 2) / Math.tan((camera.fov * Math.PI) / 360);
    const body = {
      view, fx: fy, fy, cx: w / 2, cy: h / 2, near: camera.near, far: camera.far,
      width: w, height: h, position: [camera.position.x, camera.position.y, camera.position.z],
    };
    fetch('/api/body/lod', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
      .then((j: { actions: number[]; n_concepts?: number; tally?: number[] }) => {
        const a = j.actions;
        const visible = (j.n_concepts ?? a.length) - (j.tally?.[0] ?? 0);
        // all (or nearly all) Reject ⇒ the cascade culled the whole body, which only
        // happens if the camera mapping is off. Don't black out — show everything.
        const degenerate = visible <= Math.max(1, a.length * 0.02);
        if (degenerate && !lodWarned) { console.warn('[body LOD] cascade rejected ~all concepts — showing all (camera mapping suspect)'); lodWarned = true; }
        for (let i = 0; i < d.nConcepts && i < a.length; i++) lodData[i * 4] = degenerate ? 255 : a[i];
        lodReady = true;
        lodTex.needsUpdate = true;
      })
      .catch(() => { lodFail = true; })   // endpoint absent (old deploy) → silently keep full render
      .finally(() => { lodInflight = false; });
  };

  let raf = 0, ema = 16.6, last = performance.now(), since = 0, wasT = st.transparent;
  const tmp = new THREE.Vector3();
  const tick = () => {
    raf = requestAnimationFrame(tick);
    const now = performance.now(); ema = ema * 0.9 + (now - last) * 0.1; last = now;
    const pr = ema > 30 ? 1 : Math.min(window.devicePixelRatio, 2);
    if (renderer.getPixelRatio() !== pr) renderer.setPixelRatio(pr);
    uniforms.uEnabled.value = st.enabled;          // shared by both materials
    uniforms.uGlobalAlpha.value = st.alpha;
    uniforms.uLodOn.value = st.lodOn && !lodFail && lodReady ? 1 : 0;   // only cull after a real response
    if (st.focus) {                                 // search-pick zoom: glide to the organ
      tmp.set(st.focus.t[0], st.focus.t[1], st.focus.t[2]);
      controls.target.lerp(tmp, 0.12);
      const dir = camera.position.clone().sub(controls.target).normalize();
      camera.position.lerp(tmp.clone().add(dir.multiplyScalar(st.focus.d)), 0.12);
    }
    if (st.transparent !== wasT) {
      // flip the solid group into the x-ray (whole-body translucent) and back
      opaqueMat.transparent = st.transparent; opaqueMat.depthWrite = !st.transparent; opaqueMat.needsUpdate = true;
      transMat.depthWrite = !st.transparent;
      wasT = st.transparent;
    }
    postLod(now);
    controls.update(); renderer.render(scene, camera);
    if (++since >= 20) { since = 0; onStats({ fps: Math.round(1000 / Math.max(ema, 1)) }); }
  };
  tick();
  const onResize = () => {
    w = container.clientWidth || window.innerWidth; h = container.clientHeight || window.innerHeight;
    camera.aspect = w / h; camera.updateProjectionMatrix(); renderer.setSize(w, h);
  };
  const ro = new ResizeObserver(onResize); ro.observe(container);
  return () => {
    cancelAnimationFrame(raf); ro.disconnect(); controls.dispose();
    geom.dispose(); lodTex.dispose(); opaqueMat.dispose(); transMat.dispose(); renderer.dispose();
    if (renderer.domElement.parentNode === container) container.removeChild(renderer.domElement);
  };
}

const REL = 'https://github.com/AdaWorldAPI/q2/releases/download/fma-body-soa-v3-v1';

export function BodyV3() {
  const ref = useRef<HTMLDivElement>(null);
  const [d, setD] = useState<Decoded | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [stats, setStats] = useState<{ fps: number } | null>(null);
  // skin (1) off by default so the anatomy shows.
  const [on, setOn] = useState<Record<number, boolean>>({ 1: false, 2: true, 3: true, 4: true, 5: true, 6: true, 7: true, 8: true });
  const [transparent, setTransparent] = useState(false);   // default solid (like /fma-body)
  const [lod, setLod] = useState(false);   // server HHTL LOD — opt-in (off = today's full render)
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<ConceptInfo | null>(null);
  const stRef = useRef<RenderState>({ enabled: new Float32Array([0, 0, 1, 1, 1, 1, 1, 1, 1]), alpha: 1, transparent: false, lodOn: false, focus: null });

  useEffect(() => {
    const e = new Float32Array(9);
    for (let i = 1; i <= 8; i++) e[i] = on[i] ? 1 : 0;
    stRef.current.enabled = e;
    stRef.current.transparent = transparent;
    // /fma-body translucency model: one uniform alpha for the WHOLE body. transparent
    // ⇒ 0.42 x-ray (see through skin/muscle to organs); solid ⇒ 1.0 (#17 vessels only).
    stRef.current.alpha = transparent ? 0.42 : 1.0;
    stRef.current.lodOn = lod;
  }, [on, transparent, lod]);

  useEffect(() => {
    let cancelled = false;
    const inflate = async (r: Response) =>
      r.body && typeof DecompressionStream !== 'undefined'
        ? new Response(r.body.pipeThrough(new DecompressionStream('gzip'))).arrayBuffer()
        : r.arrayBuffer();
    (async () => {
      const local = await fetch('/body.soa.gz').catch(() => null);
      if (local && local.ok) return decodeBso2(await inflate(local));
      const rel = await fetch(`${REL}/body.soa.gz`);
      if (!rel.ok) throw new Error(`HTTP ${rel.status} fetching body.soa.gz`);
      return decodeBso2(await inflate(rel));
    })().then((x) => { if (!cancelled) setD(x); }).catch((e) => { if (!cancelled) setError(String(e)); });
    return () => { cancelled = true; };
  }, []);

  useEffect(() => { const c = ref.current; if (!c || !d) return; return mount(c, d, stRef.current, setStats); }, [d]);

  // search the label codebook (concept names) → ranked matches.
  const matches = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q || !d) return [] as ConceptInfo[];
    const hits = d.concepts.filter((c) => c.name.toLowerCase().includes(q));
    hits.sort((a, b) => a.name.length - b.name.length);   // shortest (closest) name first
    return hits.slice(0, 14);
  }, [query, d]);

  function pick(c: ConceptInfo) {
    setSelected(c);
    stRef.current.focus = { t: c.centroid, d: 0.6 };   // glide the camera to the organ
  }

  const btn = (active: boolean): React.CSSProperties => ({
    padding: '5px 11px', borderRadius: 6, border: `1px solid ${active ? '#5a7fa8' : '#2a3242'}`,
    background: active ? '#16202e' : '#0e1219', color: active ? '#cdd9e5' : '#6a7686',
    font: '12px ui-monospace, monospace', cursor: 'pointer',
  });

  return (
    <div style={{ position: 'fixed', inset: 0, background: '#0a0e17', overflow: 'hidden' }}>
      <div ref={ref} style={{ position: 'absolute', inset: 0 }} />
      <div style={{ position: 'absolute', top: 12, left: 16, color: '#cdd9e5', font: '13px ui-monospace, monospace', pointerEvents: 'none' }}>
        <div style={{ fontSize: 15, color: '#fff' }}>FMA body · SoA · Gouraud · compartments</div>
        <div style={{ opacity: 0.7 }}>
          {d ? `${d.nTris.toLocaleString()} triangles · ${d.nVerts.toLocaleString()} vertices — ALL points, drag to orbit`
            : error ? '' : 'loading body.soa (BSO2)…'}
        </div>
        {d && (
          <div style={{ opacity: 0.6, marginTop: 2 }}>
            {d.nConcepts.toLocaleString()} concepts ·{' '}
            {d.classid === FMA_V3_CLASSID
              ? <span style={{ color: '#7fdca0' }}>classid 0x{d.classid.toString(16)} ✓ V3 (part_of:is_a)</span>
              : <span style={{ color: '#ff8095' }}>classid 0x{d.classid.toString(16)}</span>}
          </div>
        )}
        {stats && <div style={{ opacity: 0.5, marginTop: 2 }}>{stats.fps} fps · {transparent ? 'x-ray (whole body 0.42)' : 'solid · #17 vessels'}</div>}
      </div>

      {/* search the label codebook + detail side window (left) */}
      {d && (
        <div style={{ position: 'absolute', top: 92, left: 16, width: 320, font: '13px ui-monospace, monospace' }}>
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={`search ${d.nConcepts.toLocaleString()} labels — e.g. "liver"`}
            style={{ width: '100%', boxSizing: 'border-box', padding: '7px 9px', borderRadius: 6, border: '1px solid #2a3242', background: '#0e1219', color: '#cdd9e5', font: '13px ui-monospace, monospace' }}
          />
          {matches.length > 0 && (
            <div style={{ marginTop: 4, background: '#0e1219', border: '1px solid #1c2530', borderRadius: 6, overflow: 'hidden', maxHeight: 320, overflowY: 'auto' }}>
              {matches.map((c) => (
                <div key={c.row} onClick={() => { pick(c); setQuery(''); }} style={{ padding: '6px 9px', cursor: 'pointer', display: 'flex', justifyContent: 'space-between', gap: 8, borderBottom: '1px solid #141b24', color: '#cdd9e5' }}>
                  <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{c.name}</span>
                  <span style={{ opacity: 0.5, flexShrink: 0 }}>{LAYERS[(c.layer - 1) % 8]?.name}</span>
                </div>
              ))}
            </div>
          )}
          {selected && (
            <div style={{ marginTop: 10, background: '#0e1219', border: '1px solid #233', borderRadius: 8, padding: 12, color: '#cdd9e5' }}>
              <div style={{ color: '#fff', fontSize: 14, marginBottom: 6 }}>{selected.name}</div>
              <div style={{ display: 'grid', gridTemplateColumns: 'auto 1fr', gap: '3px 10px', fontSize: 12, opacity: 0.85 }}>
                <span style={{ opacity: 0.6 }}>compartment</span>
                <span><span style={{ display: 'inline-block', width: 8, height: 8, borderRadius: 4, background: LAYERS[(selected.layer - 1) % 8]?.color, marginRight: 6, verticalAlign: 'middle' }} />{LAYERS[(selected.layer - 1) % 8]?.name}</span>
                <span style={{ opacity: 0.6 }}>material</span>
                <span>{selected.material.replace(/_/g, ' ')}</span>
                <span style={{ opacity: 0.6 }}>row</span>
                <span>#{selected.row}</span>
                <span style={{ opacity: 0.6 }}>centroid</span>
                <span>{selected.centroid.map((v) => v.toFixed(2)).join(', ')}</span>
              </div>
              <div style={{ marginTop: 10, display: 'flex', gap: 6 }}>
                <button style={btn(false)} onClick={() => { stRef.current.focus = { t: selected.centroid, d: 0.6 }; }}>re-center</button>
                <button style={btn(false)} onClick={() => { setSelected(null); stRef.current.focus = null; }}>close</button>
              </div>
            </div>
          )}
        </div>
      )}

      {error && (
        <div style={{ position: 'absolute', top: '46%', width: '100%', textAlign: 'center', color: '#ff8095', font: '13px ui-monospace, monospace' }}>
          {error}
        </div>
      )}

      {/* 8-compartment toggles + solid/transparent (right) */}
      <div style={{ position: 'absolute', top: 12, right: 16, display: 'flex', flexDirection: 'column', gap: 8, alignItems: 'flex-end' }}>
        <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', justifyContent: 'flex-end', maxWidth: 360 }}>
          {LAYERS.map((l) => (
            <button key={l.id} style={btn(on[l.id])} onClick={() => setOn((p) => ({ ...p, [l.id]: !p[l.id] }))}>
              <span style={{ display: 'inline-block', width: 8, height: 8, borderRadius: 4, background: l.color, marginRight: 6, verticalAlign: 'middle' }} />
              {l.name}
            </button>
          ))}
        </div>
        <button style={btn(transparent)} onClick={() => setTransparent((v) => !v)}>{transparent ? 'x-ray' : 'solid'} ⇄</button>
        <button style={btn(lod)} onClick={() => setLod((v) => !v)} title="server HHTL depth-cascade: culls off-frustum concepts when zoomed in">server LOD {lod ? 'on' : 'off'}</button>
        <a href="/fma-body" style={{ color: '#7fa6c4', textDecoration: 'none', font: '12px ui-monospace, monospace' }}>2k layered →</a>
      </div>

      <div style={{ position: 'absolute', bottom: 10, left: 16, color: '#5a6b7e', font: '10px ui-monospace, monospace', maxWidth: '70%', pointerEvents: 'none' }}>
        BodyParts3D, (c) The Database Center for Life Science, licensed under CC Attribution 4.0 International
      </div>
    </div>
  );
}
