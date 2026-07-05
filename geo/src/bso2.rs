//! BSO2 `/helix` wire encoder (feature `helix`; needs `ndarray` + `lance-graph-contract`).
//!
//! Emits the exact `BSO2` layout `body-soa-wire` produces, so `BodyHelix.tsx`
//! renders an OSM bake with zero viewer change. Per-vertex F16 position +
//! `helix_orient` dir/normal (6 B), per-concept `NodeGuid` keyed by the geo
//! classid + the HHTL tiles, + a `.blocks` sidecar for the `/api/body/lod`
//! depth-cascade cull. Buildings are extruded, then the whole scene is
//! normalized to `[-1,1]` (the viewer's convention) with a vertical
//! exaggeration so 6 m facades read against a multi-km footprint.

use lance_graph_contract::canonical_node::{NodeGuid, TailVariant};
use ndarray::hpc::quantized::F16;

use crate::extrude::Footprint;
use crate::hhtl;
use crate::osm_read::GeoScene;

/// Geo-V3 classid: geo domain (`0x0F`) osm concept HIGH, V3 marker (`0x1000`)
/// LOW — the post-2026-07-02 canon-HIGH layout, parallel to
/// `NodeGuid::CLASSID_FMA_V3` (`0x0A01_1000`). No named contract constant yet;
/// a proof-local mint (a real one would live in `lance-graph-contract`).
pub const CLASSID_GEO_V3: u32 = 0x0F01_1000;

/// Vertical exaggeration: after normalizing the (wide, flat) city to `[-1,1]` by
/// its horizontal extent, heights would be sub-pixel; scale them up so buildings
/// are visible massing rather than a flat plate.
const V_EXAGGERATION: f32 = 25.0;

/// Key zoom for the 4-tier spatial cascade (`heel|hip|twig|leaf`) — deep enough
/// (~cm tiles) that each building resolves to a near-unique address, so
/// `identity` only disambiguates the rare co-tile collision.
const KEY_ZOOM: u32 = 32;

/// Feature kind → the `family` (basin / codebook selector) tier of the facet.
fn kind_family(k: crate::extrude::Layer) -> u16 {
    match k {
        crate::extrude::Layer::Building => 1,
        crate::extrude::Layer::Road => 2,
        crate::extrude::Layer::Area => 3,
    }
}

/// Encode a **display-frame** unit normal into the 6 `Signed360` helix bytes —
/// the exact inverse of `BodyHelix.tsx`'s decode (NOT the stale `helix_orient`
/// codec, which BodyHelix reads as garbage). We take the `end = 255` analytic
/// branch (`rr = √(1−yp²)`, `yw = yp`), so the `rLut`/HXFL floor is bypassed and
/// the reconstruction is exact:
///   `N = (−rr·sin az, rr·cos az, yp)` with `yp = N.z`, `az = atan2(−N.x, N.y)`.
/// Layout: `pos_helix(0, 255, 0) | nrm_helix(polar, az_lo, az_hi)`.
fn signed360(n: [f32; 3]) -> [u8; 6] {
    let m = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt().max(1e-6);
    let (nx, ny, nz) = (n[0] / m, n[1] / m, (n[2] / m).clamp(-1.0, 1.0));
    // polar carries N.z (hemisphere sign + magnitude), matching the decode's yp.
    // nz ∈ [-1,1] ⇒ the `+`/`−` branches land in [128,255] / [0,127] by
    // construction, so the cast needs no clamp.
    let polar: u8 = if nz >= 0.0 {
        (128.0 + (nz * 127.0).round()) as u8
    } else {
        (127.0 - ((-nz) * 127.0).round()) as u8
    };
    let mut az = (-nx).atan2(ny);
    if az < 0.0 {
        az += std::f32::consts::TAU;
    }
    let az16 = ((az / std::f32::consts::TAU) * 65536.0).round() as u32 & 0xFFFF;
    [0, 255, 0, polar, (az16 & 0xFF) as u8, (az16 >> 8) as u8]
}

/// One feature's extruded geometry in the metric frame + its concept metadata.
struct Feat32 {
    v_start: u32,
    v_count: u32,
    /// 4-tier spatial address (`heel|hip|twig|leaf`).
    key: hhtl::Hhtl4,
    /// feature-kind basin tier.
    family: u16,
    /// instance within (tile+family) — a local counter, always fits u16.
    identity: u16,
    /// metric centroid (pre-normalization), for the block-bounds sidecar.
    centroid: [f32; 3],
}

