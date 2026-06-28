//! `/api/body/lod` — server-side HHTL depth-cascade LOD for the `/body` viewer.
//!
//! Option-2 (compute server-side): the wasm `F32x16` backend is an un-wired stub,
//! so the per-frame LOD cascade runs here on native x86 (full SIMD). The viewer has
//! the full geometry already; this endpoint returns, per concept, an `HhtlAction`
//! byte (Reject / KeepCoarse / Refine / ProjectExact / RenderExact) for the posted
//! camera. The client gates each concept's draw by that byte (Reject ⇒ cull) — so
//! when zoomed in, off-frustum concepts stop drawing. O(concepts ≈ 1658) — the HHTL
//! O(1) reference, NOT O(verts).
//!
//! The cascade core is verified standalone in `scratch-fma/bodylod` (cockpit-server
//! can't build in-sandbox: its quarto-core→runtimelib git dep is proxy-blocked).
//! The per-concept `BlockBounds` (centroid + radius) are baked by `soabake` into the
//! 26 KB `assets/body.blocks` asset embedded below — NOT the 57 MB geometry.
use std::sync::LazyLock;

use axum::Json;
use ndarray::hpc::splat3d::depth_cascade::{cascade_blocks, BlockBounds, DepthCascadeBudget};
use ndarray::hpc::splat3d::project::Camera;
use serde::{Deserialize, Serialize};

/// `nc × (center[3] f32 | radius f32)`, little-endian — baked by `soabake`.
static BODY_BLOCKS: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/body.blocks"));

static BLOCKS: LazyLock<Vec<BlockBounds>> = LazyLock::new(|| {
    BODY_BLOCKS
        .chunks_exact(16)
        .map(|c| {
            let f = |k: usize| f32::from_le_bytes(c[k * 4..k * 4 + 4].try_into().unwrap());
            BlockBounds { center: [f(0), f(1), f(2)], radius: f(3).max(1e-4) }
        })
        .collect()
});

/// Camera posted by the viewer (mirrors `ndarray::…::project::Camera`).
#[derive(Debug, Deserialize)]
pub struct LodCamera {
    pub view: [[f32; 4]; 4],
    pub fx: f32,
    pub fy: f32,
    pub cx: f32,
    pub cy: f32,
    pub near: f32,
    pub far: f32,
    pub width: u32,
    pub height: u32,
    pub position: [f32; 3],
}

#[derive(Debug, Serialize)]
pub struct LodResponse {
    /// One `HhtlAction` byte per concept, in concept (row) order.
    pub actions: Vec<u8>,
    pub n_concepts: usize,
    /// Tally [Reject, KeepCoarse, Refine, ProjectExact, RenderExact].
    pub tally: [usize; 5],
}

/// POST `/api/body/lod` { camera } → per-concept LOD action bytes.
pub async fn body_lod_handler(Json(cam): Json<LodCamera>) -> Json<LodResponse> {
    let camera = Camera {
        view: cam.view,
        fx: cam.fx,
        fy: cam.fy,
        cx: cam.cx,
        cy: cam.cy,
        near: cam.near,
        far: cam.far,
        width: cam.width.max(1),
        height: cam.height.max(1),
        position: cam.position,
    };
    let budget = DepthCascadeBudget::default();
    let mut decisions = Vec::with_capacity(BLOCKS.len());
    cascade_blocks(&camera, &BLOCKS, &budget, &mut decisions);

    let mut actions = vec![0u8; BLOCKS.len()];
    let mut tally = [0usize; 5];
    for d in &decisions {
        let a = d.action as usize;
        if d.block_index < actions.len() {
            actions[d.block_index] = a as u8;
        }
        if a < 5 {
            tally[a] += 1;
        }
    }
    Json(LodResponse { actions, n_concepts: BLOCKS.len(), tally })
}
