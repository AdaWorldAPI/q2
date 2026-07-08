//! The drape stage: lift typed **vector line features** (roads / trails / rivers)
//! onto the terrain height surface as polylines, co-registered with the ver-8
//! radix grid. This is the "OSM ⊕ Garmin" fusion's Garmin half — the Garmin IMG
//! already carries typed `Street` / `Path` / `Stream` polylines, so the semantic
//! network is *decoded data*, not colour-guessing.
//!
//! Co-registration is exact **by construction**: a feature vertex `(lon, lat)` is
//! projected to fractional grid `(col, row)` with the SAME [`super::terrain::project`]
//! the heightfield uses, then the display-frame surface point is **bilinear-sampled
//! from the ver-8 bake's own `pos` grid**. The ver-8 client reconstructs its
//! vertices as `(x0 + c·dx, height·yscale, zrow[r])`; `pos[i]` *is* that point
//! pre-encode — so a drape point sampled from `pos` lands on the decoded surface,
//! and the renderer lifts it by the identical `uExag` the terrain shader applies.
//!
//! Landcover (Water / Woods / Park areas) is the terrain *material* — drawn by the
//! grid's `kind` overlay — so the drape is only the LINE network that reads as
//! roads and rivers over the relief.

use super::terrain::project;
use super::tre::Bbox;
use super::{Decoded, GeoKind, Kind};

/// The semantic line classes lifted onto the surface: road, trail, river.
pub const DRAPE_KINDS: [GeoKind; 3] = [GeoKind::Street, GeoKind::Path, GeoKind::Stream];

/// A draped polyline: its semantic [`GeoKind`] tag and the lifted display-frame
/// points, already on the terrain surface (pre-exaggeration — the renderer scales
/// `y` by `uExag`, matching the grid).
#[derive(Debug, Clone)]
pub struct DrapeLine {
    pub kind: u8,
    pub pts: Vec<[f32; 3]>,
}

/// Bilinear-sample the display-frame surface point at fractional grid `(col, row)`
/// from the ver-8 `pos` grid (row-major `w×h`). Clamps to the grid edge.
fn sample_pos(pos: &[[f32; 3]], w: usize, h: usize, col: f32, row: f32) -> [f32; 3] {
    let c0 = (col.floor() as isize).clamp(0, w as isize - 1) as usize;
    let r0 = (row.floor() as isize).clamp(0, h as isize - 1) as usize;
    let c1 = (c0 + 1).min(w - 1);
    let r1 = (r0 + 1).min(h - 1);
    let fc = (col - c0 as f32).clamp(0.0, 1.0);
    let fr = (row - r0 as f32).clamp(0.0, 1.0);
    let p00 = pos[r0 * w + c0];
    let p10 = pos[r0 * w + c1];
    let p01 = pos[r1 * w + c0];
    let p11 = pos[r1 * w + c1];
    let mut out = [0.0f32; 3];
    for k in 0..3 {
        let top = p00[k] * (1.0 - fc) + p10[k] * fc;
        let bot = p01[k] * (1.0 - fc) + p11[k] * fc;
        out[k] = top * (1.0 - fr) + bot * fr;
    }
    out
}

/// Lift every line feature of the wanted `kinds` at LOD `level` onto the terrain
/// surface. Each point is the display-frame surface point sampled from `pos`, so
/// the polylines are co-registered with the ver-8 grid (same `level` as the
/// heightfield keeps the drape at the terrain's own generalization).
#[must_use]
pub fn build_drape(
    dec: &Decoded,
    bbox: Bbox,
    pos: &[[f32; 3]],
    w: usize,
    h: usize,
    level: u8,
    kinds: &[GeoKind],
) -> Vec<DrapeLine> {
    debug_assert_eq!(pos.len(), w * h, "pos must be a w×h grid");
    let mut lines = Vec::new();
    for f in &dec.features {
        if f.kind != Kind::Line || f.level != level || f.coords.len() < 2 {
            continue;
        }
        let k = f.geo_kind();
        if !kinds.contains(&k) {
            continue;
        }
        let pts: Vec<[f32; 3]> = f
            .coords
            .iter()
            .map(|&(lon, lat)| {
                let (col, row) = project(bbox, lon, lat, w, h);
                sample_pos(pos, w, h, col, row)
            })
            .collect();
        lines.push(DrapeLine { kind: k.tag(), pts });
    }
    lines
}

