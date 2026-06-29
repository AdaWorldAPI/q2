// /genome — endless procedural double helix. EXPERIMENTAL, standalone (shares nothing
// with /cpic's CpicCockpit), so it can never break the working pharmacogenomics cockpit.
//
// The idea (the user's): the OGAR GUID address space is billions of slots
// (HEEL·HIP·TWIG cascade); CPIC fills almost none of it. So this is NOT a sized mesh —
// it is an ENDLESS scaffold that IS the address space, with the sparse real CPIC genes
// lighting up loci in it. Cheap because it is pure REPETITION: one instanced sugar bead
// and one instanced rung, both PLACED BY A FUNCTION of the integer step (golden-angle
// twist + linear rise), drawn only for a window of steps around a scroll offset — so the
// strand is infinite while the instance count is bounded and constant. No baked geometry,
// no forced shape. Zoom descends the 16-ary cascade: each tier subdivides the spacing ×16
// (self-similar — "scale = the next cascade level"), the literal fractal of the radix tree.
//
// Next step (documented, not done here): light loci from the real CPIC graph via
// POST /api/cpic/reason instead of the hardcoded gene table below.
import { useEffect, useRef, useState } from 'react';
import * as THREE from 'three';

const PAGE_BG = 0x05070d;
const WINDOW = 240;             // base-pairs instanced at once (the visible turns); constant
const RISE = 0.34;             // vertical gap between successive base-pairs
const RADIUS = 1.0;            // helix radius (strand centre to axis)
const TWIST = 2.399963;        // radians/step = the GOLDEN ANGLE → the pattern never exactly
// repeats (aperiodic, the "fractal endlessness" — same most-irrational step the φ-spiral uses).
const TAU = Math.PI * 2;

// The four bases as a deterministic repeating palette (A·T·G·C). Real DNA isn't periodic,
// but the SCAFFOLD is: the base at a step is a pure function of the step index, so the same
// address always paints the same rung — addressability without storage.
const BASE_RGB = [
  [0xff, 0x6b, 0x57], // A — coral
  [0xf2, 0xc9, 0x4c], // T — amber
  [0x4c, 0xa6, 0xf2], // G — azure
  [0x57, 0xd9, 0x8e], // C — mint
];
const baseAt = (step: number) => ((step * 2654435761) >>> 0) & 3; // cheap hash → 0..3, stable per step

// Sparse CPIC loci: real pharmacogenes lit up at fixed addresses in the endless scaffold.
// The gene list is pulled LIVE from GET /api/cpic/catalog; this canonical CPIC level-A set
// is only the fallback when the endpoint is absent (old deploy) so /genome still renders.
const FALLBACK_GENES = ['CYP2D6', 'CYP2C19', 'CYP2C9', 'CYP3A5', 'TPMT', 'DPYD', 'SLCO1B1',
  'UGT1A1', 'NUDT15', 'VKORC1', 'CYP4F2', 'G6PD', 'HLA-B', 'IFNL3', 'CFTR', 'RYR1'];
type Locus = { step: number; gene: string };
// Each gene gets a STABLE address from a hash of its name (FNV-1a) → a step in [0,4096).
// Same gene ⇒ same locus forever (addressability without storage), spread across the tier.
function lociFrom(genes: string[]): Locus[] {
  const seen = new Map<number, string>();
  const out: Locus[] = [];
  for (const g of genes) {
    let hsh = 2166136261;
    for (let i = 0; i < g.length; i++) { hsh ^= g.charCodeAt(i); hsh = Math.imul(hsh, 16777619); }
    let step = (hsh >>> 0) % 4096;
    while (seen.has(step)) step = (step + 1) % 4096;   // linear-probe the rare collision
    seen.set(step, g); out.push({ step, gene: g });
  }
  return out;
}

