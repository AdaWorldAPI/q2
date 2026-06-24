// FMA torso · map view — the splat AS the GUID substrate.
//
// Renders the same torso.splat (SPL2: real BodyParts3D geometry, per-vertex
// normal, per-gaussian node-row tag) but treats it as the value-tenant SoA it
// is: click a gaussian -> its node row -> O(1) into torso.nodes.json (the node
// SoA) -> the FMA structure's identity (concept id, name, partonomy path,
// gaussian range). Click a structure in the list -> its gaussians highlight.
// Geometry, graph, and splat are three tenants of one identity, switch-selected.
//
// Geometry: BodyParts3D, (c) The Database Center for Life Science (CC-BY 4.0 /
// CC-BY-SA 2.1 JP). Attribution shown in-view.
import { useEffect, useMemo, useRef, useState } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';

interface NodeRow {
  row: number;
  fma: string;
  name: string;
  depth: number;
  parent: number | null;
  tiers: number[];
  rgb: [number, number, number];
  g_start: number;
  g_count: number;
  centroid: [number, number, number] | null;
}
interface NodesDoc { attribution: string; root: string; nodes: NodeRow[] }

interface Spl2 {
  count: number;
  positions: Float32Array;
  colors: Float32Array; // 0..1 rgb
  rows: Float32Array; // node_row per gaussian
}

function decodeSpl2(buf: ArrayBuffer): Spl2 {
  const dv = new DataView(buf);
  const magic = String.fromCharCode(dv.getUint8(0), dv.getUint8(1), dv.getUint8(2), dv.getUint8(3));
  if (magic !== 'SPL2') throw new Error(`bad magic "${magic}" (expected SPL2)`);
  const count = dv.getUint32(4, true);
  const off = 40;
  const positions = new Float32Array(count * 3);
  const colors = new Float32Array(count * 3);
  const rows = new Float32Array(count);
  for (let i = 0; i < count; i++) {
    const b = off + i * 21;
    // upright: (x,y,z) -> (x,z,-y)
    positions[i * 3] = dv.getFloat32(b, true);
    positions[i * 3 + 1] = dv.getFloat32(b + 8, true);
    positions[i * 3 + 2] = -dv.getFloat32(b + 4, true);
    colors[i * 3] = dv.getUint8(b + 15) / 255;
    colors[i * 3 + 1] = dv.getUint8(b + 16) / 255;
    colors[i * 3 + 2] = dv.getUint8(b + 17) / 255;
    rows[i] = dv.getUint16(b + 19, true);
  }
  return { count, positions, colors, rows };
}

const VERT = `
attribute vec3 aColor;
attribute float aRow;
uniform float uSelected;
uniform float uSize;
varying vec3 vColor;
varying float vDim;
void main() {
  vColor = aColor;
  vDim = (uSelected < 0.0 || abs(aRow - uSelected) < 0.5) ? 1.0 : 0.16;
  vec4 mv = modelViewMatrix * vec4(position, 1.0);
  gl_Position = projectionMatrix * mv;
  gl_PointSize = uSize * (1.0 / -mv.z);
}`;
const FRAG = `
precision mediump float;
varying vec3 vColor;
varying float vDim;
void main() {
  vec2 c = gl_PointCoord - 0.5;
  if (dot(c, c) > 0.25) discard;
  float a = smoothstep(0.25, 0.05, dot(c, c));
  gl_FragColor = vec4(vColor * vDim, a);
}`;

