//! The `/OSM` cockpit — a self-contained slippy-map page over the OSM tile
//! material ([`crate::osm_tiles`]). The Geo-domain (`0x0F`) sibling of the FMA
//! body-helix cockpit: pan/zoom OSM raster tiles from the standard source, and
//! read each tile's HHTL (HEEL/HIP/TWIG/LEAF) key live from `/api/osm/locate`.
//!
//! The page is a single inline HTML string (the `/mri` cockpit pattern): no
//! build step, no external JS. The HHTL address is resolved server-side so the
//! map pyramid and the cascade address stay the same source of truth.
//!
//! **The default basemap is DRAWN, not downloaded.** Shapes come from
//! `/api/osm/geometry/tile-bin/:z/:x/:y` — the `OSM1` binary LE wire — over
//! the local bake, so a default page load fetches nothing from a third-party
//! tile server (measured: 0 external requests, against 141-277 to
//! `tile.openstreetmap.org` before). The wire is consumed as an askama-style
//! PROJECTION of the slab: `ArrayBuffer` → `Float32Array` lens → `Path2D`
//! per (tile, class) → ~150 native canvas draw calls per frame. The first
//! version drew 69k retained SVG DOM nodes from a JSON wire and was measured
//! "terribly slow" on the live deploy; both of those representations are
//! deliberately gone. The two raster skins
//! ([`crate::osm_tiles::OSM_TILE_URL`], `SAT_TILE_URL`) remain behind the
//! basemap toggle as the reference you check our render against; they are an
//! explicit opt-in, which is also what keeps a deployed host clear of the
//! OSMF tile usage policy.

/// `GET /osm` — the OSM map cockpit page.
pub async fn osm_page_handler() -> axum::response::Html<String> {
    axum::response::Html(PAGE.to_string())
}