function labelSprite(text: string): THREE.Sprite {
  const c = document.createElement('canvas'); c.width = 256; c.height = 64;
  const x = c.getContext('2d')!;
  x.fillStyle = 'rgba(8,12,20,0.0)'; x.fillRect(0, 0, 256, 64);
  x.font = 'bold 34px ui-monospace, monospace'; x.textAlign = 'center'; x.textBaseline = 'middle';
  x.fillStyle = '#eaf2ff'; x.shadowColor = '#000'; x.shadowBlur = 6; x.fillText(text, 128, 32);
  const t = new THREE.CanvasTexture(c); t.anisotropy = 4;
  const s = new THREE.Sprite(new THREE.SpriteMaterial({ map: t, transparent: true, depthWrite: false }));
  s.scale.set(0.9, 0.225, 1); return s;
}

function mount(container: HTMLDivElement, scroll: { current: number }, density: { current: number },
  dirty: { current: boolean }, locusByStep: Map<number, string>): () => void {
  let w = container.clientWidth || window.innerWidth, h = container.clientHeight || window.innerHeight;
  const scene = new THREE.Scene(); scene.background = new THREE.Color(PAGE_BG);
  scene.fog = new THREE.Fog(PAGE_BG, 6, 16);   // ends fade into the dark → reads as endless
  const camera = new THREE.PerspectiveCamera(50, w / h, 0.01, 100); camera.position.set(0, 0, 6.2);
  const renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setSize(w, h); renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  container.appendChild(renderer.domElement);
  scene.add(new THREE.AmbientLight(0xffffff, 0.55));
  const key = new THREE.DirectionalLight(0xffffff, 0.9); key.position.set(2, 3, 4); scene.add(key);

  // ── instanced geometry: 2 strands of sugar beads + 1 set of base-pair rungs ──
  const bead = new THREE.SphereGeometry(0.085, 10, 8);
  const beadMat = new THREE.MeshStandardMaterial({ roughness: 0.5, metalness: 0.1 });
  const strandA = new THREE.InstancedMesh(bead, beadMat, WINDOW);
  const strandB = new THREE.InstancedMesh(bead, beadMat, WINDOW);
  strandA.instanceColor = new THREE.InstancedBufferAttribute(new Float32Array(WINDOW * 3), 3);
  strandB.instanceColor = new THREE.InstancedBufferAttribute(new Float32Array(WINDOW * 3), 3);
  const rung = new THREE.CylinderGeometry(0.028, 0.028, 1, 6); rung.rotateZ(Math.PI / 2); // lie along X
  const rungMat = new THREE.MeshStandardMaterial({ roughness: 0.6 });
  const rungs = new THREE.InstancedMesh(rung, rungMat, WINDOW);
  rungs.instanceColor = new THREE.InstancedBufferAttribute(new Float32Array(WINDOW * 3), 3);
  scene.add(strandA, strandB, rungs);
  const geneOf: (string | null)[] = new Array(WINDOW).fill(null); // instance k → gene (for picking)

  // a small pool of reusable locus labels (only the few visible in the window)
  const LABELS = 10;
  const labels: { sprite: THREE.Sprite; text: string }[] = [];
  for (let i = 0; i < LABELS; i++) { const s = labelSprite(''); s.visible = false; scene.add(s); labels.push({ sprite: s, text: '' }); }

  const m = new THREE.Matrix4(), q = new THREE.Quaternion(), pos = new THREE.Vector3(), scl = new THREE.Vector3(1, 1, 1);
  const cA = new THREE.Color(), cB = new THREE.Color(), cR = new THREE.Color();

  // place the window. step k (0..WINDOW) maps to ABSOLUTE address = base + k/dens, so as
  // `scroll` advances the same WINDOW instances slide along an infinite strand (no realloc),
  // and as `density` grows the spacing subdivides ×16 per tier (the fractal cascade zoom).
  function layout() {
    const dens = density.current;                 // base-pairs per unit step (16^tierFrac)
    const base = scroll.current;
    const rise = RISE / dens, twist = TWIST;      // finer tiers pack tighter (self-similar)
    let li = 0;
    for (let k = 0; k < WINDOW; k++) {
      const step = base + k;                       // integer address in this tier
      const ang = step * twist;
      const y = (k - WINDOW / 2) * rise;
      const ax = Math.cos(ang) * RADIUS, az = Math.sin(ang) * RADIUS;
      // strand A bead
      pos.set(ax, y, az); m.compose(pos, q, scl); strandA.setMatrixAt(k, m);
      // strand B bead (opposite side)
      pos.set(-ax, y, -az); m.compose(pos, q, scl); strandB.setMatrixAt(k, m);
      // rung: midpoint, scaled to span 2·RADIUS, rotated to point along the strand pair
      const addr = ((step % 4096) + 4096) % 4096;
      const isLoc = locusByStep.has(addr);
      geneOf[k] = isLoc ? locusByStep.get(addr)! : null;
      const b = baseAt(step), col = BASE_RGB[b];
      cA.setRGB(col[0] / 255, col[1] / 255, col[2] / 255);
      strandA.setColorAt(k, cA.clone().multiplyScalar(0.8));
      strandB.setColorAt(k, cB.setRGB(col[2] / 255, col[1] / 255, col[0] / 255).multiplyScalar(0.8));
      rungPlace(k, ax, y, az);
      if (isLoc) cR.setRGB(1, 1, 1); else cR.copy(cA).multiplyScalar(0.65);
      rungs.setColorAt(k, cR);
      // locus label
      if (isLoc && li < LABELS) {
        const g = geneOf[k]!;
        const L = labels[li++]; if (L.text !== g) { L.sprite.material.map = labelSprite(g).material.map; L.text = g; }
        L.sprite.position.set(0, y + 0.16, 0); L.sprite.visible = true;
      }
    }
    for (; li < LABELS; li++) labels[li].sprite.visible = false;
    strandA.instanceMatrix.needsUpdate = strandB.instanceMatrix.needsUpdate = rungs.instanceMatrix.needsUpdate = true;
    strandA.instanceColor!.needsUpdate = strandB.instanceColor!.needsUpdate = rungs.instanceColor!.needsUpdate = true;
  }
  const rq = new THREE.Quaternion(), up = new THREE.Vector3(0, 1, 0);
  function rungPlace(k: number, ax: number, y: number, az: number) {
    pos.set(0, y, 0);
    const len = Math.hypot(ax, az) * 2 || 1e-3;
    rq.setFromUnitVectors(up, new THREE.Vector3(ax, 0, az).normalize()); // align rung to the A↔B chord
    m.compose(pos, rq, new THREE.Vector3(1, len, 1)); rungs.setMatrixAt(k, m);
  }

  // controls: drag = orbit, wheel = descend/ascend tiers (fractal zoom), auto-drift = endless travel
  // click (no drag) on a lit locus = hand off to the working /cpic reasoner for that gene.
  let az = 0, el = 0.0, dragging = false, moved = 0, px = 0, py = 0, dist = 6.2;
  const ray = new THREE.Raycaster(); const ndc = new THREE.Vector2();
  const pick = (e: PointerEvent): string | null => {
    const r = el2.getBoundingClientRect();
    ndc.set(((e.clientX - r.left) / r.width) * 2 - 1, -((e.clientY - r.top) / r.height) * 2 + 1);
    ray.setFromCamera(ndc, camera);
    const hit = ray.intersectObject(rungs)[0];
    return hit && hit.instanceId != null ? geneOf[hit.instanceId] : null;
  };
  const onDown = (e: PointerEvent) => { dragging = true; moved = 0; px = e.clientX; py = e.clientY; };
  const onUp = (e: PointerEvent) => {
    dragging = false;
    if (moved < 5) { const g = pick(e); if (g) window.location.assign(`/cpic?gene=${encodeURIComponent(g)}`); }
  };
  const onMove = (e: PointerEvent) => {
    if (!dragging) return;
    moved += Math.abs(e.clientX - px) + Math.abs(e.clientY - py);
    az -= (e.clientX - px) * 0.005; el = Math.max(-1.2, Math.min(1.2, el + (e.clientY - py) * 0.005)); px = e.clientX; py = e.clientY; dirty.current = true;
  };
  const onWheel = (e: WheelEvent) => { e.preventDefault(); density.current = Math.max(1, Math.min(4096, density.current * (1 - Math.sign(e.deltaY) * 0.06))); dirty.current = true; };
  const el2 = renderer.domElement;
  el2.addEventListener('pointerdown', onDown); window.addEventListener('pointerup', onUp);
  window.addEventListener('pointermove', onMove); el2.addEventListener('wheel', onWheel, { passive: false });
  const onResize = () => { w = container.clientWidth; h = container.clientHeight; camera.aspect = w / h; camera.updateProjectionMatrix(); renderer.setSize(w, h); dirty.current = true; };
  window.addEventListener('resize', onResize);

  let raf = 0, lastScroll = NaN, lastDens = NaN;
  const tick = () => {
    raf = requestAnimationFrame(tick);
    scroll.current += 0.06;                        // gentle endless travel up the strand
    if (scroll.current !== lastScroll || density.current !== lastDens) { layout(); lastScroll = scroll.current; lastDens = density.current; }
    camera.position.set(dist * Math.cos(el) * Math.sin(az), dist * Math.sin(el), dist * Math.cos(el) * Math.cos(az));
    camera.lookAt(0, 0, 0);
    renderer.render(scene, camera);
    dirty.current = false;
  };
  tick();
  return () => {
    cancelAnimationFrame(raf);
    el2.removeEventListener('pointerdown', onDown); window.removeEventListener('pointerup', onUp);
    window.removeEventListener('pointermove', onMove); el2.removeEventListener('wheel', onWheel);
    window.removeEventListener('resize', onResize);
    bead.dispose(); rung.dispose(); beadMat.dispose(); rungMat.dispose(); renderer.dispose();
    if (el2.parentElement === container) container.removeChild(el2);
  };
}