function mount(
  container: HTMLDivElement,
  splat: Spl2,
  onPick: (row: number) => void,
  apiRef: { current: { select: (row: number) => void } | null },
): () => void {
  let w = container.clientWidth || window.innerWidth;
  let h = container.clientHeight || window.innerHeight;
  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0x0a0e17);
  const camera = new THREE.PerspectiveCamera(45, w / h, 0.01, 100);
  camera.position.set(0, 0.05, 3.0);
  const renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setSize(w, h);
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  container.appendChild(renderer.domElement);

  const geom = new THREE.BufferGeometry();
  geom.setAttribute('position', new THREE.BufferAttribute(splat.positions, 3));
  geom.setAttribute('aColor', new THREE.BufferAttribute(splat.colors, 3));
  geom.setAttribute('aRow', new THREE.BufferAttribute(splat.rows, 1));
  const mat = new THREE.ShaderMaterial({
    vertexShader: VERT,
    fragmentShader: FRAG,
    uniforms: { uSelected: { value: -1 }, uSize: { value: 4.2 } },
    transparent: true,
    depthWrite: true,
  });
  const points = new THREE.Points(geom, mat);
  scene.add(points);

  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.target.set(0, 0, 0);
  controls.minDistance = 0.6;
  controls.maxDistance = 12;

  const ray = new THREE.Raycaster();
  ray.params.Points = { threshold: 0.02 };
  const ndc = new THREE.Vector2();
  let downAt = { x: 0, y: 0 };
  renderer.domElement.addEventListener('pointerdown', (e) => { downAt = { x: e.clientX, y: e.clientY }; });
  renderer.domElement.addEventListener('pointerup', (e) => {
    if (Math.hypot(e.clientX - downAt.x, e.clientY - downAt.y) > 5) return; // drag, not click
    const r = renderer.domElement.getBoundingClientRect();
    ndc.x = ((e.clientX - r.left) / r.width) * 2 - 1;
    ndc.y = -((e.clientY - r.top) / r.height) * 2 + 1;
    ray.setFromCamera(ndc, camera);
    const hit = ray.intersectObject(points, false);
    if (hit.length && hit[0].index != null) onPick(splat.rows[hit[0].index]);
  });

  apiRef.current = { select: (row: number) => { mat.uniforms.uSelected.value = row; } };

  let raf = 0;
  const tick = () => { raf = requestAnimationFrame(tick); controls.update(); renderer.render(scene, camera); };
  tick();
  const onResize = () => {
    w = container.clientWidth || window.innerWidth; h = container.clientHeight || window.innerHeight;
    camera.aspect = w / h; camera.updateProjectionMatrix(); renderer.setSize(w, h);
  };
  const ro = new ResizeObserver(onResize); ro.observe(container);
  return () => {
    cancelAnimationFrame(raf); ro.disconnect(); controls.dispose();
    geom.dispose(); mat.dispose(); renderer.dispose();
    if (renderer.domElement.parentNode === container) container.removeChild(renderer.domElement);
  };
}