/// Extrude one footprint into f32 pos/nrm/row columns (flat-shaded, per-face
/// normals, non-shared verts — the 5.5 M-vert full detail, unreduced).
fn extrude32(
    f: &Footprint,
    row: u32,
    pos: &mut Vec<[f32; 3]>,
    nrm: &mut Vec<[f32; 3]>,
    rows: &mut Vec<u32>,
    tris: &mut Vec<[u32; 3]>,
) {
    let n = f.ring.len();
    if n < 3 {
        return;
    }
    let top = f.height.max(0.0);
    // OSM footprints have no guaranteed winding. The shoelace signed area gives
    // the ring's orientation; multiplying the edge-perp normal by its sign makes
    // every wall face consistently outward regardless of CW/CCW input.
    let mut area2 = 0.0f32;
    for i in 0..n {
        let a = f.ring[i];
        let b = f.ring[(i + 1) % n];
        area2 += a[0] * b[1] - b[0] * a[1];
    }
    let wind = if area2 >= 0.0 { 1.0 } else { -1.0 };
    let mut push = |p: [f32; 3], nn: [f32; 3]| -> u32 {
        let i = pos.len() as u32;
        pos.push(p);
        nrm.push(nn);
        rows.push(row);
        i
    };
    // Walls.
    if top > 0.0 {
        for i in 0..n {
            let a = f.ring[i];
            let b = f.ring[(i + 1) % n];
            let (ex, ez) = (b[0] - a[0], b[1] - a[1]);
            let len = (ex * ex + ez * ez).sqrt().max(1e-6);
            let nn = [wind * ez / len, 0.0, -wind * ex / len];
            let v0 = push([a[0], 0.0, a[1]], nn);
            let v1 = push([b[0], 0.0, b[1]], nn);
            let v2 = push([b[0], top, b[1]], nn);
            let v3 = push([a[0], top, a[1]], nn);
            tris.push([v0, v1, v2]);
            tris.push([v0, v2, v3]);
        }
    }
    // Roof cap (fan from centroid).
    let (mut cx, mut cz) = (0.0f32, 0.0f32);
    for p in &f.ring {
        cx += p[0];
        cz += p[1];
    }
    let inv = 1.0 / n as f32;
    let up = [0.0, 1.0, 0.0];
    let cap = push([cx * inv, top, cz * inv], up);
    let rim: Vec<u32> = f.ring.iter().map(|p| push([p[0], top, p[1]], up)).collect();
    for i in 0..n {
        tris.push([cap, rim[i], rim[(i + 1) % n]]);
    }
}