const PAGE: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8"/>
<meta name="viewport" content="width=device-width, initial-scale=1"/>
<title>OSM cockpit — WebMercator ↔ HHTL</title>
<style>
  :root { color-scheme: dark; }
  * { box-sizing: border-box; }
  html,body { margin:0; height:100%; background:#0b0e13; color:#c9d4e3;
    font:13px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace; }
  #app { display:grid; grid-template-columns:1fr 320px; height:100%; }
  /* `touch-action:none` is what makes a finger pan the MAP instead of scrolling
     the PAGE: without it the browser claims the gesture before any pointer
     handler sees a move, and the map is frozen on every touch device. */
  /* The LAND colour. GL clears to transparent and Canvas2D only clears, so
     this is what shows wherever no polygon covers — i.e. it IS the basemap's
     ground, not a page backdrop. It was #11151c, which is why unmapped ground
     and dense-but-unresolved districts both read as black. */
  #map { position:relative; overflow:hidden; cursor:grab; background:#f2efe9;
    touch-action:none; }
  #map.drag { cursor:grabbing; }
  #tiles { position:absolute; left:0; top:0; will-change:transform; }
  #tiles img { position:absolute; width:256px; height:256px; user-select:none;
    -webkit-user-drag:none; image-rendering:auto; }
  #panel { border-left:1px solid #1e2733; padding:16px; overflow:auto; }
  h1 { font-size:15px; margin:0 0 4px; color:#8fd6ff; }
  .sub { color:#6b7c93; margin:0 0 16px; }
  .row { display:flex; justify-content:space-between; padding:3px 0;
    border-bottom:1px solid #161d27; }
  .k { color:#6b7c93; } .v { color:#e6edf5; }
  .tier { display:grid; grid-template-columns:repeat(4,1fr); gap:6px; margin:12px 0; }
  .cell { background:#141b24; border:1px solid #223; border-radius:6px; padding:8px;
    text-align:center; }
  .cell b { display:block; font-size:13px; color:#8fd6ff; }
  .cell span { color:#6b7c93; font-size:11px; }
  .ctl { position:absolute; left:12px; top:12px; display:flex; flex-direction:column;
    gap:6px; z-index:5; }
  .ctl button { width:34px; height:34px; font-size:18px; background:#141b24cc;
    color:#c9d4e3; border:1px solid #2a3644; border-radius:6px; cursor:pointer; }
  .ctl button:hover { background:#1b2530; }
  .hint { position:absolute; right:12px; bottom:12px; color:#4d5b6e; z-index:5; }
  code { color:#9fe0a4; word-break:break-all; }
  .attr { position:absolute; left:0; bottom:0; background:#0b0e13aa; padding:2px 6px;
    font-size:11px; z-index:5; }
  .attr a { color:#8fd6ff; }
  .ctl button.active { background:#1d3a2a; border-color:#2f6b4a; color:#9fe0a4; }
  /* Clickable on purpose: a dot answers "what IS this?" via
     /api/osm/feature/:idx. `pointer-events` is NOT disabled — panning still
     works because mousedown bubbles from the dot to #map, and the existing
     `moved` guard already suppresses the click that ends a drag. */
  .pt { position:absolute; width:5px; height:5px; margin:-3px 0 0 -3px;
    border-radius:50%; background:#ffb454; box-shadow:0 0 0 1px #0b0e13aa;
    cursor:pointer; }
  .pt.sel { background:#7ee787; box-shadow:0 0 0 2px #7ee787aa; z-index:4; }
  #feature { margin-top:14px; border-top:1px solid #2a3140; padding-top:10px; }
  #feature .tag { display:flex; gap:8px; font-size:11px; padding:1px 0; }
  #feature .tag b { color:#7ee787; font-weight:600; min-width:96px; }
  #feature .tag span { color:#c9d3e0; word-break:break-word; }
  .feat-status { position:absolute; right:12px; top:12px; z-index:5;
    background:#0b0e13aa; padding:2px 6px; border-radius:4px; color:#8fa0b8; }

  /* A phone is ~390 CSS px wide, and `1fr 320px` leaves the map ~70 of them:
     a map too narrow to read beside a readout you cannot get away from. Stack
     them instead, and keep the map the subject by giving it the larger share.
     The breakpoint is on WIDTH, not on a device class — a narrow desktop
     window has exactly the same problem. */
  @media (max-width: 720px) {
    #app { grid-template-columns:1fr;
           grid-template-rows:minmax(0,58vh) minmax(0,1fr); }
    #panel { border-left:none; border-top:1px solid #1e2733; }
    /* No room, and a finger does not need to be told it can drag. */
    .hint { display:none; }
    /* 34px is under the ~44px touch-target floor; 40 + the 6px gap clears it. */
    .ctl button { width:40px; height:40px; }
    /* The status line and the zoom controls share the top edge, and on a 390px
       map the status text is wide enough to run underneath the buttons. Cap it
       clear of them: 12px inset + 40px button + 24px breathing room. */
    .feat-status { max-width:calc(100% - 76px); text-align:right; }
  }
  /* `vh` on a phone is the tallest the viewport ever gets, so the panel's last
     rows sit behind the URL bar until it retracts. `dvh` tracks the visible
     height instead. Layered in @supports so a browser without it keeps the
     working `vh` layout above rather than dropping the whole rule. */
  @supports (height: 100dvh) {
    @media (max-width: 720px) {
      #app { grid-template-rows:minmax(0,58dvh) minmax(0,1fr); height:100dvh; }
    }
  }
</style>
</head>
<body>
<div id="app">
  <div id="map">
    <div id="tiles"></div>
    <div class="ctl"><button id="zin">+</button><button id="zout">−</button><button id="base" title="basemap: vector (drawn from the local bake) → OSM raster → ESRI satellite. Same slippy addresses, same HHTL key — the vector skin is the only one that needs no third-party tile server." style="width:auto;padding:0 8px;font-size:12px">osm</button><button id="feat" title="toggle real OSM feature overlay (points read from the baked RowSlab via /api/osm/features/:z/:x/:y)" style="width:auto;padding:0 8px;font-size:12px">features</button></div>
    <div class="hint">drag to pan · click a point for its HHTL key</div>
    <div class="feat-status" id="featStatus"></div>
    <div class="attr" id="attr">shapes from the local OSM bake · data © <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors (ODbL)</div>
  </div>
  <div id="panel">
    <h1>OSM cockpit</h1>
    <p class="sub">Geo domain <code>0x0F</code> · WebMercator ↔ HHTL</p>
    <div class="row"><span class="k">lon, lat</span><span class="v" id="ll">—</span></div>
    <div class="row"><span class="k">z / x / y</span><span class="v" id="zxy">—</span></div>
    <div class="tier">
      <div class="cell"><b id="heel">—</b><span>HEEL</span></div>
      <div class="cell"><b id="hip">—</b><span>HIP</span></div>
      <div class="cell"><b id="twig">—</b><span>TWIG</span></div>
      <div class="cell"><b id="leaf">—</b><span>LEAF</span></div>
    </div>
    <div id="feature"><p class="sub" style="margin:0">Click a feature dot to resolve its
    OSM identity and tags from the codebook sidecar.</p></div>
    <div class="row" style="margin-top:14px"><span class="k">tile source</span></div>
    <div style="padding:4px 0"><code id="src">—</code></div>
    <p class="sub" style="margin-top:16px">A quadtree <em>is</em> a cascade:
    <code>z/x/y</code> Morton-interleaves into the four 16-bit HHTL tiers
    (<code>GEO_V3_FACET</code> rails 0–3), so the map pyramid and the slab's own
    row key are one and the same address.</p>
  </div>
</div>
<script>
// ── slippy math (mirrors cockpit-server::osm_tiles) ──
function lon2x(lon,z){ return (lon+180)/360*Math.pow(2,z); }
function lat2y(lat,z){ const r=lat*Math.PI/180;
  return (1-Math.log(Math.tan(r)+1/Math.cos(r))/Math.PI)/2*Math.pow(2,z); }
function x2lon(x,z){ return x/Math.pow(2,z)*360-180; }
function y2lat(y,z){ const n=Math.PI-2*Math.PI*y/Math.pow(2,z);
  return 180/Math.PI*Math.atan(0.5*(Math.exp(n)-Math.exp(-n))); }

const map=document.getElementById('map'), tilesEl=document.getElementById('tiles');
let z=12;                       // Berlin-ish default view
let cx=lon2x(13.404954,z), cy=lat2y(52.520008,z);  // center in fractional tile units

// ── dual map: OSM (z/x/y) ↔ ESRI World Imagery satellite (z/y/x — row first!).
// Same slippy addresses, same HHTL keys — two skins. Mirrors cockpit-server::
// osm_tiles::{OSM_TILE_URL, SAT_TILE_URL}; the locate API reports both URLs.
// `vector` is FIRST and default: the map is drawn from our own bake, so a
// default page load touches no third-party tile server at all. The two raster
// skins stay reachable through the toggle — they are the reference you check
// our render against — but they are now an explicit opt-in, which is also what
// keeps a deployed host off the OSMF tile policy's toes.
const BASEMAPS={
  vector:{ src:null,   // drawn from /api/osm/geometry/tile, not fetched as images
        attr:'shapes from the local OSM bake · data © <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors (ODbL)',
        next:'osm' },
  osm:{ src:(z,x,y)=>`https://tile.openstreetmap.org/${z}/${x}/${y}.png`,
        attr:'© <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors',
        next:'sat' },
  sat:{ src:(z,x,y)=>`https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/${z}/${y}/${x}`,
        attr:'Powered by <a href="https://www.esri.com/">Esri</a> — Source: Esri, Maxar, Earthstar Geographics',
        next:'vector' },
};
let basemap='vector';

// ── real OSM feature overlay — GET /api/osm/features/:z/:x/:y over the
// baked RowSlab (cockpit-server::osm_features). Off by default (opt-in,
// same posture as the Garmin drape's toggle): the slab is a large local dev
// artifact (OSM_SLAB_PATH), not always present, and fetching it unasked
// would be surprising. Raw OSM-XYZ z/x/y goes straight to the endpoint —
// the SAME z/wx/ty a raster tile <img> uses. Since the V3 migration the
// panel's displayed HHTL address is the same key the slab is sorted by, so
// this is one address end to end; it was NOT true before that migration,
// which is why the endpoint takes raw z/x/y rather than a key.
let showFeatures=false;
const featureCache=new Map();   // "z/x/y" -> 'pending' | 'unavailable' | {total,returned,features}
function tileKey(z,x,y){ return `${z}/${x}/${y}`; }
function ensureFeatures(z,wx,ty){
  const key=tileKey(z,wx,ty);
  if(featureCache.has(key)) return;
  featureCache.set(key,'pending');
  fetch(`/api/osm/features/${z}/${wx}/${ty}`).then(r=>{
    if(r.status===503){ featureCache.set(key,'unavailable'); return null; }
    if(!r.ok) throw new Error('http '+r.status);
    return r.json();
  // paintFeatures(), NOT render(): a completed fetch adds one tile's dots and
  // changes nothing about the tile images or the transform, so rebuilding the
  // whole layer here is what produced the 24x append amplification.
  }).then(d=>{ if(d){ featureCache.set(key,d); paintFeatures(); } })
    .catch(()=>{ featureCache.delete(key); });  // transient failure: allow a retry on next render
}
function updateFeatStatus(visibleKeys){
  const el=document.getElementById('featStatus');
  // One status line, two possible owners. With the dots off, the vector
  // basemap owns it — blanking here unconditionally is what silently ate the
  // basemap's own "N shapes / M rows sampled" line, since render() paints the
  // basemap first and the features second.
  if(!showFeatures){
    if(BASEMAPS[basemap].src) el.textContent=''; else updateBaseStatus();
    return;
  }
  if(visibleKeys.some(k=>featureCache.get(k)==='unavailable')){
    el.textContent='no OSM slab baked (OSM_SLAB_PATH unset on the server)'; return;
  }
  if(visibleKeys.some(k=>featureCache.get(k)==='pending')){ el.textContent='loading features…'; return; }
  let returned=0, total=0;
  for(const k of visibleKeys){ const d=featureCache.get(k);
    if(d && typeof d==='object'){ returned+=d.returned; total+=d.total; } }
  el.textContent = total>returned ? `${returned} of ${total} features (tile cap hit)` : `${returned} features`;
}

// The tile cells covering the viewport. `tx` may sit outside [0,2^z) on a
// repeated world copy (low zoom / pan past ±180°); `wx` is it wrapped back
// into range, which is what the tile URL and the feature key both use.
function visibleGrid(){
  const w=map.clientWidth, h=map.clientHeight, n=Math.pow(2,z);
  const x0=Math.floor(cx - w/512)-1, x1=Math.floor(cx + w/512)+1;
  const y0=Math.floor(cy - h/512)-1, y1=Math.floor(cy + h/512)+1;
  const cells=[];
  for(let ty=y0;ty<=y1;ty++) for(let tx=x0;tx<=x1;tx++){
    if(ty<0||ty>=n) continue;
    cells.push({tx,ty,wx:((tx%n)+n)%n});
  }
  return cells;
}

// Cells whose feature dots are already in the DOM *for the current view*.
// Keyed by (tx,ty) and not by the tile key, because one tile drawn on two
// repeated world copies needs its own dots at each offset.
//
// This exists because feature fetches complete one at a time and each
// completion used to call render(), which clears #tiles and rebuilds EVERY
// marker. Measured on the Berlin bake at the default z12 view: 64,707 final
// markers cost 1,550,957 appendChild calls — 24x amplification, 33.4s to
// settle. That is the n^2/2 you get from redrawing all tiles drawn so far on
// each of ~49 arrivals. Painting only what is missing makes it 1x.
let drawnCells=new Set();

// The clicked feature's rehydrated SHAPE — the decode half of the .chains
// codec, drawn. A closed ring carrying an areal tag is FILLED (the operator's
// edge model: the ring is the shore; the tag says what is inside); an open
// chain is stroked. Classification is from tags the click already fetched.
function shapeLayer(){
  let svg=document.getElementById('shape');
  if(!svg){
    svg=document.createElementNS('http://www.w3.org/2000/svg','svg');
    svg.id='shape';
    svg.setAttribute('width','1'); svg.setAttribute('height','1');
    svg.style.cssText='position:absolute;left:0;top:0;overflow:visible;pointer-events:none;z-index:3';
  }
  if(svg.parentElement!==tilesEl) tilesEl.appendChild(svg); // render() clears #tiles
  return svg;
}
// Style per semantic class. The CLASS is the server's (one rule, applied the
// same way to one clicked way and to ten thousand basemap ways — see
// osm_features::ShapeClass); the LOOK is ours. This table is the only place
// colours live, so the basemap and the selection can never drift apart.
// The `texture` key names the Canvas2D FALLBACK pattern (see texture() below).
// The GL path carries a richer projected-template per class in its fragment
// shader, keyed by class index — same visual vocabulary, higher fidelity;
// building/green tone-noise is GL-only (the canvas fallback keeps them flat).
// Every fill is OPAQUE, and that is load-bearing rather than cosmetic. The
// previous palette was dark AND alpha'd (`…80`, `…4d`, `…66`), so each fill
// composited toward the ground colour instead of replacing it: one polygon
// read as a faint tint, and a stack of them in a dense district converged on
// the ground itself. The map could only ever get darker as it got busier —
// the opposite of what density should show. Opaque fills over a light land
// give the beige/green/blue a real basemap reads as, and make the draw order
// (FILL_PASS, broad cover → specific → water → buildings) the thing that
// decides what wins, which is what that ordering was written for.
//
// Tones follow OSM Carto closely enough to be legible to anyone who has seen
// a slippy map, because that vocabulary is the point of a basemap: green is
// vegetation, blue is water, beige-grey is built.
const CLASS_STYLE={
  water:    {fill:'#a8cfe0', stroke:'#7cb0c8', w:0.8, texture:'waves'},
  building: {fill:'#d9d0c9', stroke:'#b8a99b', w:0.5},
  wood:     {fill:'#add19e', stroke:'#8ab879', w:0.5, texture:'canopy'},
  green:    {fill:'#cdebb0', stroke:'#aed48f', w:0.5},
  rail:     {fill:'none',    stroke:'#9c8f82', w:0.9},
  road:     {fill:'none',    stroke:'#f5c777', w:1.1},
  other:    {fill:'none',    stroke:'#cec6bc', w:0.7},
  // Split out of `green` so a suburb stops reading as vegetation, and so the
  // kinds that SHOULD look like ground cover can carry a texture.
  meadow:   {fill:'#dcedc0', stroke:'#bcd79a', w:0.4, texture:'stipple'},
  park:     {fill:'#c8facc', stroke:'#98d9a0', w:0.5, texture:'stripes'},
  built:    {fill:'#e9e1d7', stroke:'#d2c6b8', w:0.4, texture:'speckle'},
};
function styleForClass(c){ return CLASS_STYLE[c] || CLASS_STYLE.other; }

// ── textures ─────────────────────────────────────────────────────────────
// A flat fill cannot tell a meadow from a park from a lawn, and at z16 those
// are most of the viewport — the operator's "meadows don't have a texture yet".
// Canvas gives this cheaply and SVG would not have: one small offscreen tile
// per texture, built once, handed to `createPattern` and reused for every
// polygon of that class. Cost is one pattern object per session, not per shape.
//
// The pattern is NOT transform-corrected on purpose: it is created in canvas
// space and we `translate()` per tile before filling, so the texture rides
// along with the map instead of crawling underneath it while panning.
const patternCache=new Map();
function texture(ctx,name,style){
  let p=patternCache.get(name);
  if(p!==undefined) return p;
  const s=(name==='waves'||name==='stripes')?16:8, off=document.createElement('canvas');
  off.width=s; off.height=s;
  const o=off.getContext('2d');
  o.fillStyle=style.fill; o.fillRect(0,0,s,s);
  o.strokeStyle=style.stroke; o.globalAlpha=0.5;
  if(name==='stipple'){
    // Sparse dots on the diagonal — grass, at any zoom, without banding.
    o.fillStyle=style.stroke;
    for(const [x,y] of [[1,1],[5,3],[3,6],[7,5]]) o.fillRect(x,y,1,1);
  } else if(name==='canopy'){
    // Short crossing strokes read as tree cover at a distance.
    o.lineWidth=0.7; o.beginPath();
    o.moveTo(0,6); o.lineTo(3,3); o.moveTo(4,8); o.lineTo(8,4);
    o.stroke();
  } else if(name==='waves'){
    // Two offset ripple rows — calm water, matching the GL band template.
    o.globalAlpha=0.35; o.lineWidth=0.8; o.beginPath();
    o.moveTo(0,4);  o.quadraticCurveTo(4,2,8,4);   o.quadraticCurveTo(12,6,16,4);
    o.moveTo(0,12); o.quadraticCurveTo(4,10,8,12); o.quadraticCurveTo(12,14,16,12);
    o.stroke();
  } else if(name==='stripes'){
    // Faint diagonals — the park lawn-stripe read.
    o.globalAlpha=0.25; o.lineWidth=1.2; o.beginPath();
    for(let k=-16;k<=16;k+=8){ o.moveTo(k,16); o.lineTo(k+16,0); }
    o.stroke();
  } else if(name==='speckle'){
    // Sparse concrete flecks.
    o.fillStyle=style.stroke; o.globalAlpha=0.6;
    for(const [x,y] of [[2,3],[6,1],[4,6],[7,7],[1,7]]) o.fillRect(x,y,1,1);
  }
  p=ctx.createPattern(off,'repeat');
  patternCache.set(name,p);
  return p;
}
function fillStyleFor(ctx,cls){
  const st=CLASS_STYLE[cls] || CLASS_STYLE.other;
  return st.texture ? (texture(ctx,st.texture,st) || st.fill) : st.fill;
}

// World-pixel point list for an SVG element, at the current zoom. `offset` is
// the repeated-world-copy shift the tile images already use.
function svgPoints(points, offset){
  return points.map(([lon,lat]) =>
    `${((lon2x(lon,z)+offset)*256).toFixed(1)},${(lat2y(lat,z)*256).toFixed(1)}`).join(' ');
}
function svgShape(g, offset, style, emphasis){
  const el=document.createElementNS('http://www.w3.org/2000/svg', g.closed?'polygon':'polyline');
  el.setAttribute('points',svgPoints(g.points,offset));
  el.setAttribute('fill', g.closed ? style.fill : 'none');
  el.setAttribute('stroke',style.stroke);
  el.setAttribute('stroke-width',style.w+(emphasis||0));
  el.setAttribute('vector-effect','non-scaling-stroke');
  return el;
}

async function showShape(idx){
  const svg=shapeLayer(); svg.innerHTML='';
  const r=await fetch(`/api/osm/geometry/${idx}`);
  if(!r.ok) return false;                       // a node: no chain is the answer
  const g=await r.json();
  // Emphasis, not a different colour: the selection must read as THIS thing
  // lit up, not as a differently-classified thing.
  const el=svgShape(g,0,styleForClass(g.class),1.5);
  el.setAttribute('stroke','#7ee787');
  svg.appendChild(el);
  return true;
}

// ── the basemap itself ───────────────────────────────────────────────────
// GET /api/osm/geometry/tile-bin/:z/:x/:y — the OSM1 binary LE wire, wired
// DIRECTLY into the renderer:
//
//   slab z32 cells ──(one multiply)──▶ LE f32 pairs on the wire
//     ──▶ ArrayBuffer ──(DataView lens, no JSON tree, no per-point objects)──▶
//       Path2D per (tile, class)  ◀── the ONE materialization, into the
//                                     renderer's own retained native form
//     ──▶ ~150 native draw calls per frame on a <canvas>
//
// The first version of this layer was 69k retained SVG DOM elements over a
// JSON wire — measured live at 69,125 shapes / ~40-60 MB per view, and
// "terribly slow" (operator). Both representations were wrong: SVG makes the
// browser re-rasterize every element on every pan frame, and serde-JSON on
// the hot path is the exact anti-pattern the house doctrine names
// (T3/ADR-022: to_le_bytes IS the wire). Canvas re-rasterizes from ~10 merged
// Path2D objects per tile instead.
const geomCache=new Map();  // "z/x/y" -> 'pending'|'unavailable'|{total,sampled,returned,fills,strokes}
let geomZoom=null;          // paths are built in this zoom's pixel space

// Wire codes, pinned to ShapeClass::wire_code on the server — the Rust test
// `wire_codes_are_pinned` and this array must agree, and the array index IS
// the wire byte.
const CLASS_ORDER=['water','building','wood','green','rail','road','other',
                  'meadow','park','built'];
// Areas back-to-front: the broad ground cover first, then the specific kinds
// on top of it, then water, then buildings — a lake sits on its meadow, a
// building on its block. Lines last, roads on top of everything.
const FILL_PASS=[9,7,3,8,2,0,1];        // built, meadow, green, park, wood, water, building
const STROKE_PASS=[9,7,3,8,2,0,1,6,4,5];// area outlines in the same order, then other, rail, road

function parseTileBin(buf){
  const dv=new DataView(buf);
  if(dv.byteLength<20 || dv.getUint32(0,true)!==0x314D534F) return null; // "OSM1"
  const total=dv.getUint32(4,true), sampled=dv.getUint32(8,true);
  const count=dv.getUint32(12,true), malformed=dv.getUint32(16,true);
  const fills=new Map(), strokes=new Map(), dots=new Map();
  const path=(m,c)=>{ let p=m.get(c); if(!p){ p=new Path2D(); m.set(c,p); } return p; };
  let at=20;
  for(let s=0;s<count;s++){
    if(at+8>dv.byteLength) return null;         // truncated: reject, refetch
    const cls=dv.getUint8(at+4), closed=dv.getUint8(at+5)===1;
    const n=dv.getUint16(at+6,true); at+=8;
    if(at+n*8>dv.byteLength) return null;
    // Float32Array is a LENS over the response buffer — the points are read
    // in place, never copied into JS objects. (byteOffset is 4-aligned by
    // construction: the header is 20 B and every record is 8+8n B.)
    const pts=new Float32Array(buf,at,n*2); at+=n*8;
    // n===1 is the server's sub-pixel degradation: no outline to trace, so it
    // becomes a 1px square of the class's dot colour. Same Path2D machinery,
    // so it costs one more map and nothing in the draw loop.
    if(n===1){ const dp=path(dots,cls); dp.rect(pts[0],pts[1],1,1); continue; }
    if(n<2) continue;
    const fp=closed?path(fills,cls):null, sp=path(strokes,cls);
    sp.moveTo(pts[0],pts[1]); if(fp) fp.moveTo(pts[0],pts[1]);
    for(let i=1;i<n;i++){ sp.lineTo(pts[2*i],pts[2*i+1]); if(fp) fp.lineTo(pts[2*i],pts[2*i+1]); }
    if(closed){ sp.closePath(); fp.closePath(); }
  }
  return {total,sampled,returned:count,malformed,fills,strokes,dots};
}

// ── WebGL2 renderer — hardware acceleration (operator-mandated) ─────────────
// Canvas2D above stays as the automatic fallback: same wire, same cache keys,
// same status line, chosen once at startup. The GL path replaces BOTH hot
// layers:
//   fills/strokes — Path2D re-rasterized per frame  -> retained per-tile VBOs
//   feature dots  — one DOM <div> per feature       -> one gl.POINTS draw
// The dot layer was the MEASURED lag: 57,690 features = 57,690 absolutely-
// positioned divs, the same retained-DOM architecture the module doc records
// as removed ("69k retained SVG DOM nodes... terribly slow") — it had quietly
// survived in the overlay while the basemap moved to canvas.
function hexRGBA(x){ const h=x.slice(1);
  return [parseInt(h.slice(0,2),16)/255, parseInt(h.slice(2,4),16)/255,
          parseInt(h.slice(4,6),16)/255, h.length>=8?parseInt(h.slice(6,8),16)/255:1]; }
const FILL_RGBA=CLASS_ORDER.map(n=>{const st=CLASS_STYLE[n];
  return st.fill==='none'?[0,0,0,0]:hexRGBA(st.fill);});
const STROKE_RGBA=CLASS_ORDER.map(n=>hexRGBA(CLASS_STYLE[n].stroke));
// A shape the server degraded to a dot is painted with its FILL colour: fill
// is what the eye integrates into landcover, and it is precisely what a
// sub-pixel outline loses. Line classes (road/rail/other) have no fill, so
// they fall back to their stroke — without that they would be transparent
// dots, which is the "merges into nothing" this whole path exists to fix.
const DOT_RGBA=CLASS_ORDER.map((n,i)=>FILL_RGBA[i][3]>0?FILL_RGBA[i]:STROKE_RGBA[i]);

let gl=null, glProg=null, glLoc=null, glCanvas=null, glSel=null;
function initGL(){
  const c=document.createElement('canvas');
  c.id='base-gl';
  c.style.cssText='position:absolute;left:0;top:0;pointer-events:none;';
  const g=c.getContext('webgl2',{alpha:true,antialias:true});
  if(!g) return false;                       // no WebGL2: Canvas2D path runs
  map.insertBefore(c,map.firstChild);
  glCanvas=c; gl=g;
  const vs=`#version 300 es
in vec2 aPos; in float aCls;
uniform vec2 uView; uniform vec2 uTrans; uniform float uPointPx;
out float vCls; out vec2 vWorld;
void main(){
  vec2 p=aPos+uTrans; vWorld=p; vCls=aCls;
  gl_Position=vec4(p.x*2.0/uView.x-1.0, 1.0-p.y*2.0/uView.y, 0.0, 1.0);
  gl_PointSize=uPointPx;
}`;
  // Procedural texture TEMPLATES, projected in vWorld (pre-pan-offset px) so
  // every pattern rides the map exactly like the translate()d canvas pattern
  // did. This is the Iceland/Havel recipe applied to the 2D map: material is
  // computed, never downloaded (the /garmin scenes draw 30M+ textured tris
  // from a height palette — same GPU, same budget class). Discipline:
  //   - colours come ONLY from uFill/uStroke (CLASS_STYLE is the single place
  //     colours live); templates MODULATE colour, they never introduce it.
  //   - every spatial frequency is an integer number of cycles per the 256-px
  //     fold W, and the value-noise lattice wraps at the same period, so the
  //     fold is seam-free while keeping coordinates small (mediump-safe, the
  //     same envelope the original mod-8 patterns lived in).
  const fs=`#version 300 es
precision mediump float;
in float vCls; in vec2 vWorld;
uniform vec4 uFill[10]; uniform vec4 uStroke[10]; uniform vec4 uDot[10]; uniform int uMode;
out vec4 frag;
#define TAU 6.2831853
// hash + wrap-aware value noise: vn(p,n) tiles seamlessly when p spans n cells.
float h21(vec2 p){ p=fract(p*vec2(0.1031,0.1097)); p+=dot(p,p.yx+19.19); return fract((p.x+p.y)*p.x); }
float vn(vec2 p,float n){
  vec2 i=floor(p), f=fract(p); f=f*f*(3.0-2.0*f);
  float a=h21(mod(i,n)), b=h21(mod(i+vec2(1.,0.),n)),
        c=h21(mod(i+vec2(0.,1.),n)), d=h21(mod(i+vec2(1.,1.),n));
  return mix(mix(a,b,f.x),mix(c,d,f.x),f.y);
}
void main(){
  if(uMode>=2){ vec2 d=gl_PointCoord-0.5; if(dot(d,d)>0.25) discard;
    // 4 = a basemap LOD dot: a shape too small to outline, painted in its own
    // class colour. Distinct from the feature dots (2) and selection (3),
    // which are UI marks and keep their fixed colours.
    if(uMode==4){ frag=uDot[int(vCls+0.5)]; return; }
    frag = uMode==3 ? vec4(0.494,0.906,0.529,1.0)   // selection #7ee787
                    : vec4(1.0,0.706,0.329,1.0);    // dot #ffb454
    return; }
  int c=int(vCls+0.5);
  if(uMode==1){ frag=uStroke[c]; return; }
  vec4 col=uFill[c];
  vec2 W=mod(vWorld,256.0);
  vec2 m=mod(vWorld,8.0);
  if(c==0){        // water — noise-warped ripple bands, calm and horizontal
    float w1=vn(W/32.0,8.0);
    float band=sin((W.y+10.0*w1)*TAU*12.0/256.0);
    col.rgb*=0.93+0.10*smoothstep(-0.2,0.9,band);
  } else if(c==1){ // building — coarse per-block tone so a block field reads
    float t=vn(W/32.0,8.0);          // as individual masses, not one slab
    col.rgb*=0.90+0.18*t;
  } else if(c==2){ // wood canopy — two-octave blobs: shadowed floor, lit crowns
    float n=0.65*vn(W/16.0,16.0)+0.35*vn(W/6.4,40.0);
    col.rgb=mix(col.rgb*0.82,col.rgb*1.18,smoothstep(0.35,0.75,n));
    col.a=min(1.0,col.a+0.15*smoothstep(0.60,0.80,n));
  } else if(c==3){ // green — gentle ground-cover tone variation
    col.rgb*=0.94+0.12*vn(W/16.0,16.0);
  } else if(c==7){ // meadow — the 4-dot stipple cells, over a soft tone wash
    col.rgb*=0.95+0.10*vn(W/32.0,8.0);
    if((m.x>=1.0&&m.x<2.0&&m.y>=1.0&&m.y<2.0)||(m.x>=5.0&&m.x<6.0&&m.y>=3.0&&m.y<4.0)||
       (m.x>=3.0&&m.x<4.0&&m.y>=6.0&&m.y<7.0)||(m.x>=7.0&&m.y>=5.0&&m.y<6.0))
      col=vec4(uStroke[7].rgb, min(1.0,col.a+0.35));
  } else if(c==8){ // park — faint diagonal lawn stripes (mown-grass read)
    col.rgb*=0.96+0.05*sin((W.x+W.y)*TAU*10.0/256.0);
  } else if(c==9){ // built — sparse concrete flecks
    if(h21(mod(floor(W/2.0),128.0))>0.985) col.rgb*=1.25;
  }
  frag=col;
}`;
  const sh=(t,src)=>{ const o=g.createShader(t); g.shaderSource(o,src); g.compileShader(o);
    if(!g.getShaderParameter(o,g.COMPILE_STATUS)) throw new Error(g.getShaderInfoLog(o));
    return o; };
  glProg=g.createProgram();
  g.attachShader(glProg,sh(g.VERTEX_SHADER,vs));
  g.attachShader(glProg,sh(g.FRAGMENT_SHADER,fs));
  g.linkProgram(glProg);
  if(!g.getProgramParameter(glProg,g.LINK_STATUS)) throw new Error(g.getProgramInfoLog(glProg));
  g.useProgram(glProg);
  glLoc={ aPos:g.getAttribLocation(glProg,'aPos'), aCls:g.getAttribLocation(glProg,'aCls'),
          uView:g.getUniformLocation(glProg,'uView'), uTrans:g.getUniformLocation(glProg,'uTrans'),
          uMode:g.getUniformLocation(glProg,'uMode'), uPointPx:g.getUniformLocation(glProg,'uPointPx') };
  g.uniform4fv(g.getUniformLocation(glProg,'uFill'), FILL_RGBA.flat());
  g.uniform4fv(g.getUniformLocation(glProg,'uStroke'), STROKE_RGBA.flat());
  g.uniform4fv(g.getUniformLocation(glProg,'uDot'), DOT_RGBA.flat());
  g.enable(g.BLEND); g.blendFunc(g.SRC_ALPHA,g.ONE_MINUS_SRC_ALPHA);
  return true;
}

// Ear-clipping triangulation for one simple ring (no holes — the wire delivers
// per-way rings; multipolygon inners arrive as their own ways, matching what
// the Canvas2D nonzero fill drew). Handles CW and CCW input. A degenerate or
// self-intersecting ring falls back to a fan over whatever remains rather than
// looping forever — draw something sane, never hang.
function earClip(pts,n){
  const idx=[]; for(let i=0;i<n;i++) idx.push(i);
  if(n>1 && pts[0]===pts[2*(n-1)] && pts[1]===pts[2*(n-1)+1]) idx.pop();
  let m=idx.length;
  if(m<3) return [];
  let area=0;
  for(let i=0;i<m;i++){ const j=(i+1)%m;
    area+=pts[2*idx[i]]*pts[2*idx[j]+1]-pts[2*idx[j]]*pts[2*idx[i]+1]; }
  const ccw=area>0, out=[];
  let guard=m*m+m, i=0;
  while(m>3 && guard-->0){
    const i0=idx[(i+m-1)%m], i1=idx[i%m], i2=idx[(i+1)%m];
    const ax=pts[2*i0],ay=pts[2*i0+1],bx=pts[2*i1],by=pts[2*i1+1],qx=pts[2*i2],qy=pts[2*i2+1];
    const cross=(bx-ax)*(qy-ay)-(by-ay)*(qx-ax);
    let ear=ccw?cross>0:cross<0;
    if(ear){
      for(let k=0;k<m&&ear;k++){
        const pv=idx[k]; if(pv===i0||pv===i1||pv===i2) continue;
        const px=pts[2*pv],py=pts[2*pv+1];
        const d1=(bx-ax)*(py-ay)-(by-ay)*(px-ax);
        const d2=(qx-bx)*(py-by)-(qy-by)*(px-bx);
        const d3=(ax-qx)*(py-qy)-(ay-qy)*(px-qx);
        if(!(((d1<0)||(d2<0)||(d3<0)) && ((d1>0)||(d2>0)||(d3>0)))) ear=false;
      }
    }
    if(ear){ out.push(ax,ay,bx,by,qx,qy); idx.splice(i%m,1); m--; i=0; }
    else i=(i+1)%m;
  }
  if(m>=3){
    if(guard<=0){       // bail: fan the remainder from its first vertex
      for(let k=1;k<m-1;k++)
        out.push(pts[2*idx[0]],pts[2*idx[0]+1],pts[2*idx[k]],pts[2*idx[k]+1],
                 pts[2*idx[k+1]],pts[2*idx[k+1]+1]);
    } else {
      out.push(pts[2*idx[0]],pts[2*idx[0]+1],pts[2*idx[1]],pts[2*idx[1]+1],
               pts[2*idx[2]],pts[2*idx[2]+1]);
    }
  }
  return out;
}

// The GL parse: same wire walk as parseTileBin, but the ONE materialization is
// vertex arrays instead of Path2D — triangles for fills (ear-clipped, appended
// in FILL_PASS painter order so a single draw preserves layering), gl.LINES
// segments for strokes in STROKE_PASS order. cls rides interleaved as the
// third float, indexing the color/texture tables in the shader.
function parseTileBinGL(buf){
  const dv=new DataView(buf);
  if(dv.byteLength<20 || dv.getUint32(0,true)!==0x314D534F) return null;
  const total=dv.getUint32(4,true), sampled=dv.getUint32(8,true);
  const count=dv.getUint32(12,true), malformed=dv.getUint32(16,true);
  const fillsBy=new Map(), linesBy=new Map(), dotsBy=new Map();
  let at=20;
  for(let s=0;s<count;s++){
    if(at+8>dv.byteLength) return null;
    const cls=dv.getUint8(at+4), closed=dv.getUint8(at+5)===1;
    const n=dv.getUint16(at+6,true); at+=8;
    if(at+n*8>dv.byteLength) return null;
    const pts=new Float32Array(buf,at,n*2); at+=n*8;
    // n===1 is the server's sub-pixel degradation (dot_if_subpixel): a shape
    // with no resolvable outline, carrying only its class. Drawing it as a
    // dot is the whole point — falling through to `n<2 continue` would drop
    // the dense core entirely.
    if(n===1){ let D=dotsBy.get(cls); if(!D){ D=[]; dotsBy.set(cls,D); }
      D.push(pts[0],pts[1]); continue; }
    if(n<2) continue;
    let L=linesBy.get(cls); if(!L){ L=[]; linesBy.set(cls,L); }
    for(let i=0;i<n-1;i++) L.push(pts[2*i],pts[2*i+1],pts[2*i+2],pts[2*i+3]);
    if(closed){
      L.push(pts[2*n-2],pts[2*n-1],pts[0],pts[1]);
      const tris=earClip(pts,n);
      if(tris.length){ let F=fillsBy.get(cls); if(!F){ F=[]; fillsBy.set(cls,F); }
        for(const v of tris) F.push(v); }
    }
  }
  const fill=[], line=[], dot=[];
  for(const c of FILL_PASS){ const F=fillsBy.get(c); if(!F) continue;
    for(let i=0;i<F.length;i+=2) fill.push(F[i],F[i+1],c); }
  for(const c of STROKE_PASS){ const L=linesBy.get(c); if(!L) continue;
    for(let i=0;i<L.length;i+=2) line.push(L[i],L[i+1],c); }
  // Dots ride FILL_PASS order too, so a dotted lake still lands under a
  // dotted building rather than depending on row order in the tile.
  for(const c of FILL_PASS.concat(STROKE_PASS)){ const D=dotsBy.get(c); if(!D) continue;
    dotsBy.delete(c);
    for(let i=0;i<D.length;i+=2) dot.push(D[i],D[i+1],c); }
  return {total,sampled,returned:count,malformed,
          fill:new Float32Array(fill), line:new Float32Array(line),
          dot:new Float32Array(dot), vboF:null, vboL:null, vboD:null};
}

// Delete a zoom level's GPU buffers when its tiles are evicted — the cache
// clear alone would leak them (GL objects are not garbage-collected with the
// JS entry that referenced them).
function evictGLBuffers(){
  for(const d of geomCache.values()){
    if(d && typeof d==='object'){
      if(d.vboF) gl.deleteBuffer(d.vboF);
      if(d.vboL) gl.deleteBuffer(d.vboL);
      if(d.vboD) gl.deleteBuffer(d.vboD);
    }
  }
}

function ensureGeometry(z,wx,ty){
  const key=tileKey(z,wx,ty);
  if(geomCache.has(key)) return;
  geomCache.set(key,'pending');
  fetch(`/api/osm/geometry/tile-bin/${z}/${wx}/${ty}`).then(r=>{
    if(r.status===503){ geomCache.set(key,'unavailable'); return null; }
    if(!r.ok) throw new Error('http '+r.status);
    return r.arrayBuffer();
  // scheduleBaseDraw(), NOT render(): one tile's shapes arriving changes
  // nothing about the transform or the other tiles.
  }).then(buf=>{
    if(!buf) return;
    const d=gl?parseTileBinGL(buf):parseTileBin(buf);
    if(d){ geomCache.set(key,d); scheduleBaseDraw(); }
    else geomCache.delete(key);                  // bad payload: allow a retry
  }).catch(()=>{ geomCache.delete(key); });      // transient: retry on next render
}

// One viewport-sized canvas UNDER #tiles (dots and the selection SVG stay
// above it). Not inside #tiles: the canvas repaints from Path2D per frame
// anyway, so it doesn't ride the CSS transform — it just draws at the offset
// the transform encodes.
let baseCanvas=null, baseCtx=null, baseDrawQueued=false;
function baseLayerCanvas(){
  if(!baseCanvas){
    baseCanvas=document.createElement('canvas');
    baseCanvas.id='base-shapes';
    baseCanvas.style.cssText='position:absolute;left:0;top:0;pointer-events:none;';
    map.insertBefore(baseCanvas,map.firstChild);
    baseCtx=baseCanvas.getContext('2d');
  }
  const dpr=window.devicePixelRatio||1;
  const w=map.clientWidth, h=map.clientHeight;
  if(baseCanvas.width!==Math.round(w*dpr)||baseCanvas.height!==Math.round(h*dpr)){
    baseCanvas.width=Math.round(w*dpr); baseCanvas.height=Math.round(h*dpr);
    baseCanvas.style.width=w+'px'; baseCanvas.style.height=h+'px';
  }
  baseCtx.setTransform(dpr,0,0,dpr,0,0);
  return baseCtx;
}
// rAF-throttled: pan fires pointermove far faster than the display refreshes,
// and one draw per frame is both sufficient and the cheapest correct rate.
function scheduleBaseDraw(){
  if(baseDrawQueued) return;
  baseDrawQueued=true;
  requestAnimationFrame(()=>{ baseDrawQueued=false; drawBase(); });
}
function drawBase(){
  if(gl) return drawBaseGL();
  const ctx=baseLayerCanvas();
  const w=map.clientWidth, h=map.clientHeight;
  ctx.clearRect(0,0,w,h);
  if(BASEMAPS[basemap].src) return;            // a raster skin is active
  // Path2D geometry is tile-relative at ONE zoom; crossing zooms would draw
  // the old zoom's pixel space into the new one's. Evict, don't reuse — this
  // also bounds memory, which the SVG version never did (its cache kept every
  // zoom ever visited).
  if(z!==geomZoom){ geomCache.clear(); geomZoom=z; }
  const ox=w/2 - cx*256, oy=h/2 - cy*256;
  for(const {tx,ty,wx} of visibleGrid()){
    ensureGeometry(z,wx,ty);
    const d=geomCache.get(tileKey(z,wx,ty));
    if(!(d && typeof d==='object')) continue;
    ctx.save();
    ctx.translate(tx*256+ox, ty*256+oy);
    for(const c of FILL_PASS){
      const p=d.fills.get(c); if(!p) continue;
      ctx.fillStyle=fillStyleFor(ctx,CLASS_ORDER[c]); ctx.fill(p);
    }
    // Sub-pixel shapes, as 1px squares of their class colour — drawn after the
    // fills and before the strokes, so roads stay legible over the texture.
    // STROKE_PASS (not FILL_PASS) because it enumerates all ten classes: a
    // dotted road or rail has to land somewhere too.
    if(d.dots) for(const c of STROKE_PASS){
      const p=d.dots.get(c); if(!p) continue;
      const st=styleForClass(CLASS_ORDER[c]);
      ctx.fillStyle = st.fill==='none' ? st.stroke : st.fill;
      ctx.fill(p);
    }
    for(const c of STROKE_PASS){
      const p=d.strokes.get(c); if(!p) continue;
      const st=styleForClass(CLASS_ORDER[c]);   // unknown byte -> `other`, never undefined
      ctx.strokeStyle=st.stroke; ctx.lineWidth=st.w; ctx.stroke(p);
    }
    ctx.restore();
  }
  updateBaseStatus();
}
// The GL frame. Uploads a tile's arrays once (on first sight) and thereafter
// re-draws by binding — the per-frame cost is bind + uniform + drawArrays, so
// panning never re-walks geometry. `uTrans` carries the pan offset, so no
// vertex data is rewritten while dragging.
function drawBaseGL(){
  const dpr=window.devicePixelRatio||1;
  const w=map.clientWidth, h=map.clientHeight;
  if(glCanvas.width!==Math.round(w*dpr)||glCanvas.height!==Math.round(h*dpr)){
    glCanvas.width=Math.round(w*dpr); glCanvas.height=Math.round(h*dpr);
    glCanvas.style.width=w+'px'; glCanvas.style.height=h+'px';
  }
  gl.viewport(0,0,glCanvas.width,glCanvas.height);
  gl.clearColor(0,0,0,0); gl.clear(gl.COLOR_BUFFER_BIT);
  if(BASEMAPS[basemap].src) return;              // a raster skin is active
  if(z!==geomZoom){ evictGLBuffers(); geomCache.clear(); geomZoom=z; }
  gl.useProgram(glProg);
  gl.uniform2f(glLoc.uView,w,h);
  const ox=w/2 - cx*256, oy=h/2 - cy*256;
  const bind=(vbo)=>{
    gl.bindBuffer(gl.ARRAY_BUFFER,vbo);
    gl.enableVertexAttribArray(glLoc.aPos);
    gl.vertexAttribPointer(glLoc.aPos,2,gl.FLOAT,false,12,0);
    gl.enableVertexAttribArray(glLoc.aCls);
    gl.vertexAttribPointer(glLoc.aCls,1,gl.FLOAT,false,12,8);
  };
  for(const {tx,ty,wx} of visibleGrid()){
    ensureGeometry(z,wx,ty);
    const d=geomCache.get(tileKey(z,wx,ty));
    if(!(d && typeof d==='object' && d.fill)) continue;
    if(!d.vboF){
      d.vboF=gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER,d.vboF);
      gl.bufferData(gl.ARRAY_BUFFER,d.fill,gl.STATIC_DRAW);
      d.vboL=gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER,d.vboL);
      gl.bufferData(gl.ARRAY_BUFFER,d.line,gl.STATIC_DRAW);
      d.vboD=gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER,d.vboD);
      gl.bufferData(gl.ARRAY_BUFFER,d.dot||new Float32Array(0),gl.STATIC_DRAW);
    }
    gl.uniform2f(glLoc.uTrans, tx*256+ox, ty*256+oy);
    if(d.fill.length){ gl.uniform1i(glLoc.uMode,0); bind(d.vboF);
      gl.drawArrays(gl.TRIANGLES,0,d.fill.length/3); }
    // Between fill and line ON PURPOSE: the dots are the texture of a dense
    // district, the road strokes are its structure, and the structure has to
    // stay readable on top of the texture.
    if(d.dot&&d.dot.length){ gl.uniform1i(glLoc.uMode,4);
      gl.uniform1f(glLoc.uPointPx,1.5*(window.devicePixelRatio||1));
      bind(d.vboD); gl.drawArrays(gl.POINTS,0,d.dot.length/3); }
    if(d.line.length){ gl.uniform1i(glLoc.uMode,1); bind(d.vboL);
      gl.drawArrays(gl.LINES,0,d.line.length/3); }
  }
  drawDotsGL(ox,oy);
  updateBaseStatus();
}

// The feature dots, as ONE gl.POINTS draw per tile instead of one DOM div per
// feature. This is the measured lag fix: 57,690 divs -> 1 buffer + 1 draw.
// Hit-testing moves to a nearest-point search over the same Float32Array
// (`pickFeature`), so a click still resolves to an idx without any DOM node.
let dotBuf=null;
function drawDotsGL(ox,oy){
  if(!showFeatures) return;
  if(!dotBuf) dotBuf=gl.createBuffer();
  gl.uniform1i(glLoc.uMode,2);
  gl.uniform1f(glLoc.uPointPx,6*(window.devicePixelRatio||1));
  for(const {tx,ty,wx} of visibleGrid()){
    ensureFeatures(z,wx,ty);
    const d=featureCache.get(tileKey(z,wx,ty));
    if(!(d && typeof d==='object')) continue;
    if(!d.pts){
      const a=new Float32Array(d.features.length*3);
      const offset=tx-wx;
      d.features.forEach((f,i)=>{ a[3*i]=(lon2x(f.lon,z)+offset)*256 - tx*256;
        a[3*i+1]=lat2y(f.lat,z)*256 - ty*256; a[3*i+2]=0; });
      d.pts=a;
    }
    if(!d.pts.length) continue;
    gl.bindBuffer(gl.ARRAY_BUFFER,dotBuf);
    gl.bufferData(gl.ARRAY_BUFFER,d.pts,gl.DYNAMIC_DRAW);
    gl.enableVertexAttribArray(glLoc.aPos);
    gl.vertexAttribPointer(glLoc.aPos,2,gl.FLOAT,false,12,0);
    gl.enableVertexAttribArray(glLoc.aCls);
    gl.vertexAttribPointer(glLoc.aCls,1,gl.FLOAT,false,12,8);
    gl.uniform2f(glLoc.uTrans, tx*256+ox, ty*256+oy);
    gl.drawArrays(gl.POINTS,0,d.pts.length/3);
  }
  if(glSel){                                   // the selected dot, drawn lit
    gl.uniform1i(glLoc.uMode,3);
    gl.uniform1f(glLoc.uPointPx,10*(window.devicePixelRatio||1));
    gl.bindBuffer(gl.ARRAY_BUFFER,dotBuf);
    gl.bufferData(gl.ARRAY_BUFFER,new Float32Array([glSel.x,glSel.y,0]),gl.DYNAMIC_DRAW);
    gl.enableVertexAttribArray(glLoc.aPos);
    gl.vertexAttribPointer(glLoc.aPos,2,gl.FLOAT,false,12,0);
    gl.uniform2f(glLoc.uTrans, ox, oy);
    gl.drawArrays(gl.POINTS,0,1);
  }
}

// Click -> nearest feature, over the SAME arrays the GPU drew. Replaces the
// per-div click target: screen px -> world px, then a linear scan of visible
// tiles within a 6px radius (tiles are small and the scan is per-click, not
// per-frame).
function pickFeature(px,py){
  if(!showFeatures) return null;
  const w=map.clientWidth, h=map.clientHeight;
  const wxp=cx*256+(px-w/2), wyp=cy*256+(py-h/2);
  let best=null, bestD=36;                     // 6px radius, squared
  for(const {tx,ty,wx} of visibleGrid()){
    const d=featureCache.get(tileKey(z,wx,ty));
    if(!(d && typeof d==='object' && d.pts)) continue;
    for(let i=0;i<d.pts.length/3;i++){
      const gx=d.pts[3*i]+tx*256, gy=d.pts[3*i+1]+ty*256;
      const dd=(gx-wxp)*(gx-wxp)+(gy-wyp)*(gy-wyp);
      if(dd<bestD){ bestD=dd; best={idx:d.features[i].idx,x:gx,y:gy}; }
    }
  }
  return best;
}

function updateBaseStatus(){
  const el=document.getElementById('featStatus');
  if(BASEMAPS[basemap].src || showFeatures) return;   // features own the line then
  const keys=visibleGrid().map(({wx,ty})=>tileKey(z,wx,ty));
  if(keys.some(k=>geomCache.get(k)==='unavailable')){
    el.textContent='no OSM slab baked (OSM_SLAB_PATH unset on the server)'; return;
  }
  if(keys.some(k=>geomCache.get(k)==='pending')){ el.textContent='drawing from the bake…'; return; }
  let shapes=0, total=0, sampled=0;
  for(const k of keys){ const d=geomCache.get(k);
    if(d && typeof d==='object'){ shapes+=d.returned; total+=d.total; sampled+=d.sampled; } }
  // `sampled < total` is the honest signal that this zoom is an LOD sample and
  // not the whole map — without it a thin basemap reads as missing data.
  el.textContent = sampled<total
    ? `${shapes} shapes · ${sampled} of ${total} rows sampled at z${z}`
    : `${shapes} shapes from ${total} rows`;
}

function paintFeatures(){
  const visibleKeys=[];
  // Under GL the dots are a GPU draw (drawDotsGL) — no DOM nodes at all. Only
  // the status line still needs the per-tile walk.
  if(gl){
    if(showFeatures) for(const {wx,ty} of visibleGrid()){
      visibleKeys.push(tileKey(z,wx,ty)); ensureFeatures(z,wx,ty); }
    updateFeatStatus(visibleKeys);
    scheduleBaseDraw();
    return;
  }
  if(showFeatures){
    for(const {tx,ty,wx} of visibleGrid()){
      const key=tileKey(z,wx,ty);
      visibleKeys.push(key);
      ensureFeatures(z,wx,ty);
      const cell=tx+','+ty;
      if(drawnCells.has(cell)) continue;
      const d=featureCache.get(key);
      if(!(d && typeof d==='object')) continue;
      drawnCells.add(cell);
      // lon2x() returns a coordinate wrapped into [0,2^z), so re-add the same
      // wrap offset the tile image itself used to place it under the correct
      // world copy.
      const offset=tx-wx;
      for(const f of d.features){
        const dot=document.createElement('div');
        dot.className='pt';
        dot.dataset.idx=f.idx;          // handle for /api/osm/feature/:idx
        dot.style.left=((lon2x(f.lon,z)+offset)*256)+'px';
        dot.style.top=(lat2y(f.lat,z)*256)+'px';
        tilesEl.appendChild(dot);
      }
    }
  }
  updateFeatStatus(visibleKeys);
}

function render(){
  const w=map.clientWidth, h=map.clientHeight;
  tilesEl.innerHTML=''; drawnCells=new Set();
  // pixel offset of the map center
  const ox=w/2 - cx*256, oy=h/2 - cy*256;
  tilesEl.style.transform=`translate(${ox}px,${oy}px)`;
  const bm=BASEMAPS[basemap];
  // No `src` is the vector basemap: the map is drawn, not downloaded.
  if(bm.src){
    scheduleBaseDraw();   // clears the vector canvas under the raster tiles
    for(const {tx,ty,wx} of visibleGrid()){
      const img=new Image();
      img.src=bm.src(z,wx,ty);
      img.style.left=(tx*256)+'px'; img.style.top=(ty*256)+'px';
      tilesEl.appendChild(img);
    }
  } else {
    scheduleBaseDraw();
  }
  paintFeatures();
}

// ── panning ── (moved: a drag that actually panned, so the trailing click is
// suppressed — mouseup nulls `drag` before `click` fires, so `drag` alone can't
// gate it)
//
// POINTER events, not mouse events: they are the one API that covers mouse,
// touch and pen, so a finger pans the map on a phone without a second parallel
// code path. (Mouse-only handlers are why the map was frozen on touch; the
// `touch-action:none` in the CSS is the other half — without it the browser
// takes the gesture before `pointermove` ever fires.)
//
// `pointerId` is tracked so a second finger cannot fight the first for the
// pan: the map follows the pointer that started it and ignores the rest,
// which is also what keeps a pinch from being read as a violent drag.
let drag=null, moved=false, dragId=null;
map.addEventListener('pointerdown',e=>{
  if(dragId!==null) return;                 // already panning with another pointer
  dragId=e.pointerId; drag={x:e.clientX,y:e.clientY}; moved=false;
  map.classList.add('drag'); });
window.addEventListener('pointerup',e=>{
  if(e.pointerId!==dragId) return;
  dragId=null; drag=null; map.classList.remove('drag'); });
window.addEventListener('pointercancel',e=>{
  if(e.pointerId!==dragId) return;
  dragId=null; drag=null; map.classList.remove('drag'); });
window.addEventListener('pointermove',e=>{ if(!drag||e.pointerId!==dragId) return; moved=true;
  cx-=(e.clientX-drag.x)/256; cy-=(e.clientY-drag.y)/256; drag={x:e.clientX,y:e.clientY}; render(); });

// ── zoom ── (stopPropagation so the control click doesn't bubble to the map's
// click→locate handler; the buttons live inside #map)
function zoom(d){ const nz=Math.max(0,Math.min(19,z+d)); if(nz===z) return;
  const f=Math.pow(2,nz-z); cx*=f; cy*=f; z=nz; render(); }
document.getElementById('zin').onclick=e=>{ e.stopPropagation(); zoom(1); };
document.getElementById('zout').onclick=e=>{ e.stopPropagation(); zoom(-1); };
// basemap toggle — the button names the OTHER skin (what you'll switch TO);
// attribution follows the active source (OSM contributors ↔ Esri).
document.getElementById('base').onclick=e=>{ e.stopPropagation();
  basemap=BASEMAPS[basemap].next;
  e.target.textContent=BASEMAPS[basemap].next;
  document.getElementById('attr').innerHTML=BASEMAPS[basemap].attr;
  render(); };
document.getElementById('feat').onclick=e=>{ e.stopPropagation();
  showFeatures=!showFeatures;
  e.target.classList.toggle('active',showFeatures);
  render(); };
map.addEventListener('wheel',e=>{ e.preventDefault(); zoom(e.deltaY<0?1:-1); },{passive:false});

// ── click → server-side HHTL key ──
// Resolve one feature: ordinals -> strings via the .books codebook sidecar.
// Kept separate from the locate call because they answer different questions —
// locate is "where am I", this is "what is THAT".
async function showFeature(idx, el){
  const box=document.getElementById('feature');
  // DOM selection only exists on the Canvas2D path; under GL the lit dot is
  // `glSel`, drawn by drawDotsGL.
  document.querySelectorAll('.pt.sel').forEach(n=>n.classList.remove('sel'));
  if(el) el.classList.add('sel');
  box.innerHTML='<p class="sub" style="margin:0">resolving…</p>';
  try{
    const d=await (await fetch(`/api/osm/feature/${idx}`)).json();
    if(d.error){ box.innerHTML='<p class="sub" style="margin:0">'+d.error+'</p>'; return; }
    const rows=Object.entries(d.tags||{})
      .map(([k,v])=>`<div class="tag"><b>${k}</b><span>${v}</span></div>`).join('');
    const drawn=await showShape(idx);
    box.innerHTML =
      `<div class="row"><span class="k">osm key</span><span class="v">${d.osm_key||'—'}</span></div>`
      + (rows || '<p class="sub" style="margin:6px 0 0">no tags on this element</p>')
      + (drawn ? '' : '<p class="sub" style="margin:6px 0 0">point feature — no shape chain</p>');
  }catch(err){ box.innerHTML='<p class="sub" style="margin:0">lookup failed: '+err+'</p>'; }
}

map.addEventListener('click',async e=>{
  if(moved){ moved=false; return; }   // this "click" ended a pan — ignore it
  const r=map.getBoundingClientRect();
  const px=e.clientX-r.left, py=e.clientY-r.top;
  if(gl){
    const hit=pickFeature(px,py);     // nearest point, no DOM target to test
    if(hit){ glSel={x:hit.x,y:hit.y}; scheduleBaseDraw(); showFeature(hit.idx,null); }
  } else if(e.target.classList && e.target.classList.contains('pt')){
    showFeature(e.target.dataset.idx, e.target);
  }
  const w=map.clientWidth, h=map.clientHeight;
  const n=Math.pow(2,z);
  // wrap the horizontal tile index into [0,2^z) so clicks on a repeated world
  // copy (low zoom / across ±180°) send an in-range longitude, not an edge-clamped one
  const fx=(((cx+(px-w/2)/256)%n)+n)%n, fy=cy+(py-h/2)/256;
  const lon=x2lon(fx,z), lat=y2lat(fy,z);
  try{
    const res=await fetch(`/api/osm/locate?lon=${lon}&lat=${lat}&z=${z}`);
    const d=await res.json();
    document.getElementById('ll').textContent=`${lon.toFixed(5)}, ${lat.toFixed(5)}`;
    document.getElementById('zxy').textContent=`${d.z} / ${d.x} / ${d.y}`;
    document.getElementById('heel').textContent='0x'+d.hhtl.heel.toString(16).padStart(4,'0');
    document.getElementById('hip').textContent='0x'+d.hhtl.hip.toString(16).padStart(4,'0');
    document.getElementById('twig').textContent='0x'+d.hhtl.twig.toString(16).padStart(4,'0');
    document.getElementById('leaf').textContent='0x'+d.hhtl.leaf.toString(16).padStart(4,'0');
    // tile source follows the ACTIVE basemap (the locate API reports both URLs) —
    // on satellite, showing the OSM URL would mismatch the visible tiles.
    // The readout names the source actually in use. On the vector basemap that
    // is OUR endpoint — printing tile.openstreetmap.org while nothing is being
    // fetched from it was the exact confusion this view has to stop causing.
    document.getElementById('src').textContent =
      basemap==='vector' ? `${location.origin}/api/osm/geometry/tile-bin/${d.z}/${d.x}/${d.y}`
      : basemap==='sat'  ? d.sat_tile_url : d.tile_url;
  }catch(err){ document.getElementById('src').textContent='locate failed: '+err; }
});

// Hardware acceleration is the default and the fallback is automatic: if
// WebGL2 is unavailable (or the context is lost / shaders fail to compile) the
// Canvas2D path above runs unchanged. `initGL` is the ONLY switch — every
// draw/pick site branches on `gl`.
try{ if(!initGL()) console.info('osm: WebGL2 unavailable — Canvas2D fallback'); }
catch(err){ gl=null; console.warn('osm: WebGL2 init failed, using Canvas2D —',err); }
if(gl) glCanvas.addEventListener('webglcontextlost',e=>{
  // A lost context must not leave a blank map: drop to Canvas2D and rebuild.
  e.preventDefault(); gl=null; geomCache.clear(); geomZoom=null; render();
});

addEventListener('resize',render);
render();
</script>
</body>
</html>
"##;
