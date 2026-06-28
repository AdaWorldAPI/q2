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
// pos column: ver 3 = 3×f32 (12 B/vertex); ver 4 = 3×BF16 (6 B/vertex, widened
// to f32 client-side by bits<<16). This decoder reads both.
//
// Data: BodyParts3D, (c) The Database Center for Life Science, CC-BY 4.0.
import { useEffect, useRef, useState } from 'react';
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

interface Decoded {
  nConcepts: number; nVerts: number; nTris: number; classid: number;
  positions: Float32Array; index: Uint32Array; opaqueTris: number;
  colors: Uint8Array; alpha: Float32Array; layer: Float32Array;
  materials: Material[]; labels: string[];
}
interface RenderState { enabled: Float32Array; alpha: number; transparent: boolean }

function decodeBso2(buf: ArrayBuffer): Decoded {
  const dv = new DataView(buf);
  const magic = String.fromCharCode(dv.getUint8(0), dv.getUint8(1), dv.getUint8(2), dv.getUint8(3));
  if (magic !== 'BSO2') throw new Error(`bad magic "${magic}" (expected BSO2)`);
  const ver = dv.getUint16(4, true);
  const posBf16 = ver >= 4;             // ver 4: pos is BF16 (6 B/vertex) not f32 (12 B)
  const posBytes = posBf16 ? 6 : 12;
  const nC = dv.getUint32(6, true), nV = dv.getUint32(10, true), nT = dv.getUint32(14, true);
  let o = 18;
  const guidOff = o; o += 16 * nC;
  const matOff = o; o += nC;            // material codebook index u8
  const layerOff = o; o += nC;          // LAYER index u8 (1..8)
  const labOff = o; o += 4 * nC;        // label codebook index u32
  o += 12 * nC;                         // centroid (server LOD)
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

  // pos → f32 working array. ver 4 stores BF16 (u16): widen via bits<<16 (BF16 is
  // the top 16 bits of an IEEE-754 f32, so a left-shift into the high half is the
  // exact, lossless widening — 8-bit mantissa, no rounding back). ver 3 is raw f32;
  // buf.slice → a fresh 4-aligned buffer (posOff isn't guaranteed 4-aligned, and a
  // Float32Array *view* requires 4-alignment; slice copies but always aligns).
  let srcPos: Float32Array;
  if (posBf16) {
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
  for (let i = 0; i < nV; i++) {
    positions[i * 3] = -srcPos[i * 3];               // (x,y,z)->(-x,z,y) head-up
    positions[i * 3 + 1] = srcPos[i * 3 + 2];
    positions[i * 3 + 2] = srcPos[i * 3 + 1];
    const row = rowArr[i];
    const m = materials[cMat[row]] ?? materials[materials.length - 1];
    colors[i * 3] = m.rgb[0]; colors[i * 3 + 1] = m.rgb[1]; colors[i * 3 + 2] = m.rgb[2];
    alpha[i] = P17(MATERIAL_ALPHA_LEVEL[m.name] ?? 16);
    layer[i] = cLayer[row] || 8;
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
  return { nConcepts: nC, nVerts: nV, nTris: nT, classid, positions, index, opaqueTris, colors, alpha, layer, materials, labels };
}

const VERT = `
attribute vec3 aColor; attribute float aAlpha; attribute float aLayer;
varying vec3 vNormal; varying vec3 vColor; varying float vAlpha; varying float vLayer;
void main(){ vNormal = normalMatrix * normal; vColor = aColor; vAlpha = aAlpha; vLayer = aLayer;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position,1.0); }`;
const FRAG = `
precision mediump float;
uniform float uEnabled[9]; uniform float uGlobalAlpha;
varying vec3 vNormal; varying vec3 vColor; varying float vAlpha; varying float vLayer;
void main(){
  int li = int(vLayer + 0.5);
  if(li < 1 || li > 8 || uEnabled[li] < 0.5) discard;   // compartment gate
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
  geom.setIndex(new THREE.BufferAttribute(d.index, 1));
  geom.computeVertexNormals();
  // two draw groups over the partitioned index: [0]=opaque solids (fast),
  // [1]=translucent vessels (blended, drawn after).
  geom.clearGroups();
  geom.addGroup(0, d.opaqueTris * 3, 0);
  geom.addGroup(d.opaqueTris * 3, (d.nTris - d.opaqueTris) * 3, 1);
  const uniforms = { uEnabled: { value: st.enabled }, uGlobalAlpha: { value: st.alpha } };
  const opaqueMat = new THREE.ShaderMaterial({
    vertexShader: VERT, fragmentShader: FRAG, uniforms,
    side: THREE.DoubleSide, transparent: false, depthWrite: true,
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

  let raf = 0, ema = 16.6, last = performance.now(), since = 0, wasT = st.transparent;
  const tick = () => {
    raf = requestAnimationFrame(tick);
    const now = performance.now(); ema = ema * 0.9 + (now - last) * 0.1; last = now;
    const pr = ema > 30 ? 1 : Math.min(window.devicePixelRatio, 2);
    if (renderer.getPixelRatio() !== pr) renderer.setPixelRatio(pr);
    uniforms.uEnabled.value = st.enabled;          // shared by both materials
    uniforms.uGlobalAlpha.value = st.alpha;
    if (st.transparent !== wasT) { transMat.depthWrite = !st.transparent; wasT = st.transparent; }
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
    geom.dispose(); opaqueMat.dispose(); transMat.dispose(); renderer.dispose();
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
  const [transparent, setTransparent] = useState(true);
  const stRef = useRef<RenderState>({ enabled: new Float32Array([0, 0, 1, 1, 1, 1, 1, 1, 1]), alpha: 1, transparent: true });

  useEffect(() => {
    const e = new Float32Array(9);
    for (let i = 1; i <= 8; i++) e[i] = on[i] ? 1 : 0;
    stRef.current.enabled = e;
    stRef.current.transparent = transparent;
    stRef.current.alpha = transparent ? 1.0 : 1.0; // alpha comes from #17 material; toggle flips depthWrite
  }, [on, transparent]);

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
        {stats && <div style={{ opacity: 0.5, marginTop: 2 }}>{stats.fps} fps · vessels translucent (#17 alpha)</div>}
      </div>

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
        <button style={btn(transparent)} onClick={() => setTransparent((v) => !v)}>{transparent ? 'translucent' : 'solid'} ⇄</button>
        <a href="/fma-body" style={{ color: '#7fa6c4', textDecoration: 'none', font: '12px ui-monospace, monospace' }}>2k layered →</a>
      </div>

      <div style={{ position: 'absolute', bottom: 10, left: 16, color: '#5a6b7e', font: '10px ui-monospace, monospace', maxWidth: '70%', pointerEvents: 'none' }}>
        BodyParts3D, (c) The Database Center for Life Science, licensed under CC Attribution 4.0 International
      </div>
    </div>
  );
}
