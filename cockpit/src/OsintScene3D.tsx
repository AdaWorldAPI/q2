// 3D OSINT scene — the CAM address space rendered from raw SoA bytes.
//
// Fetches the binary SoA buffer `/osint.soa` (node GUIDs + class byte + edge
// index pairs) and decodes each 16-byte GUID → xyz IN THE BROWSER — the
// `osint_gotham::position()` logic ported to JS. No JSON anywhere: the bits feed
// the render. The address IS the coordinate; there is no force-layout pass.
import { useEffect, useRef, useState, type CSSProperties } from 'react';
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
import {
  CSS2DRenderer,
  CSS2DObject,
} from 'three/addons/renderers/CSS2DRenderer.js';

const GOLDEN_ANGLE = 2.3999632;
const NODE_REC = 17; // guid(16) + class(1)
const EDGE_REC = 5; // src(2) + tgt(2) + rel(1)
const HUB_CLASS = 0xff;

// class order → colour (matches OSINT_SCHEMA order in osint_gotham.rs).
const ORDER_COLOR = [
  0x4dd0e1, // 0 System
  0xffb547, // 1 Stakeholder
  0x35d07f, // 2 Person
  0x9b8cff, // 3 CivicSystem
  0xff637d, // 4 HistoricalSystem
  0x5a6b7f, // 5 SchemaValue
  0xc792ea, // 6 SchemaAxis
];
const LEGEND: Array<[string, number]> = [
  ['System', 0x4dd0e1],
  ['Stakeholder', 0xffb547],
  ['Person', 0x35d07f],
  ['CivicSystem', 0x9b8cff],
  ['HistoricalSystem', 0xff637d],
  ['SchemaValue', 0x5a6b7f],
  ['SchemaAxis', 0xc792ea],
  ['family hub', 0xffffff],
];

function colorOf(cls: number): number {
  return cls === HUB_CLASS ? 0xffffff : ORDER_COLOR[cls] ?? 0x8899aa;
}

// ── Relation types: the `rel` byte the bake already emits (rel_code in
// osint_gotham.rs). 0 member-of / 1 interfaces are the basin SCAFFOLD (the
// adapter's own spokes to the dissolved hubs); 2..9 are the real neo4j
// relations — the VIEW. The client had been discarding this byte and drawing
// every edge one colour, so the scaffold drowned the semantic graph.
const REL_NAME = [
  'member-of', 'interfaces', 'CONNECTED_TO', 'DEVELOPED_BY', 'DEPLOYED_BY',
  'PERSON_LINK', 'USED_IN', 'HIERARCHICAL', 'VALID_FOR', 'related',
];
const REL_COLOR = [
  0x223040, 0x223040, // 0,1 scaffold (unused once the family node dissolves)
  0x4dd0e1, // 2 CONNECTED_TO
  0xffb547, // 3 DEVELOPED_BY
  0x35d07f, // 4 DEPLOYED_BY
  0xff637d, // 5 PERSON_LINK
  0x9b8cff, // 6 USED_IN
  0xc792ea, // 7 HIERARCHICAL
  0x7fd1c7, // 8 VALID_FOR
  0x8fa6c4, // 9 related/other
];

// The family-basin MIXIN — the inverse of Louvain. Louvain reads edges and
// infers communities; the mixin reads the community LABEL (basin, on the GUID)
// and emits the intra-community edges it implies. Clique for small families
// (the true "connective category": membership ⇒ every pair related); ring for
// large families (bounded O(m) proxy of the same idea).
const MIXIN_COLOR = 0x3f6090;
const MIXIN_CLIQUE_MAX = 8;

// Legend for the relation layers (the mixin + the typed neo4j relations).
const REL_LEGEND: Array<[string, number]> = [
  ...REL_NAME.slice(2).map((nm, i) => [nm, REL_COLOR[i + 2]] as [string, number]),
];

// ── The generative grammar (the "counterpoint") ────────────────────────────
// A co-family type-PAIR voices an inferred relation. This is the imaginative
// inverse-Louvain: the community only handed us co-membership; the type-pair
// infers *how* two members relate. Selective — a pair with no rule stays
// silent (variation IS the expression). Hand-authored for now; later these
// rules bind to the OGIT class AST (OGAR / lance-graph) so the grammar is
// grounded in the ontology instead of by hand. Classes (OSINT_SCHEMA order):
//   0 System · 1 Stakeholder · 2 Person · 3 CivicSystem · 4 HistoricalSystem
//   5 SchemaValue · 6 SchemaAxis
// Returns a rel code (2..9) to voice, or -1 for silence.
function inferRel(a: number, b: number): number {
  const has = (x: number) => a === x || b === x;
  const pair = (x: number, y: number) => (a === x && b === y) || (a === y && b === x);
  if (has(4)) return 7; // HistoricalSystem × anything → precedes (HIERARCHICAL)
  if (has(3)) return 8; // CivicSystem × anything → governs / valid-for (VALID_FOR)
  if (has(5) || has(6)) return 6; // Schema × anything → typed-by / used-in (USED_IN)
  if (pair(2, 0)) return 3; // Person × System → develops / uses (DEVELOPED_BY)
  if (pair(1, 0)) return 4; // Stakeholder × System → deploys (DEPLOYED_BY)
  if (a === 0 && b === 0) return 2; // System × System → connects (CONNECTED_TO)
  if (a === 2 && b === 2) return 5; // Person × Person → person-link (PERSON_LINK)
  return -1; // no rule → silent
}

// φ⁻¹ — golden-ratio fractional walk. Feeding the index through `frac(i·φ⁻¹)`
// gives a low-discrepancy height in [0,1) WITHOUT needing the total count N, so
// the same even-on-a-sphere distribution works in a per-node pure decode.
const PHI_INV = 0.618033988749895;

