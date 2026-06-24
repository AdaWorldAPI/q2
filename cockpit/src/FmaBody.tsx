// /fma-body — MY full-body FMA viewer (additive to the other session's /torso*).
//
// Renders cockpit/public/fma_body.mesh (baked by `fma`'s cockpit_bake) — the SPM1 wire
// the cockpit decodes, with the per-vertex `opacity` byte = a LAYER id (1 skin · 2 muscle ·
// 3 organ · 4 skeleton · 5 vessel · 6 nerve · 7 connective · 8 other). Layers toggle via
// buttons; solid↔transparent switches; a SEARCH BAR zooms the camera to any organ
// (centroids from the per-vertex node_row + fma_body.mesh.nodes.json), and a CLINICAL NARS
// panel posts {organ, condition, medication, labs} to the real q2 NARS engine
// (POST /api/clinical/reason → lance_graph_planner TruthValue::deduction) and shows the
// inferred risk chains. The reasoning is a DEMO over a small rule KB — NOT medical advice.
//
// Does not touch /torso, /torso-live, /torso-splat, /torso-map (#57/#58).
// Geometry/data: BodyParts3D, (c) The Database Center for Life Science, CC-BY 4.0.
import { useEffect, useMemo, useRef, useState } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

const PAGE_BG = 0x0a0e17;

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

// clickable demo scenarios for the NARS panel (mock dataset).
const SCENARIOS = [
  { label: 'Cirrhotic liver · acetaminophen', organ: 'liver', condition: 'cirrhosis', medication: 'acetaminophen', labs: [{ name: 'inr', flag: 'high' }, { name: 'bilirubin', flag: 'high' }] },
  { label: 'Cirrhosis · warfarin', organ: 'liver', condition: 'cirrhosis', medication: 'warfarin', labs: [{ name: 'inr', flag: 'high' }, { name: 'platelets', flag: 'low' }] },
  { label: 'CKD kidney · ibuprofen', organ: 'kidney', condition: 'ckd', medication: 'ibuprofen', labs: [{ name: 'creatinine', flag: 'high' }, { name: 'egfr', flag: 'low' }] },
  { label: 'Heart failure · NSAID', organ: 'heart', condition: 'heart_failure', medication: 'ibuprofen', labs: [{ name: 'creatinine', flag: 'high' }] },
];

interface Mesh {
  vertCount: number;
  triCount: number;
  positions: Float32Array; // decoded (-x,z,y) — world space
  normals: Float32Array;
  colors: Uint8Array;
  layer: Float32Array;
  nodeRow: Uint16Array;
  index: Uint32Array;
}

interface NodeInfo { row: number; fma: string; name: string; tissue: string }

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
  const layer = new Float32Array(vertCount);
  const nodeRow = new Uint16Array(vertCount);
  for (let i = 0; i < vertCount; i++) {
    const b = voff + i * 21;
    const x = dv.getFloat32(b, true), y = dv.getFloat32(b + 4, true), z = dv.getFloat32(b + 8, true);
    positions[i * 3] = -x; positions[i * 3 + 1] = z; positions[i * 3 + 2] = y; // (-x,z,y) head-up
    normals[i * 3] = -dv.getInt8(b + 12) / 127;
    normals[i * 3 + 1] = dv.getInt8(b + 14) / 127;
    normals[i * 3 + 2] = dv.getInt8(b + 13) / 127;
    colors[i * 3] = dv.getUint8(b + 15);
    colors[i * 3 + 1] = dv.getUint8(b + 16);
    colors[i * 3 + 2] = dv.getUint8(b + 17);
    layer[i] = dv.getUint8(b + 18);
    nodeRow[i] = dv.getUint16(b + 19, true);
  }
  const ioff = voff + vertCount * 21;
  const index = new Uint32Array(triCount * 3);
  for (let t = 0; t < triCount; t++) {
    const b = ioff + t * 12;
    index[t * 3] = dv.getUint32(b, true);
    index[t * 3 + 1] = dv.getUint32(b + 4, true);
    index[t * 3 + 2] = dv.getUint32(b + 8, true);
  }
  return { vertCount, triCount, positions, normals, colors, layer, nodeRow, index };
}