/// Bake a read [`GeoScene`] into a gzip-ready `(BSO2 bytes, .blocks bytes)` pair
/// at the given HHTL `zoom`. Buildings only (roads/water are the next layer).
pub fn encode_bso2(scene: &GeoScene) -> (Vec<u8>, Vec<u8>) {
    // Force the V3 6×u16 facet: the geo classid isn't registered in the contract
    // yet, so `classid_read_mode(CLASSID_GEO_V3)` would default to V1 (whose
    // `new()` has no `leaf` tier — it would silently drop the 4th spatial tier
    // and misplace family/identity). A real geo classid would register V3 in
    // lance-graph-contract; here we mint it explicitly.
    let tail = TailVariant::V3;

    // ── Extrude every feature, tracking per-concept vertex ranges. ──
    let (mut pos, mut nrm, mut rows, mut tris) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut feats: Vec<Feat32> = Vec::with_capacity(scene.features.len());
    for (row, gf) in scene.features.iter().enumerate() {
        let v_start = pos.len() as u32;
        let fp = Footprint {
            ring: gf.ring.clone(),
            height: gf.height,
            layer: gf.kind,
            node_row: 0,
        };
        extrude32(&fp, row as u32, &mut pos, &mut nrm, &mut rows, &mut tris);
        let v_count = pos.len() as u32 - v_start;
        if v_count == 0 {
            continue;
        }
        // metric centroid over this feature's verts.
        let mut c = [0.0f32; 3];
        for v in &pos[v_start as usize..] {
            c[0] += v[0];
            c[1] += v[1];
            c[2] += v[2];
        }
        let inv = 1.0 / v_count as f32;
        feats.push(Feat32 {
            v_start,
            v_count,
            key: hhtl::point_to_hhtl4(gf.lon, gf.lat, KEY_ZOOM),
            family: kind_family(gf.kind),
            identity: 0, // assigned below by (tile+family) local counter
            centroid: [c[0] * inv, c[1] * inv, c[2] * inv],
        });
    }

    // ── Local instance counter: identity disambiguates features that land in
    //    the SAME cm-tile + kind (rare), keeping it a u16 the facet accepts. ──
    let mut seen: std::collections::HashMap<(u16, u16, u16, u16, u16), u16> =
        std::collections::HashMap::new();
    for f in feats.iter_mut() {
        let k = (f.key.heel, f.key.hip, f.key.twig, f.key.leaf, f.family);
        let slot = seen.entry(k).or_insert(0);
        f.identity = *slot;
        *slot = slot.wrapping_add(1);
    }
    let nv = pos.len();
    let nt = tris.len();
    let nc = feats.len();

    // ── Normalize to [-1,1] (horizontal by extent, vertical exaggerated). ──
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for p in &pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let cx = (lo[0] + hi[0]) * 0.5;
    let cz = (lo[2] + hi[2]) * 0.5;
    let half = ((hi[0] - lo[0]).max(hi[2] - lo[2]) * 0.5).max(1.0);
    let inv = 1.0 / half;
    let norm = |p: [f32; 3]| -> [f32; 3] {
        [
            (p[0] - cx) * inv,
            p[1] * inv * V_EXAGGERATION,
            (p[2] - cz) * inv,
        ]
    };
    for p in pos.iter_mut() {
        *p = norm(*p);
    }
    for f in feats.iter_mut() {
        f.centroid = norm(f.centroid);
    }

    // ── Assemble BSO2 ver 6 (F16 pos + Signed360 normals + HXFL trailer), the
    //    layout the CURRENT /helix bake uses (BodyHelix rejects the ver-5
    //    helix_orient codec as garbage). ──
    let mut o = Vec::with_capacity(nc * 40 + nv * 22 + nt * 12 + 64);
    o.extend_from_slice(b"BSO2");
    o.extend_from_slice(&6u16.to_le_bytes());
    o.extend_from_slice(&(nc as u32).to_le_bytes());
    o.extend_from_slice(&(nv as u32).to_le_bytes());
    o.extend_from_slice(&(nt as u32).to_le_bytes());
    // per-concept: guid | material | layer | label | centroid | (v_start,v_count)
    for f in &feats {
        // All 6×16-bit facet tiers filled: 4 spatial (heel|hip|twig|leaf),
        // family = kind basin, identity = per-tile instance.
        let key = NodeGuid::mint_for(
            tail,
            CLASSID_GEO_V3,
            f.key.heel,
            f.key.hip,
            f.key.twig,
            f.key.leaf,
            u32::from(f.family),
            u32::from(f.identity),
        );
        o.extend_from_slice(key.as_bytes());
    }
    o.extend(feats.iter().map(|_| 4u8)); // material (doppler default)
    o.extend(feats.iter().map(|_| 4u8)); // layer (toggle/colour byte)
    for _ in &feats {
        o.extend_from_slice(&0u32.to_le_bytes()); // label idx → names[0] = "building"
    }
    for f in &feats {
        // srcPos frame (-Dx, Dz, Dy): BodyHelix applies the same (-x, z, y) remap
        // to this centroid column as to the vertex positions, so store it
        // pre-remap or focus targets land Y/Z-swapped from the mesh.
        for c in [-f.centroid[0], f.centroid[2], f.centroid[1]] {
            o.extend_from_slice(&c.to_le_bytes());
        }
    }
    for f in &feats {
        o.extend_from_slice(&f.v_start.to_le_bytes());
        o.extend_from_slice(&f.v_count.to_le_bytes());
    }
    // per-vertex: pos 3×F16 | helix 6B (Signed360) | row u32.
    // `pos` holds display coords (Dx, Dy=up, Dz); BodyHelix remaps srcPos as
    // (-sx, sz, sy), so we write srcPos = (-Dx, Dz, Dy) to land upright.
    for p in &pos {
        for &c in &[-p[0], p[2], p[1]] {
            o.extend_from_slice(&F16::from_f32(c).0.to_le_bytes());
        }
    }
    for n in &nrm {
        o.extend_from_slice(&signed360(*n)); // display-frame normal → Signed360
    }
    for r in &rows {
        o.extend_from_slice(&r.to_le_bytes());
    }
    // tris
    for t in &tris {
        for idx in t {
            o.extend_from_slice(&idx.to_le_bytes());
        }
    }
    // labels_json + materials_json (BodyHelix reads names from labels_json).
    let labels = br#"{"names":["building"]}"#;
    o.extend_from_slice(&(labels.len() as u32).to_le_bytes());
    o.extend_from_slice(labels);
    let materials = b"{}";
    o.extend_from_slice(&(materials.len() as u32).to_le_bytes());
    o.extend_from_slice(materials);
    // HXFL trailer (12 B): the rim-dequant floor. We take the analytic branch
    // (`end = 255`), so these values are unused, but a well-formed ver-6 artifact
    // carries the tag; BodyHelix's own fallback matches these constants.
    o.extend_from_slice(b"HXFL");
    o.extend_from_slice(&(-2.256_794_5f32).to_le_bytes());
    o.extend_from_slice(&11.535_854f32.to_le_bytes());

    // ── .blocks sidecar: per-concept (center3, radius) in DISPLAY space (the
    //    frame the client posts its camera in), for the /api/body/lod cascade.
    //    `f.centroid` is already display coords (Dx, Dy, Dz). ──
    let mut blocks = Vec::with_capacity(nc * 16);
    for f in &feats {
        let mut rad = 0.0f32;
        for v in &pos[f.v_start as usize..(f.v_start + f.v_count) as usize] {
            let d = (v[0] - f.centroid[0]).powi(2)
                + (v[1] - f.centroid[1]).powi(2)
                + (v[2] - f.centroid[2]).powi(2);
            rad = rad.max(d);
        }
        for v in f.centroid {
            blocks.extend_from_slice(&v.to_le_bytes());
        }
        blocks.extend_from_slice(&rad.sqrt().max(1e-4).to_le_bytes());
    }

    (o, blocks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osm_read::GeoFeature;
    use crate::extrude::Layer;

    /// Reproduce BodyHelix.tsx's Signed360 decode (end=255 analytic branch) so
    /// we prove our encoder is its exact inverse — the one check that actually
    /// gates whether /helix shades correctly.
    fn decode_signed360(b: [u8; 6]) -> [f32; 3] {
        let (end, polar) = (b[1], b[3]);
        let az16 = u32::from(b[4]) | (u32::from(b[5]) << 8);
        let yp = if polar >= 128 {
            (f32::from(polar) - 128.0) / 127.0
        } else {
            -(127.0 - f32::from(polar)) / 127.0
        };
        let rr = if end == 255 {
            (1.0 - yp * yp).max(0.0).sqrt()
        } else {
            0.0
        };
        let sgn = if polar >= 128 { 1.0 } else { -1.0 };
        let yw = sgn * (1.0 - rr * rr).max(0.0).sqrt();
        let az = (az16 as f32 / 65536.0) * std::f32::consts::TAU;
        [-rr * az.sin(), rr * az.cos(), yw]
    }

    #[test]
    fn signed360_is_the_exact_inverse_of_bodyhelix_decode() {
        // 3-4-5 style unit vectors (exact, no irrational-constant approximations).
        for n in [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.6, 0.0, 0.8],
            [0.0, -0.6, 0.8],
            [-0.8, 0.0, 0.6],
        ] {
            let got = decode_signed360(signed360(n));
            let err = (0..3).map(|k| (got[k] - n[k]).powi(2)).sum::<f32>().sqrt();
            assert!(err < 0.02, "signed360 round-trip {n:?} -> {got:?} err {err}");
        }
    }

    #[test]
    fn one_building_bakes_to_valid_bso2() {
        let scene = GeoScene {
            features: vec![GeoFeature {
                ring: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
                height: 12.0,
                kind: Layer::Building,
                lon: 8.807,
                lat: 53.075,
            }],
            lon0: 8.807,
            lat0: 53.075,
        };
        let (bso2, blocks) = encode_bso2(&scene);
        assert_eq!(&bso2[0..4], b"BSO2");
        let ver = u16::from_le_bytes(bso2[4..6].try_into().unwrap());
        let nc = u32::from_le_bytes(bso2[6..10].try_into().unwrap());
        let nv = u32::from_le_bytes(bso2[10..14].try_into().unwrap());
        let nt = u32::from_le_bytes(bso2[14..18].try_into().unwrap());
        assert_eq!(ver, 6);
        assert_eq!(nc, 1);
        assert_eq!(nv, 21); // 4-wall × 4 + cap(1+4)
        assert_eq!(nt, 12);
        assert_eq!(blocks.len(), 16); // one concept × 16 B
    }
}