export default function GenomeHelix() {
  const ref = useRef<HTMLDivElement>(null);
  const scroll = useRef(0);
  const density = useRef(1);
  const dirty = useRef(true);
  const [genes, setGenes] = useState<string[] | null>(null);   // null = still loading the catalog
  const [live, setLive] = useState(false);                     // true = real /api/cpic/catalog
  const [, force] = useState(0);

  // pull the REAL CPIC gene catalogue; fall back to the canonical list if the endpoint is
  // absent (old deploy) so /genome always renders. Same graceful-degradation as /helix LOD.
  useEffect(() => {
    let cancelled = false;
    fetch('/api/cpic/catalog')
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
      .then((j: { genes?: string[] }) => {
        if (cancelled) return;
        const gs = (j.genes ?? []).filter(Boolean);
        if (gs.length) { setGenes(gs); setLive(true); } else { setGenes(FALLBACK_GENES); }
      })
      .catch(() => { if (!cancelled) setGenes(FALLBACK_GENES); });
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    const c = ref.current; if (!c || !genes) return;
    const locusByStep = new Map(lociFrom(genes).map((l) => [l.step, l.gene]));
    return mount(c, scroll, density, dirty, locusByStep);
  }, [genes]);
  // light re-render so the tier readout updates as you zoom
  useEffect(() => { const id = setInterval(() => force((n) => n + 1), 250); return () => clearInterval(id); }, []);
  const tier = Math.log(density.current) / Math.log(16);
  return (
    <div style={{ position: 'fixed', inset: 0, background: `#${PAGE_BG.toString(16).padStart(6, '0')}` }}>
      <div ref={ref} style={{ position: 'absolute', inset: 0 }} />
      <div style={{ position: 'absolute', top: 12, left: 16, color: '#cdd9e5', font: '13px ui-monospace, monospace', pointerEvents: 'none' }}>
        <div style={{ color: '#fff', fontSize: 15 }}>/genome — endless pharmacogenomic helix</div>
        <div style={{ opacity: 0.62, marginTop: 3, maxWidth: 360 }}>
          {genes ? `${WINDOW} instanced base-pairs · golden-angle scaffold · ${genes.length} CPIC gene loci ${live ? 'lit (live /api/cpic)' : 'lit (fallback list)'} · tier ${tier.toFixed(2)}`
            : 'loading CPIC gene catalogue…'}
        </div>
        <div style={{ opacity: 0.4, marginTop: 4 }}>drag = orbit · wheel = descend the 16-ary cascade · click a lit gene → /cpic</div>
      </div>
    </div>
  );
}