const VERT = `
attribute vec3 aNormal;
attribute vec3 aColor;
attribute float aLayer;
varying vec3 vNormal;
varying vec3 vColor;
varying float vLayer;
void main() {
  vNormal = aNormal; vColor = aColor; vLayer = aLayer;
  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
}`;
const FRAG = `
precision mediump float;
uniform float uEnabled[9];
uniform float uAlpha;
varying vec3 vNormal;
varying vec3 vColor;
varying float vLayer;
void main() {
  int li = int(vLayer + 0.5);
  if (li < 0 || li > 8 || uEnabled[li] < 0.5) discard;
  vec3 n = normalize(vNormal);
  if (!gl_FrontFacing) n = -n;
  const vec3 L = vec3(-0.401, 0.783, 0.476);
  float ndl = max(dot(n, L), 0.0);
  float hemi = 0.34 + 0.20 * (n.y * 0.5 + 0.5);
  float fill = 0.12 * (-n.x * 0.5 + 0.5);
  float shade = min(hemi + fill + 0.92 * ndl, 1.3);
  gl_FragColor = vec4(vColor * shade, uAlpha);
}`;

interface RenderState {
  enabled: Float32Array;
  alpha: number;
  transparent: boolean;
  focus: { t: [number, number, number]; d: number } | null; // zoom target
}

function mount(container: HTMLDivElement, mesh: Mesh, st: RenderState, onStats: (s: { fps: number }) => void): () => void {
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
  geom.setAttribute('aColor', new THREE.BufferAttribute(mesh.colors, 3, true));
  geom.setAttribute('aLayer', new THREE.BufferAttribute(mesh.layer, 1));
  geom.setIndex(new THREE.BufferAttribute(mesh.index, 1));
  const mat = new THREE.ShaderMaterial({
    vertexShader: VERT,
    fragmentShader: FRAG,
    uniforms: { uEnabled: { value: st.enabled }, uAlpha: { value: st.alpha } },
    side: THREE.DoubleSide,
    transparent: st.transparent,
    depthWrite: !st.transparent,
  });
  scene.add(new THREE.Mesh(geom, mat));

  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.autoRotate = true;
  controls.autoRotateSpeed = 0.5;
  controls.minDistance = 0.3;
  controls.maxDistance = 12;

  let raf = 0, ema = 16.6, last = performance.now(), sinceStat = 0, wasT = st.transparent;
  const tmp = new THREE.Vector3();
  const tick = () => {
    raf = requestAnimationFrame(tick);
    const now = performance.now();
    ema = ema * 0.9 + (now - last) * 0.1;
    last = now;
    const pr = ema > 30 ? 1 : Math.min(window.devicePixelRatio, 2);
    if (renderer.getPixelRatio() !== pr) renderer.setPixelRatio(pr);
    mat.uniforms.uEnabled.value = st.enabled;
    mat.uniforms.uAlpha.value = st.alpha;
    if (st.transparent !== wasT) { mat.transparent = st.transparent; mat.depthWrite = !st.transparent; mat.needsUpdate = true; wasT = st.transparent; }
    if (st.focus) {
      tmp.set(st.focus.t[0], st.focus.t[1], st.focus.t[2]);
      controls.target.lerp(tmp, 0.12);
      const dir = camera.position.clone().sub(controls.target).normalize();
      camera.position.lerp(tmp.clone().add(dir.multiplyScalar(st.focus.d)), 0.12);
    }
    controls.update();
    renderer.render(scene, camera);
    if (++sinceStat >= 20) { sinceStat = 0; onStats({ fps: Math.round(1000 / Math.max(ema, 1)) }); }
  };
  tick();

  const onResize = () => {
    w = container.clientWidth || window.innerWidth;
    h = container.clientHeight || window.innerHeight;
    camera.aspect = w / h; camera.updateProjectionMatrix(); renderer.setSize(w, h);
  };
  const ro = new ResizeObserver(onResize);
  ro.observe(container);
  return () => {
    cancelAnimationFrame(raf); ro.disconnect(); controls.dispose(); geom.dispose(); mat.dispose(); renderer.dispose();
    if (renderer.domElement.parentNode === container) container.removeChild(renderer.domElement);
  };
}