// Port of osint_gotham::basin_center — basins on a 3D golden-angle LATTICE.
// The old layout was a flat Vogel disc (y ≡ 0) → a pancake. Lift the direction
// onto a spherical-Fibonacci shell so the basins fill a BALL with real depth.
// Height is even in cos(φ) (no pole clustering); longitude is the golden angle;
// radius still grows with the basin index so later basins ring outward.
function basinCenter(basin: number): [number, number, number] {
  const r = 40 * Math.sqrt(basin + 1);
  const cosPhi = 1 - 2 * (((basin + 0.5) * PHI_INV) % 1); // [-1,1], even in cosφ
  const sinPhi = Math.sqrt(Math.max(0, 1 - cosPhi * cosPhi));
  const theta = basin * GOLDEN_ANGLE;
  return [r * sinPhi * Math.cos(theta), r * cosPhi, r * sinPhi * Math.sin(theta)];
}

// Port of osint_gotham::position — decode a 16-byte GUID → xyz.
// GUID (little-endian): classid[0..4] heel[4..6] hip[6..8] twig[8..10]
//                       leaf[10..12] family[12..14] identity[14..16].
// The cascade tiers (heel/hip/twig) are dormant-ZERO in minted GUIDs per the
// CANON zero-fallback ladder, so depth must come from the LIVE discriminators
// (basin, identity) — using heel for Y collapsed the scene to a flat slab.
function decodePosition(
  dv: DataView,
  off: number,
  isHub: boolean,
): [number, number, number] {
  const family = dv.getUint16(off + 12, true);
  const identity = dv.getUint16(off + 14, true);
  const basin = family & 0xff;
  const c = basinCenter(basin);
  if (isHub) return c;
  // within-basin: a small 3D bloom (spherical spiral on identity), so members
  // get local depth too instead of a flat skirt around the basin centre.
  const r = 6 * Math.sqrt(identity + 1);
  const cosPhi = 1 - 2 * (((identity + 0.5) * PHI_INV) % 1);
  const sinPhi = Math.sqrt(Math.max(0, 1 - cosPhi * cosPhi));
  const theta = identity * GOLDEN_ANGLE;
  return [
    c[0] + r * sinPhi * Math.cos(theta),
    c[1] + r * cosPhi,
    c[2] + r * sinPhi * Math.sin(theta),
  ];
}

/** How a family-basin mixin is projected — the ClassView lens, as a render param. */
type MixinMode = 'category' | 'adapter';

// The six analysis lenses from the Palantir view. Each is a SEED for the
// reasoning bloom: firing a lens spreads activation from the most-connected
// entity of that lens's class (Civil Engineering = the civic lens, CivicSystem;
// AI Development & Surveillance both read the System class but fire from
// different hubs via a per-class cursor). Placeholder routing until the real
// /api/graph/infer NARS trace lands; the lens→class map is the front door.
const ANGLES: Array<{ name: string; cls: number }> = [
  { name: 'Economic Review', cls: 1 }, // Stakeholder
  { name: 'Civil Engineering', cls: 3 }, // CivicSystem — the civic lens
  { name: 'Political Dynamics', cls: 2 }, // Person
  { name: 'AI Development', cls: 0 }, // System
  { name: 'Kill Chain', cls: 4 }, // HistoricalSystem
  { name: 'Surveillance', cls: 0 }, // System (2nd-ranked hub)
];

/** Imperative handle the cockpit rail uses to drive the live scene. */
interface SceneApi {
  /** Fire the reasoning bloom from analysis lens `angleIdx` (index into ANGLES). */
  seedAngle: (angleIdx: number) => void;
  /** Clear any in-flight reasoning wave. */
  clearReasoning: () => void;
}

interface Scene {
  pos: Float32Array; // nodeCount * 3
  cls: Uint8Array; // nodeCount
  basin: Uint8Array; // nodeCount — family-basin community label (the mixin)
  edges: Uint32Array; // edgeCount * 2 (src, tgt)
  rels: Uint8Array; // edgeCount — relation type per edge (rel_code)
  labels: string[]; // nodeCount — entity names (label tail; '' if not baked)
  nodeCount: number;
  edgeCount: number;
}

function parseSoa(buf: ArrayBuffer): Scene {
  const dv = new DataView(buf);
  // magic "OSO1"
  if (dv.getUint8(0) !== 0x4f || dv.getUint8(1) !== 0x53) {
    throw new Error('bad SoA magic');
  }
  const nodeCount = dv.getUint32(4, true);
  const edgeCount = dv.getUint32(8, true);
  const pos = new Float32Array(nodeCount * 3);
  const cls = new Uint8Array(nodeCount);
  const basin = new Uint8Array(nodeCount);
  let off = 12;
  for (let i = 0; i < nodeCount; i++) {
    const o = off + i * NODE_REC;
    const c = dv.getUint8(o + 16);
    cls[i] = c;
    basin[i] = dv.getUint16(o + 12, true) & 0xff; // family low byte = the mixin label
    const [x, y, z] = decodePosition(dv, o, c === HUB_CLASS);
    pos[i * 3] = x;
    pos[i * 3 + 1] = y;
    pos[i * 3 + 2] = z;
  }
  off += nodeCount * NODE_REC;
  const edges = new Uint32Array(edgeCount * 2);
  const rels = new Uint8Array(edgeCount);
  for (let i = 0; i < edgeCount; i++) {
    const o = off + i * EDGE_REC;
    edges[i * 2] = dv.getUint16(o, true);
    edges[i * 2 + 1] = dv.getUint16(o + 2, true);
    rels[i] = dv.getUint8(o + 4); // the relation type the client had been discarding
  }
  off += edgeCount * EDGE_REC;
  // optional label tail (OSO1 additive): node_count × [len u8 | utf8 name].
  // Absent in pre-label assets → names stay '' and the view falls back to class.
  const labels: string[] = new Array(nodeCount).fill('');
  if (off < dv.byteLength) {
    const dec = new TextDecoder();
    for (let i = 0; i < nodeCount && off < dv.byteLength; i++) {
      const len = dv.getUint8(off);
      off += 1;
      labels[i] = dec.decode(new Uint8Array(buf, off, len));
      off += len;
    }
  }
  return { pos, cls, basin, edges, rels, labels, nodeCount, edgeCount };
}

