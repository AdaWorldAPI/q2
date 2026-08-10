//! The `/OSM` cockpit — a self-contained slippy-map page over the OSM tile
//! material ([`crate::osm_tiles`]). The Geo-domain (`0x0F`) sibling of the FMA
//! body-helix cockpit: pan/zoom OSM raster tiles from the standard source, and
//! read each tile's HHTL (HEEL/HIP/TWIG) key live from `/api/osm/locate`.
//!
//! The page is a single inline HTML string (the `/mri` cockpit pattern): no
//! build step, no external JS. Tiles are `<img>` fetched directly from
//! `tile.openstreetmap.org`; the HHTL address is resolved server-side so the
//! map pyramid and the cascade address stay the same source of truth.

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
  #map { position:relative; overflow:hidden; cursor:grab; background:#11151c; }
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
  .tier { display:grid; grid-template-columns:repeat(3,1fr); gap:8px; margin:12px 0; }
  .cell { background:#141b24; border:1px solid #223; border-radius:6px; padding:8px;
    text-align:center; }
  .cell b { display:block; font-size:16px; color:#8fd6ff; }
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
  .pt { position:absolute; width:5px; height:5px; margin:-3px 0 0 -3px;
    border-radius:50%; background:#ffb454; box-shadow:0 0 0 1px #0b0e13aa;
    pointer-events:none; }
  .feat-status { position:absolute; right:12px; top:12px; z-index:5;
    background:#0b0e13aa; padding:2px 6px; border-radius:4px; color:#8fa0b8; }
</style>
</head>
<body>
<div id="app">
  <div id="map">
    <div id="tiles"></div>
    <div class="ctl"><button id="zin">+</button><button id="zout">−</button><button id="base" title="basemap: OSM map ↔ ESRI satellite (same tiles, same HHTL address — two skins)" style="width:auto;padding:0 8px;font-size:12px">sat</button><button id="feat" title="toggle real OSM feature overlay (points read from the baked RowSlab via /api/osm/features/:z/:x/:y)" style="width:auto;padding:0 8px;font-size:12px">features</button></div>
    <div class="hint">drag to pan · click a point for its HHTL key</div>
    <div class="feat-status" id="featStatus"></div>
    <div class="attr" id="attr">© <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors</div>
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
    </div>
    <div class="row"><span class="k">tile source</span></div>
    <div style="padding:4px 0"><code id="src">—</code></div>
    <p class="sub" style="margin-top:16px">A quadtree <em>is</em> a cascade:
    <code>z/x/y</code> Morton-interleaves into the three 16-bit HHTL tiers, so
    the map pyramid and the semantic address are one and the same.</p>
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
const BASEMAPS={
  osm:{ src:(z,x,y)=>`https://tile.openstreetmap.org/${z}/${x}/${y}.png`,
        attr:'© <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors',
        next:'sat' },
  sat:{ src:(z,x,y)=>`https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/${z}/${y}/${x}`,
        attr:'Powered by <a href="https://www.esri.com/">Esri</a> — Source: Esri, Maxar, Earthstar Geographics',
        next:'osm' },
};
let basemap='osm';

// ── real OSM feature overlay — GET /api/osm/features/:z/:x/:y over the
// baked RowSlab (cockpit-server::osm_features). Off by default (opt-in,
// same posture as the Garmin drape's toggle): the slab is a large local dev
// artifact (OSM_SLAB_PATH), not always present, and fetching it unasked
// would be surprising. Raw OSM-XYZ z/x/y goes straight to the endpoint —
// this is deliberately the SAME z/wx/ty a raster tile <img> uses, never
// converted through the HHTL address the panel displays (see osm_features.rs
// module docs: the two key spaces are not interchangeable).
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
  }).then(d=>{ if(d){ featureCache.set(key,d); render(); } })
    .catch(()=>{ featureCache.delete(key); });  // transient failure: allow a retry on next render
}
function updateFeatStatus(visibleKeys){
  const el=document.getElementById('featStatus');
  if(!showFeatures){ el.textContent=''; return; }
  if(visibleKeys.some(k=>featureCache.get(k)==='unavailable')){
    el.textContent='no OSM slab baked (OSM_SLAB_PATH unset on the server)'; return;
  }
  if(visibleKeys.some(k=>featureCache.get(k)==='pending')){ el.textContent='loading features…'; return; }
  let returned=0, total=0;
  for(const k of visibleKeys){ const d=featureCache.get(k);
    if(d && typeof d==='object'){ returned+=d.returned; total+=d.total; } }
  el.textContent = total>returned ? `${returned} of ${total} features (tile cap hit)` : `${returned} features`;
}

function render(){
  const w=map.clientWidth, h=map.clientHeight, n=Math.pow(2,z);
  tilesEl.innerHTML='';
  // pixel offset of the map center
  const ox=w/2 - cx*256, oy=h/2 - cy*256;
  tilesEl.style.transform=`translate(${ox}px,${oy}px)`;
  const x0=Math.floor(cx - w/512)-1, x1=Math.floor(cx + w/512)+1;
  const y0=Math.floor(cy - h/512)-1, y1=Math.floor(cy + h/512)+1;
  const bm=BASEMAPS[basemap];
  const visibleKeys=[];
  for(let ty=y0;ty<=y1;ty++) for(let tx=x0;tx<=x1;tx++){
    const wx=((tx%n)+n)%n; if(ty<0||ty>=n) continue;
    const img=new Image();
    img.src=bm.src(z,wx,ty);
    img.style.left=(tx*256)+'px'; img.style.top=(ty*256)+'px';
    tilesEl.appendChild(img);

    if(showFeatures){
      const key=tileKey(z,wx,ty);
      visibleKeys.push(key);
      ensureFeatures(z,wx,ty);
      const d=featureCache.get(key);
      if(d && typeof d==='object'){
        // tx may sit outside [0,n) on a repeated world copy (low zoom /
        // pan past ±180°); lon2x() returns a coordinate wrapped into
        // [0,n), so re-add the same wrap offset the tile image itself
        // used to place it under the correct world copy.
        const offset=tx-wx;
        for(const f of d.features){
          const dot=document.createElement('div');
          dot.className='pt';
          dot.style.left=((lon2x(f.lon,z)+offset)*256)+'px';
          dot.style.top=(lat2y(f.lat,z)*256)+'px';
          tilesEl.appendChild(dot);
        }
      }
    }
  }
  updateFeatStatus(visibleKeys);
}

// ── panning ── (moved: a drag that actually panned, so the trailing click is
// suppressed — mouseup nulls `drag` before `click` fires, so `drag` alone can't
// gate it)
let drag=null, moved=false;
map.addEventListener('mousedown',e=>{ drag={x:e.clientX,y:e.clientY}; moved=false; map.classList.add('drag'); });
window.addEventListener('mouseup',()=>{ drag=null; map.classList.remove('drag'); });
window.addEventListener('mousemove',e=>{ if(!drag) return; moved=true;
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
map.addEventListener('click',async e=>{
  if(moved){ moved=false; return; }   // this "click" ended a pan — ignore it
  const r=map.getBoundingClientRect();
  const px=e.clientX-r.left, py=e.clientY-r.top;
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
    // tile source follows the ACTIVE basemap (the locate API reports both URLs) —
    // on satellite, showing the OSM URL would mismatch the visible tiles.
    document.getElementById('src').textContent=basemap==='sat'?d.sat_tile_url:d.tile_url;
  }catch(err){ document.getElementById('src').textContent='locate failed: '+err; }
});

addEventListener('resize',render);
render();
</script>
</body>
</html>
"##;
