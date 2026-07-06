# sculpt — module contract

Each module owns its file completely; cross-module calls go through the
signatures below. Plain f32 math, no `unsafe`, deps limited to png + askama +
helix. This is the spec the modules were built against.

## src/stl.rs
- `struct TriSoup { tris: Vec<[[f32;3];3]> }` — raw triangle soup.
- `read_stl(&[u8]) -> Result<TriSoup, String>` — binary detection is by
  arithmetic (`len == 84 + 50*count`) FIRST, because real binary STLs can begin
  with the bytes `"solid"`; ASCII fallback.
- `write_binary_stl(pos, tris) -> Vec<u8>` — the printer round-trip; face normals
  recomputed, never trusted.
- `icosphere(subdiv) -> TriSoup`, `printer_cube(half, chamfer) -> TriSoup` — closed
  manifold built-ins (Euler V−E+F=2 tested).

## src/mesh.rs
- `struct Mesh { pos, tris, nrm, col, adj }` — welded, indexed, per-vertex color.
- `weld(&TriSoup) -> Mesh` — lattice weld (1e-4 × bbox diagonal), area-weighted
  unit normals, deduped symmetric adjacency, degenerate tris dropped.
- `Mesh::{recompute_normals, normalize_unit, vertex_count, tri_count}`.

## src/sculpt.rs
- `enum Tool { Grab, Inflate, Smooth, Spray, Ruler }` (+ `FromStr`).
- `struct Stroke { tool, center, dir, radius, strength, color, detail }`.
- `struct Undo { verts: Vec<(u32,[f32;3],[u8;3])> }`.
- `apply(&mut Mesh, &Stroke) -> Undo` / `revert(&mut Mesh, &Undo)` — falloff
  `(1−t²)²`; touched set + Undo captured from the UNMODIFIED mesh before write-back.
- `ruler_phase(pos, detail) -> f32` — THE kurvenlineal: deterministic bipolar
  relief in [−1,1] from the vertex lattice address via `helix::CurveRuler`
  (stride-4-over-17). Phase regenerated from address, never stored → re-stroking
  converges instead of accumulating.

## src/raster.rs
- `struct Camera { yaw, pitch, dist, focal }` — eye orbits the origin, +Y up.
- `struct RenderOut { png, depth, w, h, eye, right, up, fwd, focal }`.
- `render(&Mesh, &Camera, w, h) -> RenderOut` — pinhole project, z-buffer
  barycentric fill, Gouraud shading (headlight + upper-left fill + ambient),
  depth cue, background #0c0f14.
- `pick(&RenderOut, x, y) -> Option<[f32;3]>` — EXACT inverse of the projection,
  via the stored basis + depth buffer.
- `drag_world(&RenderOut, at, dx, dy) -> [f32;3]` — screen drag → world view-plane
  vector at `at`'s depth (Grab tracks the cursor 1:1).

## src/view.rs + templates/sculpt.html
The OGAR FieldView pattern: `const FIELDS: &[FieldDesc]`, `tool_mask(&Tool) -> u64`
(Spray adds color, Ruler adds detail), and `SculptPage` (`#[derive(askama::Template)]`)
whose `selected()` iterator drives ONE template loop — the mask carves, the loop
renders, zero per-field conditionals (widget HTML per kind computed in Rust).

## src/bin/serve.rs
Dep-free std HTTP/1.1 (the fma server pattern). `Mutex<App>` holding mesh + camera
+ brush + undo stack + a cached `RenderOut`. Routes: `/`, `/view.png`,
`/stroke?x&y&dx&dy`, `/camera?dyaw&dpitch&ddist`, `/brush?…`, `/undo`,
`/reset?model=`, `GET|POST /model.stl` (download / upload ≤ 32 MB). Port `$PORT`
(default 8090).