function mount(
  container: HTMLDivElement,
  s: Scene,
  mode: MixinMode,
  apiRef: { current: SceneApi | null },
): () => void {
  let w = container.clientWidth || window.innerWidth;
  let h = container.clientHeight || window.innerHeight;

  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0x0a0e17);
  scene.fog = new THREE.FogExp2(0x0a0e17, 0.0014);
  const camera = new THREE.PerspectiveCamera(55, w / h, 0.1, 4000);
  const renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setSize(w, h);
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  container.appendChild(renderer.domElement);

  // normalize the decoded cloud to a centred radius-R sphere.
  const n = Math.max(s.nodeCount, 1);
  let cx = 0;
  let cy = 0;
  let cz = 0;
  for (let i = 0; i < s.nodeCount; i++) {
    cx += s.pos[i * 3];
    cy += s.pos[i * 3 + 1];
    cz += s.pos[i * 3 + 2];
  }
  cx /= n;
  cy /= n;
  cz /= n;
  // Frame the BULK, not the farthest outlier. basin_center radius grows with
  // basin index, so a handful of high-basin nodes sit far out on the rim while
  // ~90 % of the cloud is a dense core — normalizing by the max shrinks that
  // core into a speck with big empty margins. Normalize by the 93rd-percentile
  // distance instead: the core fills the radius-R sphere and the sparse rim
  // simply spills past the frame edge (acceptable — those nodes are few).
  const dists = new Float64Array(s.nodeCount);
  let maxd = 1e-6;
  for (let i = 0; i < s.nodeCount; i++) {
    const d = Math.hypot(
      s.pos[i * 3] - cx,
      s.pos[i * 3 + 1] - cy,
      s.pos[i * 3 + 2] - cz,
    );
    dists[i] = d;
    if (d > maxd) maxd = d;
  }
  const sortedD = Array.from(dists).sort((a, b) => a - b);
  const pctD = sortedD[Math.floor(sortedD.length * 0.93)] || maxd;
  const R = 100;
  const scale = R / Math.max(pctD, maxd * 1e-3);
  const p = new Float32Array(s.nodeCount * 3);
  for (let i = 0; i < s.nodeCount; i++) {
    p[i * 3] = (s.pos[i * 3] - cx) * scale;
    p[i * 3 + 1] = (s.pos[i * 3 + 1] - cy) * scale;
    p[i * 3 + 2] = (s.pos[i * 3 + 2] - cz) * scale;
  }

  // nodes as spheres. In `category` mode the family node is DISSOLVED into the
  // edges it implies (the connective category), so hubs aren't drawn; in
  // `adapter` mode the family node is a modest visible connector.
  const showHubs = mode === 'adapter';
  const sphereGeom = new THREE.SphereGeometry(1, 10, 8);
  const matCache = new Map<number, THREE.MeshBasicMaterial>();
  const nodeMeshes: THREE.Mesh[] = []; // members only — pick targets for foveal focus
  for (let i = 0; i < s.nodeCount; i++) {
    const c = s.cls[i];
    const isHub = c === HUB_CLASS;
    if (isHub && !showHubs) continue;
    let mat = matCache.get(c);
    if (!mat) {
      mat = new THREE.MeshBasicMaterial({
        color: colorOf(c),
        transparent: true,
        opacity: isHub ? 0.32 : 0.92,
      });
      matCache.set(c, mat);
    }
    const mesh = new THREE.Mesh(sphereGeom, mat);
    mesh.position.set(p[i * 3], p[i * 3 + 1], p[i * 3 + 2]);
    mesh.scale.setScalar(isHub ? 1.5 : 1.3);
    mesh.userData.idx = i;
    scene.add(mesh);
    if (!isHub) nodeMeshes.push(mesh);
  }

  const pushSeg = (arr: number[], a: number, b: number) => {
    arr.push(
      p[a * 3], p[a * 3 + 1], p[a * 3 + 2],
      p[b * 3], p[b * 3 + 1], p[b * 3 + 2],
    );
  };

  // Bow a straight edge into a quadratic-bezier polyline — the "wobbly arrow"
  // arc from the Palantir view, lifted into 3D. The control point pushes
  // perpendicular to the edge by a fraction of its length, so edges arc instead
  // of cutting dead-straight through the cloud (depth you can *feel* on orbit).
  // Emitted as consecutive GL_LINES pairs; optionally writes a flat per-vertex
  // colour and reports the arc apex (for label placement).
  const CURVE_SEG = 7; // sub-segments per edge
  const _cs = new THREE.Vector3();
  const _ce = new THREE.Vector3();
  const _cc = new THREE.Vector3(); // bezier control point
  const _cbow = new THREE.Vector3();
  const _cup = new THREE.Vector3(0, 1, 0);
  const _cprev = new THREE.Vector3();
  const _ccur = new THREE.Vector3();
  const pushCurve = (
    arr: number[],
    a: number,
    b: number,
    col?: number[],
    r = 0,
    g = 0,
    bl = 0,
    apexOut?: THREE.Vector3,
  ) => {
    _cs.set(p[a * 3], p[a * 3 + 1], p[a * 3 + 2]);
    _ce.set(p[b * 3], p[b * 3 + 1], p[b * 3 + 2]);
    _cc.addVectors(_cs, _ce).multiplyScalar(0.5);
    _cbow.subVectors(_ce, _cs);
    const len = _cbow.length() || 1;
    _cbow.cross(_cup); // perpendicular bow direction
    if (_cbow.lengthSq() < 1e-4) _cbow.set(1, 0, 0); // edge ∥ up → fall back
    _cbow.normalize().multiplyScalar(len * 0.14); // bow height = 14 % of length
    _cc.add(_cbow);
    if (apexOut) apexOut.copy(_cc).addScaledVector(_cbow, -0.5); // B(0.5)
    _cprev.copy(_cs);
    for (let k = 1; k <= CURVE_SEG; k++) {
      const t = k / CURVE_SEG;
      const it = 1 - t;
      const w0 = it * it;
      const w1 = 2 * it * t;
      const w2 = t * t;
      _ccur.set(
        w0 * _cs.x + w1 * _cc.x + w2 * _ce.x,
        w0 * _cs.y + w1 * _cc.y + w2 * _ce.y,
        w0 * _cs.z + w1 * _cc.z + w2 * _ce.z,
      );
      arr.push(_cprev.x, _cprev.y, _cprev.z, _ccur.x, _ccur.y, _ccur.z);
      if (col) col.push(r, g, bl, r, g, bl);
      _cprev.copy(_ccur);
    }
  };
  const _apex = new THREE.Vector3();

  // ── Layer 1: the family-basin MIXIN, projected per the ClassView mode ──
  // `category`: the GENERATIVE GRAMMAR — for each co-family pair, inferRel()
  //   voices a typed relation from the member type-pair (selective: silent
  //   where no rule fits). Rendered DASHED so inference reads distinctly from
  //   the literal baked relations below. (clique candidates for small families,
  //   ring candidates for large — bounded.)
  // `adapter`: the family node as a connector — the literal member→hub spokes
  //   the bake emits (rel 0 member-of / 1 interfaces).
  const grammarMode = mode === 'category';
  const famPos: number[] = [];
  const famCol: number[] = [];
  const famColor = new THREE.Color();
  if (grammarMode) {
    const byBasin = new Map<number, number[]>();
    for (let i = 0; i < s.nodeCount; i++) {
      if (s.cls[i] === HUB_CLASS) continue; // members only
      const arr = byBasin.get(s.basin[i]);
      if (arr) arr.push(i);
      else byBasin.set(s.basin[i], [i]);
    }
    const consider = (u: number, v: number) => {
      const rel = inferRel(s.cls[u], s.cls[v]);
      if (rel < 0) return; // no rule → silent
      pushSeg(famPos, u, v);
      famColor.set(REL_COLOR[rel] ?? 0x8fa6c4);
      famCol.push(famColor.r, famColor.g, famColor.b, famColor.r, famColor.g, famColor.b);
    };
    byBasin.forEach((members) => {
      const m = members.length;
      if (m < 2) return;
      if (m <= MIXIN_CLIQUE_MAX) {
        for (let a = 0; a < m; a++) {
          for (let b = a + 1; b < m; b++) consider(members[a], members[b]);
        }
      } else {
        for (let a = 0; a < m; a++) consider(members[a], members[(a + 1) % m]);
      }
    });
  } else {
    for (let i = 0; i < s.edgeCount; i++) {
      if (s.rels[i] > 1) continue; // 0 member-of / 1 interfaces = the adapter spokes
      const a = s.edges[i * 2];
      const b = s.edges[i * 2 + 1];
      if (a < s.nodeCount && b < s.nodeCount) pushSeg(famPos, a, b);
    }
  }
  const famGeom = new THREE.BufferGeometry();
  famGeom.setAttribute('position', new THREE.BufferAttribute(new Float32Array(famPos), 3));
  let famMat: THREE.LineBasicMaterial | THREE.LineDashedMaterial;
  if (grammarMode) {
    famGeom.setAttribute('color', new THREE.BufferAttribute(new Float32Array(famCol), 3));
    famMat = new THREE.LineDashedMaterial({
      vertexColors: true,
      transparent: true,
      opacity: 0.5,
      dashSize: 1.6,
      gapSize: 1.3,
      depthWrite: false,
    });
  } else {
    famMat = new THREE.LineBasicMaterial({
      color: MIXIN_COLOR,
      transparent: true,
      opacity: 0.16,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
  }
  const famLines = new THREE.LineSegments(famGeom, famMat);
  if (grammarMode) famLines.computeLineDistances(); // required for dashes
  scene.add(famLines);

  // ── Layer 2: the explicit neo4j relations (rel ≥ 2), colour-typed ──
  // The VIEW on top of the mixin fabric. The bake already carries these; the
  // client now honours the `rel` byte instead of painting every edge alike.
  const semPos: number[] = [];
  const semCol: number[] = [];
  const tmp = new THREE.Color();
  for (let i = 0; i < s.edgeCount; i++) {
    const rel = s.rels[i];
    if (rel < 2) continue; // scaffold handled by the family layer
    const a = s.edges[i * 2];
    const b = s.edges[i * 2 + 1];
    if (a >= s.nodeCount || b >= s.nodeCount) continue;
    tmp.set(REL_COLOR[rel] ?? 0x8fa6c4);
    pushCurve(semPos, a, b, semCol, tmp.r, tmp.g, tmp.b);
  }
  const semGeom = new THREE.BufferGeometry();
  semGeom.setAttribute('position', new THREE.BufferAttribute(new Float32Array(semPos), 3));
  semGeom.setAttribute('color', new THREE.BufferAttribute(new Float32Array(semCol), 3));
  const semMat = new THREE.LineBasicMaterial({
    vertexColors: true,
    transparent: true,
    opacity: 0.6,
    blending: THREE.AdditiveBlending,
    depthWrite: false,
  });
  scene.add(new THREE.LineSegments(semGeom, semMat));

  // ── Foveated connection labelling ──────────────────────────────────────────
  // 3344 connections can't all carry a label (that's the soup). The labels are
  // FOVEAL: hover a node and its connections light up and NAME themselves with
  // the relation type (CONNECTED_TO, DEPLOYED_BY, …) — the way the Palantir view
  // labels its edges. The node's class shows now; its entity name arrives with
  // the re-bake (names aren't in the SoA wire).
  const labelRenderer = new CSS2DRenderer();
  labelRenderer.setSize(w, h);
  labelRenderer.domElement.style.position = 'absolute';
  labelRenderer.domElement.style.top = '0';
  labelRenderer.domElement.style.left = '0';
  labelRenderer.domElement.style.pointerEvents = 'none';
  container.appendChild(labelRenderer.domElement);

  const mkLabel = (text: string, color: string, big: boolean): CSS2DObject => {
    const el = document.createElement('div');
    el.textContent = text;
    el.style.cssText =
      `font-family:monospace;font-size:${big ? 12 : 10}px;color:${color};` +
      'white-space:nowrap;text-shadow:0 0 4px #000,0 0 4px #000;' +
      'background:rgba(8,12,20,0.55);padding:1px 5px;border-radius:4px;pointer-events:none;';
    return new CSS2DObject(el);
  };

  // adjacency over the LITERAL relations (rel ≥ 2) — the facts the connections name.
  const adj = new Map<number, Array<{ j: number; rel: number }>>();
  const addAdj = (a: number, j: number, rel: number) => {
    let arr = adj.get(a);
    if (!arr) {
      arr = [];
      adj.set(a, arr);
    }
    arr.push({ j, rel });
  };
  for (let i = 0; i < s.edgeCount; i++) {
    const rel = s.rels[i];
    if (rel < 2) continue;
    const a = s.edges[i * 2];
    const b = s.edges[i * 2 + 1];
    if (a >= s.nodeCount || b >= s.nodeCount) continue;
    addAdj(a, b, rel);
    addAdj(b, a, rel);
  }

  const focusGroup = new THREE.Group();
  scene.add(focusGroup);
  const baseFamO = famMat.opacity;
  const baseSemO = semMat.opacity;
  const colTmp = new THREE.Color();
  let focusIdx = -1;
  const clearFocus = () => {
    focusGroup.children.slice().forEach((ch) => {
      focusGroup.remove(ch);
      (ch as THREE.LineSegments).geometry?.dispose?.();
      const el = (ch as unknown as CSS2DObject).element;
      if (el && el.parentNode) el.parentNode.removeChild(el);
    });
  };
  const setFocus = (idx: number) => {
    if (idx === focusIdx) return;
    focusIdx = idx;
    clearFocus();
    const active = idx >= 0;
    matCache.forEach((m, cls) => {
      m.opacity = active ? 0.14 : cls === HUB_CLASS ? 0.32 : 0.92;
    });
    famMat.opacity = active ? baseFamO * 0.22 : baseFamO;
    semMat.opacity = active ? baseSemO * 0.16 : baseSemO;
    if (!active) return;
    const list = adj.get(idx) ?? [];
    const fpos: number[] = [];
    const fcol: number[] = [];
    list.forEach(({ j, rel }) => {
      colTmp.set(REL_COLOR[rel] ?? 0x8fa6c4);
      pushCurve(fpos, idx, j, fcol, colTmp.r, colTmp.g, colTmp.b, _apex);
      const hex = `#${(REL_COLOR[rel] ?? 0x8fa6c4).toString(16).padStart(6, '0')}`;
      const lbl = mkLabel(REL_NAME[rel] ?? 'related', hex, false);
      lbl.position.copy(_apex); // label rides the arc apex, off the sight-line
      focusGroup.add(lbl);
    });
    const fg = new THREE.BufferGeometry();
    fg.setAttribute('position', new THREE.BufferAttribute(new Float32Array(fpos), 3));
    fg.setAttribute('color', new THREE.BufferAttribute(new Float32Array(fcol), 3));
    focusGroup.add(
      new THREE.LineSegments(
        fg,
        new THREE.LineBasicMaterial({
          vertexColors: true,
          transparent: true,
          opacity: 0.95,
          blending: THREE.AdditiveBlending,
          depthWrite: false,
        }),
      ),
    );
    const clsName = (LEGEND[s.cls[idx]] && LEGEND[s.cls[idx]][0]) || 'node';
    const name = s.labels[idx] || clsName; // real entity name once baked, else class
    const node = mkLabel(`${name}  ·  ${clsName} · ${list.length} conn`, '#cfe7ff', true);
    node.position.set(p[idx * 3], p[idx * 3 + 1] + 4, p[idx * 3 + 2]);
    focusGroup.add(node);
  };

  const ray = new THREE.Raycaster();
  const ndc = new THREE.Vector2();
  const onMove = (ev: PointerEvent) => {
    const rect = renderer.domElement.getBoundingClientRect();
    ndc.x = ((ev.clientX - rect.left) / rect.width) * 2 - 1;
    ndc.y = -((ev.clientY - rect.top) / rect.height) * 2 + 1;
    ray.setFromCamera(ndc, camera);
    const hit = ray.intersectObjects(nodeMeshes, false)[0];
    setFocus(hit ? (hit.object.userData.idx as number) : -1);
  };
  renderer.domElement.addEventListener('pointermove', onMove);

  // ── Spreading activation (double-click to seed) ─────────────────────────────
  // Double-click a node to SEED a reasoning wave: activation spreads outward
  // along the typed relations in timed hops — the lit tree grows like firing
  // axons developing a concept. First slice of the live-reasoning / "watch the
  // brain think" view; the real NARS trace from /api/graph/infer (truth-weighted
  // deduction/abduction over the 4096-bit SoA) replaces this BFS next.
  const TARGET = 40; // surviving theories after the prune
  const REVEAL_MS = 38; // bloom cadence (firing speed)
  const PRUNE_MS = 2600; // contraction duration
  let actLines: THREE.LineSegments | null = null;
  let actBase: Float32Array | null = null; // bloom colours (restore target for fade)
  let actScore: Float32Array | null = null; // per-segment score (entropy proxy)
  let actThresh = 0; // final prune threshold = TARGET-th highest score
  let actStart = 0;
  let actPruneStart = 0;
  let actTotalSeg = 0;
  let actPhase: 'bloom' | 'prune' = 'bloom';
  const counter = document.createElement('div');
  counter.style.cssText =
    'position:absolute;top:14px;left:50%;transform:translateX(-50%);font-family:monospace;' +
    'font-size:13px;color:#cfe7ff;background:rgba(8,12,20,0.6);padding:4px 12px;border-radius:6px;' +
    'pointer-events:none;display:none;text-shadow:0 0 4px #000;';
  container.appendChild(counter);
  const clearActivation = () => {
    if (actLines) {
      scene.remove(actLines);
      actLines.geometry.dispose();
      (actLines.material as THREE.Material).dispose();
      actLines = null;
    }
    actBase = null;
    actScore = null;
    actTotalSeg = 0;
    counter.style.display = 'none';
  };
  const seedActivation = (seed: number) => {
    clearActivation();
    const seen = new Set<number>([seed]);
    let frontier = [seed];
    const apos: number[] = [];
    const acol: number[] = [];
    const scores: number[] = [];
    const heat = new THREE.Color();
    let depth = 0;
    const MAXD = 8;
    const MAXN = 700;
    const hash = (a: number, b: number) =>
      ((((a * 73856093) ^ (b * 19349663)) >>> 0) % 1000) / 1000;
    while (frontier.length && depth < MAXD && seen.size < MAXN) {
      const next: number[] = [];
      for (const u of frontier) {
        for (const { j } of adj.get(u) ?? []) {
          if (seen.has(j)) continue;
          seen.add(j);
          next.push(j);
          apos.push(p[u * 3], p[u * 3 + 1], p[u * 3 + 2], p[j * 3], p[j * 3 + 1], p[j * 3 + 2]);
          const t = depth / MAXD;
          heat.setHSL(0.55 - t * 0.12, 0.9, 0.78 - t * 0.4); // hot near seed → cool far
          acol.push(heat.r, heat.g, heat.b, heat.r, heat.g, heat.b);
          // entropy proxy: shallow + well-connected + a deterministic jitter.
          // (placeholder for the real NARS truth/confidence over the SoA.)
          const deg = Math.min(1, (adj.get(j)?.length ?? 1) / 12);
          scores.push(0.45 * (1 - t) + 0.35 * deg + 0.2 * hash(u, j));
          if (seen.size >= MAXN) break;
        }
        if (seen.size >= MAXN) break;
      }
      frontier = next;
      depth++;
    }
    if (!apos.length) return;
    actBase = new Float32Array(acol);
    actScore = new Float32Array(scores);
    const sorted = [...scores].sort((a, b) => b - a);
    actThresh = sorted[Math.min(TARGET, sorted.length) - 1] ?? 0;
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.BufferAttribute(new Float32Array(apos), 3));
    g.setAttribute('color', new THREE.BufferAttribute(new Float32Array(acol), 3));
    g.setDrawRange(0, 0);
    actLines = new THREE.LineSegments(
      g,
      new THREE.LineBasicMaterial({
        vertexColors: true,
        transparent: true,
        opacity: 0.95,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
      }),
    );
    scene.add(actLines);
    actTotalSeg = scores.length;
    actStart = performance.now();
    actPhase = 'bloom';
    counter.style.display = 'block';
  };
  const onDbl = (ev: MouseEvent) => {
    const rect = renderer.domElement.getBoundingClientRect();
    ndc.x = ((ev.clientX - rect.left) / rect.width) * 2 - 1;
    ndc.y = -((ev.clientY - rect.top) / rect.height) * 2 + 1;
    ray.setFromCamera(ndc, camera);
    const hit = ray.intersectObjects(nodeMeshes, false)[0];
    if (hit) seedActivation(hit.object.userData.idx as number);
    else clearActivation();
  };
  renderer.domElement.addEventListener('dblclick', onDbl);

  // ── Analysis-lens seeds: one node per ANGLE for the cockpit rail ────────────
  // Rank each class's members by literal degree (the `adj` fabric), then assign
  // each lens the next-highest hub of its class via a per-class cursor — so two
  // lenses on the same class (AI Development / Surveillance both = System) fire
  // from DIFFERENT entities. Firing a lens = double-clicking its hub.
  const byClassRanked = new Map<number, number[]>();
  for (let i = 0; i < s.nodeCount; i++) {
    if (s.cls[i] === HUB_CLASS) continue;
    const arr = byClassRanked.get(s.cls[i]);
    if (arr) arr.push(i);
    else byClassRanked.set(s.cls[i], [i]);
  }
  byClassRanked.forEach((arr) =>
    arr.sort((x, y) => (adj.get(y)?.length ?? 0) - (adj.get(x)?.length ?? 0)),
  );
  const classCursor = new Map<number, number>();
  const angleSeeds = ANGLES.map(({ cls }) => {
    const arr = byClassRanked.get(cls) ?? [];
    const c = classCursor.get(cls) ?? 0;
    classCursor.set(cls, c + 1);
    return arr.length ? arr[Math.min(c, arr.length - 1)] : -1;
  });
  apiRef.current = {
    seedAngle: (ai) => {
      const idx = angleSeeds[ai];
      if (idx >= 0) seedActivation(idx);
    },
    clearReasoning: () => clearActivation(),
  };

  // Frame the centred radius-R cloud, then hand control to the user:
  // left-drag = orbit, scroll = zoom, right-drag = pan. A gentle idle
  // auto-rotate runs until the first interaction, then yields fully.
  const fitDist = R / Math.sin((camera.fov * Math.PI) / 360); // fit sphere R in FOV
  camera.position.set(fitDist * 0.55, fitDist * 0.62, fitDist * 0.95);
  camera.lookAt(0, 0, 0);

  const controls = new OrbitControls(camera, renderer.domElement);
  controls.target.set(0, 0, 0);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.rotateSpeed = 0.6;
  controls.zoomSpeed = 0.9;
  controls.panSpeed = 0.7;
  controls.minDistance = 15;
  controls.maxDistance = 1500;
  controls.autoRotate = true;
  controls.autoRotateSpeed = 0.5;
  controls.addEventListener('start', () => {
    controls.autoRotate = false;
  });

  let animId = 0;
  const animate = () => {
    controls.update();
    if (actLines && actTotalSeg && actBase && actScore) {
      const now = performance.now();
      if (actPhase === 'bloom') {
        // expansion: reveal the wave segment-by-segment
        const rev = Math.min(actTotalSeg, Math.floor((now - actStart) / REVEAL_MS));
        actLines.geometry.setDrawRange(0, rev * 2);
        counter.textContent = `${rev} ideas firing…`;
        if (rev >= actTotalSeg) {
          actPhase = 'prune';
          actPruneStart = now;
          actLines.geometry.setDrawRange(0, actTotalSeg * 2);
        }
      } else {
        // contraction (temporal-entropy prune): threshold rises 0→actThresh,
        // below-threshold branches fade to black (additive ⇒ invisible),
        // survivors stay lit — thousands of ideas collapse to ~TARGET theories.
        const f = Math.min(1, (now - actPruneStart) / PRUNE_MS);
        const thr = actThresh * f;
        const band = actThresh * 0.14 + 1e-6;
        const col = actLines.geometry.getAttribute('color') as THREE.BufferAttribute;
        const arr = col.array as Float32Array;
        let alive = 0;
        for (let sIdx = 0; sIdx < actTotalSeg; sIdx++) {
          const sc = actScore[sIdx];
          const k = sc >= thr ? 1 : Math.max(0, 1 - (thr - sc) / band);
          if (sc >= thr) alive++;
          const o = sIdx * 6;
          for (let v = 0; v < 6; v++) arr[o + v] = actBase[o + v] * k;
        }
        col.needsUpdate = true;
        counter.textContent = f >= 1 ? `${alive} theories` : `${alive} → ${TARGET} · pruning…`;
      }
    }
    renderer.render(scene, camera);
    labelRenderer.render(scene, camera);
    animId = requestAnimationFrame(animate);
  };
  animate();

  const handleResize = () => {
    w = container.clientWidth || window.innerWidth;
    h = container.clientHeight || window.innerHeight;
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
    renderer.setSize(w, h);
    labelRenderer.setSize(w, h);
  };
  window.addEventListener('resize', handleResize);
  // The cockpit rail toggling does NOT fire a window resize — it just reflows
  // the flex main pane — so observe the container's box directly to keep the
  // canvas + label layer matched to the viewport in both cockpit and fullscreen.
  const ro = new ResizeObserver(handleResize);
  ro.observe(container);

  return () => {
    cancelAnimationFrame(animId);
    controls.dispose();
    apiRef.current = null;
    ro.disconnect();
    renderer.domElement.removeEventListener('pointermove', onMove);
    renderer.domElement.removeEventListener('dblclick', onDbl);
    clearFocus();
    clearActivation();
    window.removeEventListener('resize', handleResize);
    sphereGeom.dispose();
    famGeom.dispose();
    famMat.dispose();
    semGeom.dispose();
    semMat.dispose();
    matCache.forEach((m) => m.dispose());
    renderer.dispose();
    if (container.contains(renderer.domElement)) {
      container.removeChild(renderer.domElement);
    }
    if (container.contains(labelRenderer.domElement)) {
      container.removeChild(labelRenderer.domElement);
    }
    if (container.contains(counter)) {
      container.removeChild(counter);
    }
  };
}