export function FmaBody() {
  const ref = useRef<HTMLDivElement>(null);
  const [mesh, setMesh] = useState<Mesh | null>(null);
  const [nodes, setNodes] = useState<NodeInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [stats, setStats] = useState<{ fps: number } | null>(null);
  const [on, setOn] = useState<Record<number, boolean>>({ 1: false, 2: true, 3: true, 4: true, 5: true, 6: true, 7: true, 8: true });
  const [transparent, setTransparent] = useState(false);
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<NodeInfo | null>(null);
  const [reason, setReason] = useState<{ summary: string[]; inferences: { source: string; target: string; truth_f: number; truth_c: number; inference_type: string; via: string[] }[]; disclaimer: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const stRef = useRef<RenderState>({ enabled: new Float32Array([0, 0, 1, 1, 1, 1, 1, 1, 1]), alpha: 1, transparent: false, focus: null });

  // per-node centroid in decoded world space (group vertices by node_row).
  const rowCentroid = useMemo(() => {
    const m = new Map<number, [number, number, number]>();
    if (!mesh) return m;
    const sum = new Map<number, [number, number, number, number]>();
    for (let i = 0; i < mesh.vertCount; i++) {
      const r = mesh.nodeRow[i];
      const a = sum.get(r) ?? [0, 0, 0, 0];
      a[0] += mesh.positions[i * 3]; a[1] += mesh.positions[i * 3 + 1]; a[2] += mesh.positions[i * 3 + 2]; a[3]++;
      sum.set(r, a);
    }
    for (const [r, a] of sum) if (a[3]) m.set(r, [a[0] / a[3], a[1] / a[3], a[2] / a[3]]);
    return m;
  }, [mesh]);

  useEffect(() => {
    const e = new Float32Array(9);
    for (let i = 1; i <= 8; i++) e[i] = on[i] ? 1 : 0;
    stRef.current.enabled = e;
    stRef.current.transparent = transparent;
    stRef.current.alpha = transparent ? 0.42 : 1.0;
  }, [on, transparent]);

  useEffect(() => {
    let cancelled = false;
    fetch('/fma_body.mesh').then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status} fetching fma_body.mesh`); return r.arrayBuffer(); })
      .then((buf) => { if (!cancelled) setMesh(decodeSpm1(buf)); }).catch((e) => { if (!cancelled) setError(String(e)); });
    fetch('/fma_body.mesh.nodes.json').then((r) => (r.ok ? r.json() : { nodes: [] }))
      .then((d) => { if (!cancelled) setNodes(d.nodes ?? []); }).catch(() => {});
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    const container = ref.current;
    if (!container || !mesh) return;
    return mount(container, mesh, stRef.current, setStats);
  }, [mesh]);

  const matches = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    return nodes.filter((n) => n.name.toLowerCase().includes(q)).slice(0, 12);
  }, [query, nodes]);

  function pick(n: NodeInfo) {
    setSelected(n);
    setReason(null);
    const c = rowCentroid.get(n.row);
    if (c) stRef.current.focus = { t: c, d: 0.9 };
  }

  async function runReason(sc: { organ: string; condition: string; medication: string; labs: { name: string; flag: string }[] }) {
    setBusy(true);
    setReason(null);
    try {
      const r = await fetch('/api/clinical/reason', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(sc) });
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      setReason(await r.json());
    } catch (e) {
      setReason({ summary: [`reasoning endpoint unavailable (${e}) — needs the cockpit-server backend deployed`], inferences: [], disclaimer: 'NARS demo — not medical advice.' });
    } finally {
      setBusy(false);
    }
  }

  const btn = (active: boolean): React.CSSProperties => ({
    padding: '5px 11px', borderRadius: 6, border: `1px solid ${active ? '#5a7fa8' : '#2a3242'}`,
    background: active ? '#16202e' : '#0e1219', color: active ? '#cdd9e5' : '#6a7686',
    font: '12px ui-monospace, monospace', cursor: 'pointer',
  });

  return (
    <div style={{ position: 'fixed', inset: 0, background: '#0a0e17', overflow: 'hidden' }}>
      <div ref={ref} style={{ position: 'absolute', inset: 0 }} />

      {/* search + reasoning panel (left) */}
      <div style={{ position: 'absolute', top: 12, left: 16, width: 340, color: '#cdd9e5', font: '13px ui-monospace, monospace' }}>
        <div style={{ fontSize: 15, color: '#fff', marginBottom: 6 }}>FMA body · (place:tissue) layers</div>
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={`search ${nodes.length} organs/parts — e.g. "liver"`}
          style={{ width: '100%', boxSizing: 'border-box', padding: '7px 9px', borderRadius: 6, border: '1px solid #2a3242', background: '#0e1219', color: '#cdd9e5', font: '13px ui-monospace, monospace' }}
        />
        {matches.length > 0 && (
          <div style={{ marginTop: 4, background: '#0e1219', border: '1px solid #1c2530', borderRadius: 6, overflow: 'hidden' }}>
            {matches.map((n) => (
              <div key={n.row} onClick={() => { pick(n); setQuery(''); }} style={{ padding: '6px 9px', cursor: 'pointer', display: 'flex', justifyContent: 'space-between', borderBottom: '1px solid #141b24' }}>
                <span>{n.name}</span><span style={{ opacity: 0.5 }}>{n.tissue}</span>
              </div>
            ))}
          </div>
        )}

        {selected && (
          <div style={{ marginTop: 10, background: '#0e1219', border: '1px solid #233', borderRadius: 8, padding: 12 }}>
            <div style={{ color: '#fff', marginBottom: 2 }}>{selected.name} <span style={{ opacity: 0.5 }}>· {selected.tissue}</span></div>
            <div style={{ opacity: 0.6, fontSize: 11, marginBottom: 8 }}>NARS clinical reasoning (demo) — pick a scenario:</div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
              {SCENARIOS.map((s) => (
                <button key={s.label} style={btn(false)} disabled={busy} onClick={() => runReason(s)}>
                  {s.label}
                </button>
              ))}
            </div>
            {busy && <div style={{ opacity: 0.6, marginTop: 8 }}>reasoning…</div>}
            {reason && (
              <div style={{ marginTop: 10 }}>
                {reason.summary.map((s, i) => (
                  <div key={i} style={{ color: '#ffb86b', fontSize: 12, marginBottom: 3 }}>▸ {s}</div>
                ))}
                {reason.inferences.filter((x) => x.inference_type === 'Deduction').slice(0, 6).map((x, i) => (
                  <div key={i} style={{ opacity: 0.7, fontSize: 11 }}>
                    {x.source} → {x.target} <span style={{ color: '#6db3ff' }}>f={x.truth_f.toFixed(2)} c={x.truth_c.toFixed(2)}</span>
                  </div>
                ))}
                <div style={{ opacity: 0.4, fontSize: 10, marginTop: 8, borderTop: '1px solid #1c2530', paddingTop: 6 }}>{reason.disclaimer}</div>
              </div>
            )}
            <div style={{ marginTop: 8 }}><button style={btn(false)} onClick={() => { setSelected(null); setReason(null); }}>close</button></div>
          </div>
        )}
        {stats && !selected && <div style={{ opacity: 0.5, marginTop: 6, fontSize: 11 }}>{stats.fps} fps · search zooms to the organ; layers gated by the converged key</div>}
      </div>

      {/* layer toggles + solid/transparent (right) */}
      <div style={{ position: 'absolute', top: 12, right: 16, display: 'flex', flexDirection: 'column', gap: 8, alignItems: 'flex-end' }}>
        <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', justifyContent: 'flex-end', maxWidth: 360 }}>
          {LAYERS.map((l) => (
            <button key={l.id} style={btn(on[l.id])} onClick={() => setOn((p) => ({ ...p, [l.id]: !p[l.id] }))}>
              <span style={{ display: 'inline-block', width: 8, height: 8, borderRadius: 4, background: l.color, marginRight: 6, verticalAlign: 'middle' }} />
              {l.name}
            </button>
          ))}
        </div>
        <button style={btn(transparent)} onClick={() => setTransparent((v) => !v)}>{transparent ? 'transparent' : 'solid'} ⇄</button>
        <div style={{ display: 'flex', gap: 14, font: '12px ui-monospace, monospace', marginTop: 2 }}>
          <a href="/torso-live" style={{ color: '#7fa6c4', textDecoration: 'none' }}>/torso-live →</a>
          <a href="/fma" style={{ color: '#7fa6c4', textDecoration: 'none' }}>/fma graph →</a>
        </div>
      </div>

      {error && <div style={{ position: 'absolute', top: '46%', width: '100%', textAlign: 'center', color: '#ff8095', font: '13px ui-monospace, monospace' }}>{error}</div>}
      <div style={{ position: 'absolute', bottom: 10, left: 16, color: '#5a6b7e', font: '10px ui-monospace, monospace', maxWidth: '70%', pointerEvents: 'none' }}>
        BodyParts3D, (c) The Database Center for Life Science, CC Attribution 4.0 International · clinical reasoning is a NARS demo, not medical advice
      </div>
    </div>
  );
}
