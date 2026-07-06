//! # sculpt — STL under the hand, server-rendered
//!
//! Load a 3D-printer STL (or a built-in model), sculpt it with ZBrush-shaped
//! brushes — **grab** (view-plane pull), **inflate**, **smooth**, **spray**
//! (vertex color), and the **ruler** brush: surface relief whose magnitude is
//! the deterministic stride-4-over-17 phase of [`helix::CurveRuler`], keyed by
//! the vertex's lattice address. Same address → same relief, forever; the
//! detail is *uncovered*, not stored.
//!
//! No WebGL: [`raster`] is a software z-buffer rasterizer emitting PNG; the
//! browser only shows an `<img>` and posts pointer strokes. The brush panel is
//! an askama **FieldView** ([`view`]): a mask-filtered field list rendered by
//! one dumb template loop — the per-tool mask carves, the loop renders.
//!
//! Module contract (each module owns its file; see `CONTRACT.md`):
//! - [`stl`] — binary/ASCII STL read, binary write, built-in demo models.
//! - [`mesh`] — welded indexed mesh (positions/tris/normals/colors/adjacency).
//! - [`sculpt`] — the brush ops; every stroke returns an [`sculpt::Undo`].
//! - [`raster`] — render + depth-buffer pick + screen-drag→world mapping.
//! - [`view`] — the FieldView brush panel + page template state.

// Axis loops over `[f32; 3]` (`for a in 0..3 { p[a] = (p[a]-c[a])*s }`) read
// clearer as index loops than as a zip of three arrays; this is exactly the
// case `needless_range_loop` over-fires on.
#![allow(clippy::needless_range_loop)]

pub mod mesh;
pub mod raster;
pub mod sculpt;
pub mod stl;
pub mod view;