/** Route component: fetch the binary SoA and render it as a 3D scene. */
export function OsintScene3D() {
  const ref = useRef<HTMLDivElement>(null);
  const sceneRef = useRef<Scene | null>(null);
  const apiRef = useRef<SceneApi | null>(null);
  const [mode, setMode] = useState<MixinMode>('category');
  const [view, setView] = useState<'cockpit' | 'fullscreen'>('cockpit');
  const [angle, setAngle] = useState<number | null>(null);
  const [info, setInfo] = useState<{ nodes: number; edges: number } | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Fetch + parse the SoA once.
  useEffect(() => {
    let cancelled = false;
    fetch('/osint.soa')
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.arrayBuffer();
      })
      .then((buf) => {
        if (cancelled) return;
        const s = parseSoa(buf);
        sceneRef.current = s;
        setInfo({ nodes: s.nodeCount, edges: s.edgeCount });
      })
      .catch((e: unknown) => {
        if (!cancelled) setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // (Re)mount whenever the data lands or the mixin projection flips. The
  // cockpit↔fullscreen toggle does NOT remount (the ResizeObserver in mount
  // keeps the canvas sized), so the camera + any in-flight reasoning survive it.
  useEffect(() => {
    const container = ref.current;
    const s = sceneRef.current;
    if (!container || !s) return;
    return mount(container, s, mode, apiRef);
  }, [mode, info]);

  const fireLens = (i: number) => {
    setAngle(i);
    apiRef.current?.seedAngle(i);
  };
  const clearLens = () => {
    setAngle(null);
    apiRef.current?.clearReasoning();
  };

  const cockpit = view === 'cockpit';
  const lensBtn = (i: number): CSSProperties => ({
    display: 'block',
    width: '100%',
    textAlign: 'left',
    fontFamily: 'monospace',
    fontSize: 12,
    color: angle === i ? '#0a0e17' : '#cfe7ff',
    background: angle === i ? `#${colorOf(ANGLES[i].cls).toString(16).padStart(6, '0')}` : 'rgba(17,32,48,0.5)',
    border: `1px solid #${colorOf(ANGLES[i].cls).toString(16).padStart(6, '0')}`,
    borderRadius: 6,
    padding: '7px 10px',
    marginBottom: 6,
    cursor: 'pointer',
    fontWeight: angle === i ? 700 : 400,
  });

  return (
    <div
      style={{
        position: 'relative',
        width: '100%',
        height: '100vh',
        background: '#0a0e17',
        overflow: 'hidden',
        display: 'flex',
      }}
    >
      {cockpit && (
        <aside
          style={{
            flex: '0 0 256px',
            width: 256,
            height: '100%',
            boxSizing: 'border-box',
            overflowY: 'auto',
            padding: '16px 14px',
            background: 'linear-gradient(180deg,#0d1320,#0a0e17)',
            borderRight: '1px solid #1b2c3e',
            fontFamily: 'monospace',
            color: '#93a9bf',
          }}
        >
          <div style={{ fontSize: 14, color: '#cfe7ff', marginBottom: 2 }}>
            OSINT COCKPIT
          </div>
          <div style={{ fontSize: 11, color: '#6f87a0' }}>
            classid 0x0700 · SoA → GUID-decoded 3D
          </div>
          {info && (
            <div style={{ fontSize: 11, marginTop: 4 }}>
              {info.nodes} nodes · {info.edges} edges
            </div>
          )}
          {error && (
            <div style={{ color: '#ff637d', fontSize: 11, marginTop: 4 }}>
              load error: {error}
            </div>
          )}

          <div style={{ fontSize: 11, color: '#7f97b0', margin: '14px 0 6px', letterSpacing: 1 }}>
            ANALYSIS LENS · seed reasoning
          </div>
          {ANGLES.map((a, i) => (
            <button key={a.name} onClick={() => fireLens(i)} style={lensBtn(i)}>
              {a.name}
            </button>
          ))}
          <button
            onClick={clearLens}
            style={{
              display: 'block',
              width: '100%',
              fontFamily: 'monospace',
              fontSize: 11,
              color: '#93a9bf',
              background: 'transparent',
              border: '1px solid #243a52',
              borderRadius: 6,
              padding: '5px 10px',
              marginTop: 2,
              cursor: 'pointer',
            }}
          >
            clear reasoning
          </button>
          <div style={{ fontSize: 10, color: '#5f7790', marginTop: 6 }}>
            bloom → temporal-entropy prune → ~40 theories. hover any node to
            read its links; double-click to reason from it.
          </div>

          <div style={{ fontSize: 11, color: '#7f97b0', margin: '14px 0 6px', letterSpacing: 1 }}>
            MIXIN PROJECTION
          </div>
          <button
            onClick={() => setMode((m) => (m === 'category' ? 'adapter' : 'category'))}
            title="Project the family-basin mixin as a connective category (clique among co-family members) or as an adapter (family node + member→hub spokes)"
            style={{
              display: 'block',
              width: '100%',
              fontFamily: 'monospace',
              fontSize: 12,
              color: '#cfe7ff',
              background: 'rgba(17,32,48,0.6)',
              border: '1px solid #2a4a6a',
              borderRadius: 6,
              padding: '7px 10px',
              cursor: 'pointer',
            }}
          >
            {mode === 'category' ? 'connective category' : 'adapter'}
          </button>
          <div style={{ fontSize: 10, color: '#5f7790', marginTop: 4 }}>
            solid = literal · dashed = inferred (counterpoint grammar)
          </div>

          <div style={{ fontSize: 11, color: '#7f97b0', margin: '14px 0 6px', letterSpacing: 1 }}>
            CLASSES
          </div>
          {LEGEND.map(([name, c]) => (
            <div key={name} style={{ fontSize: 11, marginBottom: 3, whiteSpace: 'nowrap' }}>
              <span
                style={{
                  display: 'inline-block',
                  width: 8,
                  height: 8,
                  borderRadius: 8,
                  background: `#${c.toString(16).padStart(6, '0')}`,
                  marginRight: 6,
                }}
              />
              {name}
            </div>
          ))}
          <div style={{ fontSize: 11, color: '#7f97b0', margin: '12px 0 6px', letterSpacing: 1 }}>
            RELATIONS
          </div>
          {REL_LEGEND.map(([name, c]) => (
            <div key={name} style={{ fontSize: 11, marginBottom: 3, whiteSpace: 'nowrap' }}>
              <span
                style={{
                  display: 'inline-block',
                  width: 14,
                  borderTop: `2px solid #${c.toString(16).padStart(6, '0')}`,
                  marginRight: 6,
                  verticalAlign: 'middle',
                }}
              />
              {name}
            </div>
          ))}
        </aside>
      )}

      <main style={{ position: 'relative', flex: '1 1 auto', minWidth: 0, height: '100%' }}>
        <div ref={ref} style={{ position: 'absolute', inset: 0 }} />
        {!cockpit && info && (
          <div
            style={{
              position: 'absolute',
              top: 16,
              left: 16,
              fontFamily: 'monospace',
              color: '#93a9bf',
              fontSize: 12,
              textShadow: '0 0 4px #0a0e17',
              pointerEvents: 'none',
            }}
          >
            <div style={{ fontSize: 14, color: '#cfe7ff' }}>
              OSINT · classid 0x0700 · GUID-decoded 3D
            </div>
            <div>
              {info.nodes} nodes · {info.edges} edges · hover to read links ·
              double-click to reason
            </div>
          </div>
        )}
        <button
          onClick={() => setView((v) => (v === 'cockpit' ? 'fullscreen' : 'cockpit'))}
          title="Cycle between the framed cockpit (analysis lenses + legend) and the fullscreen vast-space view"
          style={{
            position: 'absolute',
            top: 12,
            right: 12,
            fontFamily: 'monospace',
            fontSize: 12,
            color: '#cfe7ff',
            background: 'rgba(17,32,48,0.7)',
            border: '1px solid #2a4a6a',
            borderRadius: 6,
            padding: '6px 12px',
            cursor: 'pointer',
          }}
        >
          {cockpit ? '⛶ vast space' : '▣ cockpit'}
        </button>
      </main>
    </div>
  );
}