export function TorsoMap() {
  const ref = useRef<HTMLDivElement>(null);
  const apiRef = useRef<{ select: (row: number) => void } | null>(null);
  const [splat, setSplat] = useState<Spl2 | null>(null);
  const [doc, setDoc] = useState<NodesDoc | null>(null);
  const [sel, setSel] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetch('/torso.splat')
      .then((r) => { if (!r.ok) throw new Error(`HTTP ${r.status} torso.splat`); return r.arrayBuffer(); })
      .then((b) => !cancelled && setSplat(decodeSpl2(b)))
      .catch((e) => !cancelled && setError(String(e)));
    fetch('/torso.nodes.json')
      .then((r) => (r.ok ? r.json() : null))
      .then((d) => !cancelled && d && setDoc(d as NodesDoc))
      .catch(() => {});
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    const c = ref.current;
    if (!c || !splat) return;
    return mount(c, splat, (row) => setSel(row), apiRef);
  }, [splat]);

  useEffect(() => { apiRef.current?.select(sel ?? -1); }, [sel]);

  // the "switch": row -> node, built once (O(1) lookup)
  const byRow = useMemo(() => {
    const m = new Map<number, NodeRow>();
    doc?.nodes.forEach((n) => m.set(n.row, n));
    return m;
  }, [doc]);

  // partonomy breadcrumb (walk parent links) for the selected node
  const path = useMemo(() => {
    if (sel == null) return [];
    const out: NodeRow[] = [];
    let cur: number | null = sel;
    for (let i = 0; i < 24 && cur != null; i++) {
      const n = byRow.get(cur);
      if (!n) break;
      out.push(n);
      cur = n.parent;
    }
    return out.reverse();
  }, [sel, byRow]);

  const owners = useMemo(
    () => (doc?.nodes ?? []).filter((n) => n.g_count > 0).sort((a, b) => b.g_count - a.g_count),
    [doc],
  );

  const selNode = sel != null ? byRow.get(sel) : undefined;
  const css = (rgb: [number, number, number]) => `rgb(${rgb[0]},${rgb[1]},${rgb[2]})`;

  return (
    <div style={{ position: 'fixed', inset: 0, background: '#0a0e17', overflow: 'hidden', color: '#cdd9e5', font: '12px ui-monospace, monospace' }}>
      <div ref={ref} style={{ position: 'absolute', inset: 0 }} />

      <div style={{ position: 'absolute', top: 12, left: 16, pointerEvents: 'none' }}>
        <div style={{ fontSize: 15, color: '#fff' }}>FMA torso · map</div>
        <div style={{ opacity: 0.7 }}>
          click a structure — splat ↔ FMA, one node at one address
          {doc ? ` · ${owners.length} structures` : ''}
        </div>
      </div>

      {error && (
        <div style={{ position: 'absolute', top: '46%', width: '100%', textAlign: 'center', color: '#ff8095' }}>{error}</div>
      )}

      {/* selected structure + partonomy path */}
      {selNode && (
        <div style={{ position: 'absolute', top: 12, right: 16, width: 290, background: '#0e1422cc', border: '1px solid #20304a', borderRadius: 6, padding: '10px 12px' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style={{ width: 11, height: 11, borderRadius: 3, background: css(selNode.rgb) }} />
            <span style={{ color: '#fff', fontSize: 14 }}>{selNode.name}</span>
          </div>
          <div style={{ opacity: 0.7, marginTop: 4 }}>
            {selNode.fma} · {selNode.g_count.toLocaleString()} gaussians · depth {selNode.depth}
          </div>
          <div style={{ opacity: 0.55, marginTop: 6, fontSize: 11 }}>HHTL tiers [{selNode.tiers.join(', ')}]</div>
          <div style={{ marginTop: 8, borderTop: '1px solid #1b2740', paddingTop: 6, opacity: 0.85 }}>
            {path.map((n, i) => (
              <div key={n.row} style={{ paddingLeft: i * 8, color: n.row === sel ? '#fff' : '#8aa0bb', cursor: 'pointer' }} onClick={() => setSel(n.row)}>
                {i > 0 ? '└ ' : ''}{n.name}
              </div>
            ))}
          </div>
        </div>
      )}

      {/* structure list (graph -> splat) */}
      <div style={{ position: 'absolute', bottom: 30, left: 16, maxHeight: '42%', width: 250, overflowY: 'auto', background: '#0e1422aa', border: '1px solid #1b2740', borderRadius: 6, padding: 6 }}>
        {owners.map((n) => (
          <div
            key={n.row}
            onClick={() => setSel(n.row)}
            style={{ display: 'flex', alignItems: 'center', gap: 7, padding: '2px 4px', borderRadius: 3, cursor: 'pointer', background: n.row === sel ? '#1d2b40' : 'transparent', color: n.row === sel ? '#fff' : '#9fb2c9' }}
          >
            <span style={{ width: 9, height: 9, borderRadius: 2, background: css(n.rgb), flex: '0 0 auto' }} />
            <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{n.name}</span>
          </div>
        ))}
      </div>

      <div style={{ position: 'absolute', top: 14, right: 320, font: '12px ui-monospace, monospace' }}>
        <a href="/torso-live" style={{ color: '#7fa6c4', textDecoration: 'none' }}>live orbit →</a>
      </div>
      <div style={{ position: 'absolute', bottom: 8, left: 16, color: '#5a6b7e', fontSize: 10, pointerEvents: 'none' }}>
        {doc?.attribution ?? 'BodyParts3D, (c) The Database Center for Life Science (CC-BY 4.0 / CC-BY-SA 2.1 JP)'}
      </div>
    </div>
  );
}