/// Fixed-point scale for DRP1 points: display coords live in `[-1, 1]` (x/z) and
/// `[0, 1]` (y, pre-exag), so `coord · 16384` fits `i16` with precision `6e-5` —
/// far finer than the ~`2/844` grid cell, i.e. sub-cell exact for the canyon.
pub const DRAPE_SCALE: f32 = 16384.0;

/// Encode draped polylines into the **DRP1** wire — a tiny, self-describing line
/// sidecar the ver-8 terrain wire is unaware of (kept separate so the proven grid
/// decoder is untouched):
///
/// ```text
/// b"DRP1" | ver=1 u16 | nLines u32 | nKind u8 | palette(nKind × rgb u8) | scale f32 LE
///         | per line: kind u8 | nPts u16 | pts(nPts × 3 i16 LE, coord = i16 / scale)
/// ```
///
/// `palette` is [`GeoKind::PALETTE`] so the drape colours match the terrain's KIND
/// palette; `kind` indexes it. Points are display-frame surface points (pre-exag),
/// quantized `i16` at [`DRAPE_SCALE`] — half the f32 payload, sub-cell exact.
#[must_use]
pub fn encode_drape(lines: &[DrapeLine], palette: &[[u8; 3]]) -> Vec<u8> {
    assert!(palette.len() <= 255, "palette must fit a u8 count");
    let pts_total: usize = lines.iter().map(|l| l.pts.len()).sum();
    let mut o = Vec::with_capacity(15 + palette.len() * 3 + lines.len() * 3 + pts_total * 6);
    o.extend_from_slice(b"DRP1");
    o.extend_from_slice(&1u16.to_le_bytes());
    o.extend_from_slice(&(lines.len() as u32).to_le_bytes());
    o.push(palette.len() as u8);
    for p in palette {
        o.extend_from_slice(p);
    }
    o.extend_from_slice(&DRAPE_SCALE.to_le_bytes());
    let q = |v: f32| -> i16 { (v * DRAPE_SCALE).round().clamp(-32768.0, 32767.0) as i16 };
    for l in lines {
        o.push(l.kind);
        let n = l.pts.len().min(u16::MAX as usize);
        o.extend_from_slice(&(n as u16).to_le_bytes());
        for p in l.pts.iter().take(n) {
            for &c in p {
                o.extend_from_slice(&q(c).to_le_bytes());
            }
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::garmin::terrain::heightfield_for_level;

    fn village() -> (Decoded, Vec<u8>) {
        let path = format!(
            "{}/../.claude/maps/garmin-grand-canyon/47505316.img",
            env!("CARGO_MANIFEST_DIR")
        );
        let img = crate::garmin::Img::read(&path).expect("read tile");
        let lbl = img.lbl().expect("lbl").to_vec();
        (img.decode().expect("decode"), lbl)
    }

    /// Build a trivial display-frame `pos` grid from a heightfield: x,z in [-1,1],
    /// y = the heightfield elevation normalized. Mirrors the bake's frame shape
    /// closely enough to prove sampling + co-registration mechanically.
    fn pos_grid(hf: &super::super::HeightField) -> Vec<[f32; 3]> {
        let (w, h) = (hf.w, hf.h);
        let (lo, hi) = hf.range();
        let span = (hi - lo).max(1.0);
        let mut pos = vec![[0.0f32; 3]; w * h];
        for r in 0..h {
            for c in 0..w {
                let x = c as f32 / (w - 1).max(1) as f32 * 2.0 - 1.0;
                let z = r as f32 / (h - 1).max(1) as f32 * 2.0 - 1.0;
                let y = (hf.at(c, r) - lo) / span; // [0,1]
                pos[r * w + c] = [x, y, z];
            }
        }
        pos
    }

    #[test]
    fn canyon_level4_lifts_roads_and_rivers_onto_the_surface() {
        let (dec, lbl_bytes) = village();
        let lbl = crate::garmin::lbl::parse(&lbl_bytes);
        let (w, h) = (128usize, 128usize);
        let hf = heightfield_for_level(&dec, &lbl, 4, w, h, true);
        let pos = pos_grid(&hf);

        let lines = build_drape(&dec, dec.tre.bbox, &pos, w, h, 4, &DRAPE_KINDS);
        assert!(
            lines.len() > 100,
            "level-4 drape should carry a real network: {} lines",
            lines.len()
        );

        // The three wanted classes are all present at the detail level.
        let has = |g: GeoKind| lines.iter().any(|l| l.kind == g.tag());
        assert!(has(GeoKind::Stream), "rivers lifted");
        assert!(has(GeoKind::Street), "roads lifted");

        // Only the wanted line classes — never contours/areas — are draped.
        for l in &lines {
            assert!(
                DRAPE_KINDS.iter().any(|k| k.tag() == l.kind),
                "unexpected drape kind {}",
                l.kind
            );
            assert!(l.pts.len() >= 2, "a polyline has ≥2 points");
        }

        // Every lifted point is finite and inside the display grid's convex hull
        // ([-1,1]³ here) — i.e. it sits ON the sampled surface, never floating.
        let (mut ylo, mut yhi) = (f32::INFINITY, f32::NEG_INFINITY);
        for l in &lines {
            for p in &l.pts {
                assert!(p.iter().all(|v| v.is_finite()), "finite drape point");
                assert!(p[0] >= -1.001 && p[0] <= 1.001, "x on grid: {}", p[0]);
                assert!(p[2] >= -1.001 && p[2] <= 1.001, "z on grid: {}", p[2]);
                assert!(
                    p[1] >= -0.001 && p[1] <= 1.001,
                    "y in surface range: {}",
                    p[1]
                );
                ylo = ylo.min(p[1]);
                yhi = yhi.max(p[1]);
            }
        }
        // Rivers run the canyon floor, roads climb — the drape spans real relief.
        assert!(
            yhi - ylo > 0.2,
            "drape follows the relief: y span {}",
            yhi - ylo
        );
    }

    #[test]
    fn drp1_header_round_trips() {
        let lines = vec![
            DrapeLine {
                kind: GeoKind::Stream.tag(),
                pts: vec![[0.0, 0.1, 0.0], [0.5, 0.2, 0.5], [1.0, 0.3, 1.0]],
            },
            DrapeLine {
                kind: GeoKind::Street.tag(),
                pts: vec![[-1.0, 0.0, -1.0], [-0.5, 0.05, -0.5]],
            },
        ];
        let palette: Vec<[u8; 3]> = GeoKind::PALETTE.iter().map(|k| k.color()).collect();
        let buf = encode_drape(&lines, &palette);

        assert_eq!(&buf[0..4], b"DRP1");
        assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 1);
        assert_eq!(u32::from_le_bytes([buf[6], buf[7], buf[8], buf[9]]), 2);
        assert_eq!(buf[10] as usize, GeoKind::PALETTE.len());
        let scale_off = 11 + GeoKind::PALETTE.len() * 3;
        assert_eq!(
            f32::from_le_bytes(buf[scale_off..scale_off + 4].try_into().unwrap()),
            DRAPE_SCALE
        );
        // header(11) + palette(9×3) + scale(4) + line0(1+2 + 3×6=18) + line1(1+2 + 2×6=12)
        let expect = 11 + GeoKind::PALETTE.len() * 3 + 4 + (3 + 18) + (3 + 12);
        assert_eq!(buf.len(), expect, "byte-exact DRP1 size");
    }
}
