# sculpt — module contract (pinned by the orchestrator; implement EXACTLY)

Every module owns its file completely. Cross-module calls go through these
signatures only. All fields listed are `pub`. Plain f32 math, `#![forbid(unsafe_code)]`
not required but no `unsafe` anyway. No deps beyond Cargo.toml (png, askama, helix).

## src/stl.rs
```rust
/// Raw triangle soup, exactly as an STL stores it (no connectivity).
pub struct TriSoup { pub tris: Vec<[[f32; 3]; 3]> }

/// Binary + ASCII STL, autodetected. Binary check FIRST and by arithmetic
/// (len >= 84 && 84 + 50*tri_count == len) — some binary files start with
/// the bytes "solid", so "starts_with(solid)" alone is WRONG.
pub fn read_stl(bytes: &[u8]) -> Result<TriSoup, String>;

/// Binary STL writer (80B header, u32 count, per tri: normal 12B + 3×12B verts
/// + u16 attr=0). Normal = unit face normal recomputed from the triangle.
/// This is the printer round-trip: what you sculpted is what you slice.
pub fn write_binary_stl(pos: &[[f32; 3]], tris: &[[u32; 3]]) -> Vec<u8>;

/// Built-in models (no upload needed to start playing):
pub fn icosphere(subdivisions: u32) -> TriSoup;        // unit radius, subdiv<=5 hard-capped
pub fn printer_cube(half: f32, chamfer: f32) -> TriSoup; // XYZ-calibration-cube-ish, chamfered edges
```
Unit tests: binary round-trip (write→read→same tri count & verts within 1e-6);
ASCII parse of a hand-written 2-triangle solid; the "solid"-prefixed *binary*
file decodes as binary; icosphere(2) is closed (every edge shared by exactly 2 tris).

## src/mesh.rs
```rust
/// Welded, indexed mesh. `col` is per-vertex RGB (sculpt paint lives here).
/// `adj` is vertex→vertex adjacency (from shared triangle edges, deduped).
pub struct Mesh {
    pub pos: Vec<[f32; 3]>,
    pub tris: Vec<[u32; 3]>,
    pub nrm: Vec<[f32; 3]>,   // per-vertex, area-weighted, unit
    pub col: Vec<[u8; 3]>,    // init 200,200,205
    pub adj: Vec<Vec<u32>>,
}

/// Weld the soup: quantize positions to a lattice of 1e-4 × bbox-diagonal,
/// coincident verts merge (i64x3 lattice key in a HashMap). Degenerate tris
/// (repeated welded index) are dropped. Then normals + adjacency are built.
pub fn weld(soup: &stl::TriSoup) -> Mesh;

impl Mesh {
    pub fn recompute_normals(&mut self);       // area-weighted vertex normals
    pub fn normalize_unit(&mut self);          // center on origin, max half-extent → 1.0
    pub fn vertex_count(&self) -> usize;
    pub fn tri_count(&self) -> usize;
}
```
Unit tests: welding an icosphere soup yields V-E+F=2 (Euler; count E via unique
edges); normals unit-length; normalize_unit puts bbox inside [-1,1].

## src/sculpt.rs
```rust
pub enum Tool { Grab, Inflate, Smooth, Spray, Ruler }
impl std::str::FromStr for Tool { /* "grab"|"inflate"|"smooth"|"spray"|"ruler" */ }

pub struct Stroke {
    pub tool: Tool,
    pub center: [f32; 3],   // picked surface point (world)
    pub dir: [f32; 3],      // world-space drag vector (Grab uses it; others ignore)
    pub radius: f32,        // world units (mesh is unit-normalized, so 0.05..0.5)
    pub strength: f32,      // 0..1
    pub color: [u8; 3],     // Spray
    pub detail: f32,        // Ruler lattice frequency (cells per unit), 4..64
}

/// Inverse record of exactly the vertices a stroke touched.
pub struct Undo { pub verts: Vec<(u32, [f32; 3], [u8; 3])> }

/// Apply one stroke. Falloff w = (1 - t^2)^2 with t = dist(center)/radius,
/// clamped; vertices with w <= 0 untouched. Compute the touched set FIRST,
/// record Undo, then write back (no incremental reads of half-written state).
///   Grab:    pos += dir * strength * w
///   Inflate: pos += nrm * 0.2 * strength * w
///   Smooth:  pos → lerp(pos, adjacency_mean, strength * w)
///   Spray:   col → lerp(col, color, strength * w)   (u8 rounded)
///   Ruler:   pos += nrm * 0.08 * strength * w * phase(pos, detail)
/// After any position-changing tool: recompute normals (call mesh method).
pub fn apply(mesh: &mut Mesh, s: &Stroke) -> Undo;
pub fn revert(mesh: &mut Mesh, u: &Undo);   // restore pos+col, recompute normals

/// THE KURVENLINEAL. Deterministic bipolar relief in [-1, 1] from the vertex's
/// lattice address — phase is regenerated from the address, NEVER stored:
///   cell  = floor(pos * detail) per axis (i64x3)  → place: u64 by mixing the
///           three cell ints with wrapping_mul of three large odd constants + xor
///   sub   = a finer 3x lattice inside the cell → k ∈ [0, 17) by the same mix mod 17
///   value = (CurveRuler::from_place(place).index(k) as f32 / 16.0) * 2.0 - 1.0
/// Same position+detail → same value on every stroke, every session: re-stroking
/// CONVERGES on the same relief instead of accumulating noise.
pub fn ruler_phase(pos: [f32; 3], detail: f32) -> f32;   // uses helix::CurveRuler
```
Unit tests: ruler_phase determinism (same input twice, byte-equal); ruler_phase
spans both signs over a lattice sweep; Grab moves only in-radius verts; Undo
round-trip restores byte-identical pos+col; Smooth shrinks a spike toward
neighbors.

## src/raster.rs
```rust
pub struct Camera { pub yaw: f32, pub pitch: f32, pub dist: f32, pub focal: f32 }
// eye orbits the ORIGIN (mesh is unit-normalized): standard orbit —
// eye = dist * [cos(pitch)sin(yaw), sin(pitch), cos(pitch)cos(yaw)], target 0,
// up +Y. Default { yaw: 0.6, pitch: 0.35, dist: 3.0, focal: 1.2*h }.

pub struct RenderOut {
    pub png: Vec<u8>,       // encoded RGB PNG
    pub depth: Vec<f32>,    // view-space z per pixel, f32::MAX = background
    pub w: u32, pub h: u32,
    pub eye: [f32; 3], pub right: [f32; 3], pub up: [f32; 3], pub fwd: [f32; 3],
    pub focal: f32,
}

/// Software raster: perspective project (x' = cx + focal*vx/vz, y' = cy - focal*vy/vz,
/// vz = view depth > 0 in front), z-buffered barycentric fill, Gouraud shading:
/// headlight (n·view) 0.75 + fill from upper-left 0.35 + ambient 0.12, times the
/// vertex color; simple depth-cue darkening far pixels 10%. Background #0c0f14
/// (the cockpit dark). Backface skip. w,h up to 1600 each.
pub fn render(mesh: &Mesh, cam: &Camera, w: u32, h: u32) -> RenderOut;

/// Unproject a screen pixel through the stored basis + its depth buffer value →
/// the world point on the surface. None on background. MUST be the exact inverse
/// of render's projection (see the round-trip test).
pub fn pick(ro: &RenderOut, x: f32, y: f32) -> Option<[f32; 3]>;

/// Screen drag (pixels) → world vector in the view plane, scaled so the vector
/// matches what the cursor covered AT THE PICKED DEPTH:
/// world = (right*dx - up*dy) * (vz / focal), vz = view depth of `at`.
pub fn drag_world(ro: &RenderOut, at: [f32; 3], dx: f32, dy: f32) -> [f32; 3];
```
Unit tests (the tiny details live here — get them right):
project→pick round-trip: render an icosphere, for 25 sampled non-background
pixels pick(x,y) must land within 2% of unit scale of a true surface point
(|len-1| small); drag_world of (focal,0) at depth vz has length ≈ vz; a known
front-facing pixel is lit brighter than a silhouette pixel.

## src/view.rs + templates/sculpt.html  (askama FieldView — read the pattern doc!)
Read /home/user/OGAR/docs/CLASSVIEW-FIELDVIEW-ASKAMA-BITMASK.md FIRST.
```rust
pub enum FieldKind { Range { min: f32, max: f32, step: f32 }, Color, ToolSelect }
pub struct FieldDesc { pub idx: u8, pub key: &'static str, pub label: &'static str, pub kind: FieldKind }

/// THE generated-once ordered field table (tool, radius, strength, color, detail).
pub const FIELDS: &[FieldDesc];

/// Per-tool render mask — the mask carves, the loop renders (ZERO ifs in the
/// template): Grab/Inflate/Smooth → tool|radius|strength; Spray → +color;
/// Ruler → +detail. fn tool_mask(t: &sculpt::Tool) -> u64;

pub struct BrushState { pub tool: sculpt::Tool, pub radius: f32, pub strength: f32,
                        pub color: [u8; 3], pub detail: f32 }

#[derive(askama::Template)]           // templates/sculpt.html
pub struct SculptPage<'a> {
    pub fields: &'a [FieldDesc], pub mask: u64, pub brush: &'a BrushState,
    pub verts: usize, pub tris: usize, pub model_name: &'a str,
}
impl SculptPage<'_> { pub fn selected(&self) -> impl Iterator<Item = &FieldDesc>; 
                      pub fn value_of(&self, f: &FieldDesc) -> String; }
```
Template: dark cockpit styling (#0c0f14 bg, system-ui, #8aaab8 accents), the
`<img id="v" src="/view.png">` canvas, ONE `{% for f in self.selected() %}` loop
rendering each field by kind (range input / color input / tool buttons), a
toolbar (undo · reset · download STL · model picker sphere/cube), stats line
"{{verts}} verts · {{tris}} tris · CurveRuler detail = deterministic phase".
Inline JS (~70 lines, plain): left-drag on img = stroke (pointerdown pick-start,
send POST /stroke?x&y&dx&dy on pointermove throttled ~30ms and pointerup);
right-drag or two-finger = orbit POST /camera?dyaw&dpitch; wheel = POST
/camera?ddist; after every POST set v.src='/view.png?t='+Date.now(); field
inputs POST /brush?key=value on change; buttons POST /undo, /reset?model=,
GET /model.stl (download link). Touch events mapped to the same handlers.

## src/bin/serve.rs  (std-only HTTP/1.1 — mirror fma/src/bin/serve.rs's style)
Routes (GET unless noted): `/` page (askama render) · `/view.png` current
render · POST `/stroke?x=&y=&dx=&dy=` (pick at x,y; Grab dir = drag_world;
others need only center; ignore background picks) · POST `/camera?dyaw=&dpitch=&ddist=`
(clamp pitch ±1.4, dist 1.2..8) · POST `/brush?tool=|radius=|strength=|color=RRGGBB|detail=`
· POST `/undo` · POST `/reset?model=sphere|cube` · `/model.stl`
(Content-Disposition attachment) · PUT or POST `/model.stl` with raw STL body
(Content-Length-bounded read, 32 MB cap) loads an uploaded model (weld +
normalize_unit). 404 else. State: `Mutex<App>` { mesh, cam, brush, undo:
Vec<Undo> (cap 64), last render cache invalidated on any mutation; render
lazily on /view.png }. Render 900×700. Bind 0.0.0.0:$PORT default 8090.
Parse just: request line, Content-Length header, query string (simple split —
no percent-decoding needed beyond '%23'→'#' for color, or just send color as
hex without '#').
```

## Iron details (all agents)
- Edit-only: do NOT run cargo (orchestrator compiles centrally, one target/).
- `use` paths: crate-internal `crate::stl::TriSoup` etc.; helix via `helix::CurveRuler`.
- Comments: explain invariants (projection inverse, weld epsilon, mask bits),
  never narration. Match fma's terse style.
- Tests in-module `#[cfg(test)]`. No test files under tests/ (workspace rule).
- The model identifier never appears in any artifact.
