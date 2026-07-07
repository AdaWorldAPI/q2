//! The terrain stage: labeled contour polylines → a connected grid heightfield.
//!
//! Method **B** (grid, not a point TIN, not Gaussian splat): rasterize each
//! contour polyline onto the `W×H` grid at its elevation, then fill the gaps
//! between them by **harmonic relaxation with the contour cells held fixed**
//! (Dirichlet boundary conditions). The contours stay razor-sharp — only the
//! interior fills — so the surface is one connected sheet (grid neighbours =
//! structural connectivity), never a field of isolated spikes. This is *not*
//! Gaussian smoothing: a Gaussian blur would smear the contours; here they are
//! immovable and the relaxation only interpolates the unconstrained cells.
//!
//! LOD is the TRE level pyramid, for free: each level carries a generalized
//! contour set (this canyon tile: level 2 = 3690, level 3 = 8068, level 4 =
//! 50816), so [`heightfield_for_level`] bakes one surface per level and the
//! renderer swaps tier by camera distance.
//!
//! The sub-contour relief (the `kurvenlineal` residue) and the palette are
//! applied downstream by the bake; this module produces the base geometry.

use super::tre::Bbox;
use super::{Decoded, Feature, GeoKind, Kind};

/// A row-major heightfield over a lon/lat bbox. `z[r*w + c]` is the surface
/// elevation in metres; row 0 is the north edge (equirectangular, matching the
/// OSM / Iceland bakes).
#[derive(Debug, Clone)]
pub struct HeightField {
    pub w: usize,
    pub h: usize,
    pub z: Vec<f32>,
}

impl HeightField {
    #[must_use]
    pub fn at(&self, col: usize, row: usize) -> f32 {
        self.z[row * self.w + col]
    }

    /// `(min, max)` elevation over the field.
    #[must_use]
    pub fn range(&self) -> (f32, f32) {
        let mut lo = f32::INFINITY;
        let mut hi = f32::NEG_INFINITY;
        for &v in &self.z {
            lo = lo.min(v);
            hi = hi.max(v);
        }
        (lo, hi)
    }
}

/// Project a `(lon, lat)` mapunit onto fractional grid `(col, row)` over `bbox`.
/// Row 0 = north. Degenerate spans collapse to 0.
pub fn project(bbox: Bbox, lon: i64, lat: i64, w: usize, h: usize) -> (f32, f32) {
    let ew = f64::from(bbox.east) - f64::from(bbox.west);
    let ns = f64::from(bbox.north) - f64::from(bbox.south);
    let fx = if ew != 0.0 {
        (lon as f64 - f64::from(bbox.west)) / ew
    } else {
        0.0
    };
    let fy = if ns != 0.0 {
        (f64::from(bbox.north) - lat as f64) / ns
    } else {
        0.0
    };
    ((fx * (w - 1) as f64) as f32, (fy * (h - 1) as f64) as f32)
}

/// Stamp a straight segment `(c0,r0)→(c1,r1)` into the grid at `elev` (DDA walk,
/// one sample per max-axis step), marking each touched cell `known`.
#[allow(clippy::too_many_arguments)]
fn stamp_segment(
    z: &mut [f32],
    known: &mut [bool],
    w: usize,
    h: usize,
    p0: (f32, f32),
    p1: (f32, f32),
    elev: f32,
) {
    let steps = ((p1.0 - p0.0).abs().max((p1.1 - p0.1).abs())).ceil() as i64;
    let n = steps.max(1);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let c = (p0.0 + (p1.0 - p0.0) * t).round() as i64;
        let r = (p0.1 + (p1.1 - p0.1) * t).round() as i64;
        if c >= 0 && r >= 0 && (c as usize) < w && (r as usize) < h {
            let idx = r as usize * w + c as usize;
            z[idx] = elev;
            known[idx] = true;
        }
    }
}

/// Fill the unknown cells by successive over-relaxation (SOR) of Laplace's
/// equation, holding `known` (contour) cells fixed. Converges when the largest
/// update drops below `tol`, or after `max_iters`.
fn relax(z: &mut [f32], known: &[bool], w: usize, h: usize, max_iters: usize, tol: f32) {
    const OMEGA: f32 = 1.8; // over-relaxation factor (1 < ω < 2)
    for _ in 0..max_iters {
        let mut max_delta = 0.0f32;
        for r in 0..h {
            for c in 0..w {
                let idx = r * w + c;
                if known[idx] {
                    continue;
                }
                // Average of the 4-neighbourhood, clamping at the grid edge.
                let left = z[if c > 0 { idx - 1 } else { idx }];
                let right = z[if c + 1 < w { idx + 1 } else { idx }];
                let up = z[if r > 0 { idx - w } else { idx }];
                let down = z[if r + 1 < h { idx + w } else { idx }];
                let avg = 0.25 * (left + right + up + down);
                let new = z[idx] + OMEGA * (avg - z[idx]);
                max_delta = max_delta.max((new - z[idx]).abs());
                z[idx] = new;
            }
        }
        if max_delta < tol {
            break;
        }
    }
}

/// Build a heightfield from contour polylines. Each entry is `(polyline in
/// mapunits, elevation in metres)`. Unconstrained cells are harmonically
/// interpolated between the contours.
#[must_use]
pub fn grid_from_contours(
    contours: &[(&[(i64, i64)], f32)],
    bbox: Bbox,
    w: usize,
    h: usize,
) -> HeightField {
    let mut z = vec![0.0f32; w * h];
    let mut known = vec![false; w * h];

    // Seed the unknown cells at the mean contour elevation so relaxation starts
    // from a sensible level (fewer iterations, no dependence on the 0.0 default).
    let mean = if contours.is_empty() {
        0.0
    } else {
        contours.iter().map(|(_, e)| *e).sum::<f32>() / contours.len() as f32
    };
    z.fill(mean);

    for (line, elev) in contours {
        if line.len() < 2 {
            if let Some(&(lon, lat)) = line.first() {
                let (c, r) = project(bbox, lon, lat, w, h);
                stamp_segment(&mut z, &mut known, w, h, (c, r), (c, r), *elev);
            }
            continue;
        }
        for pair in line.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let p0 = project(bbox, a.0, a.1, w, h);
            let p1 = project(bbox, b.0, b.1, w, h);
            stamp_segment(&mut z, &mut known, w, h, p0, p1, *elev);
        }
    }

    // Relaxation tolerance: ~1 cm of the elevation span is plenty for terrain.
    relax(&mut z, &known, w, h, 400, 0.01);
    HeightField { w, h, z }
}

/// Stamp a straight segment into a KIND grid with `tag` (DDA walk).
fn stamp_kind_line(kinds: &mut [u8], w: usize, h: usize, p0: (f32, f32), p1: (f32, f32), tag: u8) {
    let steps = ((p1.0 - p0.0).abs().max((p1.1 - p0.1).abs())).ceil() as i64;
    let n = steps.max(1);
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let c = (p0.0 + (p1.0 - p0.0) * t).round() as i64;
        let r = (p0.1 + (p1.1 - p0.1) * t).round() as i64;
        if c >= 0 && r >= 0 && (c as usize) < w && (r as usize) < h {
            kinds[r as usize * w + c as usize] = tag;
        }
    }
}

/// The natural-landcover overlay for a wilderness terrain scene: blue water +
/// rivers, green forest + park. Roads/paths/buildings are deliberately excluded
/// — on a mountain scene they clutter; the hypsometric terrain tint carries it.
pub const LANDCOVER: [GeoKind; 4] = [
    GeoKind::Water,
    GeoKind::Stream,
    GeoKind::Woods,
    GeoKind::Park,
];

/// Rasterize selected map features' semantic KIND onto a `W×H` grid, so the
/// `overlay` classes (e.g. [`LANDCOVER`]) drape onto the terrain heightfield.
/// Bare terrain stays [`GeoKind::Other`] (the shader tints it hypsometrically by
/// height). Draw order: areas (polygons) first, then lines on top, so a river on
/// a lake reads as river. Contours are the height source and are not stamped.
#[must_use]
pub fn kind_grid(dec: &Decoded, bbox: Bbox, w: usize, h: usize, overlay: &[GeoKind]) -> Vec<u8> {
    let mut kinds = vec![GeoKind::Other.tag(); w * h];
    let mut ordered: Vec<&Feature> = dec
        .features
        .iter()
        .filter(|f| !f.is_contour() && overlay.contains(&f.geo_kind()))
        .collect();
    ordered.sort_by_key(|f| match f.kind {
        Kind::Poly => 0u8,
        Kind::Line => 1,
        _ => 2,
    });
    for f in ordered {
        let tag = f.geo_kind().tag();
        if f.coords.len() == 1 {
            let p = project(bbox, f.coords[0].0, f.coords[0].1, w, h);
            stamp_kind_line(&mut kinds, w, h, p, p, tag);
            continue;
        }
        for pair in f.coords.windows(2) {
            let p0 = project(bbox, pair[0].0, pair[0].1, w, h);
            let p1 = project(bbox, pair[1].0, pair[1].1, w, h);
            stamp_kind_line(&mut kinds, w, h, p0, p1, tag);
        }
    }
    kinds
}

/// Parse a contour label ("6000") to an elevation. `feet` converts US-topo feet
/// to metres (OTM labels are already metres).
fn label_elevation(text: &str, feet: bool) -> Option<f32> {
    let digits: String = text
        .chars()
        .skip_while(|c| !c.is_ascii_digit() && *c != '-')
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect();
    let raw: f32 = digits.parse().ok()?;
    Some(if feet { raw * 0.3048 } else { raw })
}

/// The labeled contour polylines of one LOD `level`, paired with their elevation
/// in metres. Borrows the decoded features.
#[must_use]
pub fn contours_for_level<'a>(
    dec: &'a Decoded,
    lbl: &super::lbl::Lbl<'_>,
    level: u8,
    feet: bool,
) -> Vec<(&'a [(i64, i64)], f32)> {
    dec.features
        .iter()
        .filter(|f: &&Feature| f.is_contour() && f.level == level && f.lbl != 0)
        .filter_map(|f| label_elevation(&lbl.text(f.lbl), feet).map(|e| (f.coords.as_slice(), e)))
        .collect()
}

/// Bake one LOD level's contours into a `W×H` heightfield over the TRE bbox.
#[must_use]
pub fn heightfield_for_level(
    dec: &Decoded,
    lbl: &super::lbl::Lbl<'_>,
    level: u8,
    w: usize,
    h: usize,
    feet: bool,
) -> HeightField {
    let contours = contours_for_level(dec, lbl, level, feet);
    grid_from_contours(&contours, dec.tre.bbox, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_elevation_feet_and_metres() {
        assert!((label_elevation("6000", true).unwrap() - 1828.8).abs() < 0.1);
        assert_eq!(label_elevation("500", false), Some(500.0));
        assert_eq!(label_elevation("", true), None);
    }

    #[test]
    fn synthetic_two_contours_ramp_linearly() {
        // Two horizontal contours (bottom row elev 0, top row elev 100) over a
        // unit bbox → the fill must ramp ~linearly with row.
        let bbox = Bbox {
            north: 1_000_000,
            east: 1_000_000,
            south: 0,
            west: 0,
        };
        let w = 32;
        let h = 32;
        // North edge (row 0) = lat=north → elev 100; south edge (row h-1) = elev 0.
        let top: Vec<(i64, i64)> = (0..w).map(|c| (c as i64 * 32_258, 1_000_000)).collect();
        let bot: Vec<(i64, i64)> = (0..w).map(|c| (c as i64 * 32_258, 0)).collect();
        let contours: Vec<(&[(i64, i64)], f32)> = vec![(&top, 100.0), (&bot, 0.0)];
        let hf = grid_from_contours(&contours, bbox, w, h);

        // Row 0 ≈ 100, last row ≈ 0, middle ≈ 50, monotone decreasing down.
        assert!((hf.at(w / 2, 0) - 100.0).abs() < 1.0);
        assert!((hf.at(w / 2, h - 1) - 0.0).abs() < 1.0);
        assert!((hf.at(w / 2, h / 2) - 50.0).abs() < 6.0);
        for r in 1..h {
            assert!(
                hf.at(w / 2, r) <= hf.at(w / 2, r - 1) + 0.5,
                "monotone down at row {r}"
            );
        }
    }

    fn village() -> (Decoded, Vec<u8>) {
        let path = format!(
            "{}/../.claude/maps/garmin-grand-canyon/47505316.img",
            env!("CARGO_MANIFEST_DIR")
        );
        let img = super::super::Img::read(&path).expect("read tile");
        let lbl_bytes = img.lbl().expect("lbl").to_vec();
        (img.decode().expect("decode"), lbl_bytes)
    }

    #[test]
    fn village_tile_level4_is_connected_terrain() {
        let (dec, lbl_bytes) = village();
        let lbl = super::super::lbl::parse(&lbl_bytes);

        // Level 4 = the full-detail contour set (50816 contours, 1840..10360 ft).
        let contours = contours_for_level(&dec, &lbl, 4, true);
        assert!(
            contours.len() > 40_000,
            "level 4 contour count: {}",
            contours.len()
        );

        let hf = heightfield_for_level(&dec, &lbl, 4, 256, 256, true);

        // Elevation stays within the real canyon envelope (1840 ft = 560.8 m,
        // 10360 ft = 3157.7 m), with a little slack for the ramp.
        let (lo, hi) = hf.range();
        assert!(lo >= 540.0 && hi <= 3180.0, "range {lo}..{hi} m");
        assert!(
            hi - lo > 1500.0,
            "canyon has real relief: span {} m",
            hi - lo
        );

        // CONNECTED, not a needle field. A *cliff* (a large monotone step — the
        // canyon has genuine ~900 m/cell walls) is fine; a *needle* is an
        // isolated spike that pokes above (or below) ALL four neighbours by a
        // wide margin. The harmonic fill obeys the discrete maximum principle,
        // so a connected surface has essentially none; the old per-cell bake
        // produced thousands. Assert isolated spikes are < 0.1% of interior cells.
        let mut spikes = 0usize;
        for r in 1..hf.h - 1 {
            for c in 1..hf.w - 1 {
                let v = hf.at(c, r);
                let n = [
                    hf.at(c - 1, r),
                    hf.at(c + 1, r),
                    hf.at(c, r - 1),
                    hf.at(c, r + 1),
                ];
                let nmin = n.iter().copied().fold(f32::INFINITY, f32::min);
                let nmax = n.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                if v > nmax + 300.0 || v < nmin - 300.0 {
                    spikes += 1;
                }
            }
        }
        let interior = (hf.h - 2) * (hf.w - 2);
        assert!(
            spikes * 1000 < interior,
            "isolated needle spikes {spikes} of {interior} interior cells (>0.1%)"
        );

        // Fidelity: the field is finite everywhere (no NaN/inf leaked from relax).
        assert!(
            hf.z.iter().all(|v| v.is_finite()),
            "heightfield has non-finite cells"
        );
    }

    #[test]
    fn village_tile_kind_grid_overlays_landcover() {
        let (dec, _lbl) = village();
        let kinds = kind_grid(&dec, dec.tre.bbox, 512, 512, &LANDCOVER);
        assert_eq!(kinds.len(), 512 * 512);

        let n = |g: GeoKind| kinds.iter().filter(|&&k| k == g.tag()).count();
        // Rivers (streams) drape onto the terrain…
        assert!(n(GeoKind::Stream) > 0, "streams stamped");
        // …roads/paths are NOT overlaid (landcover only — no urban clutter).
        assert_eq!(n(GeoKind::Street), 0, "roads excluded from terrain overlay");
        assert_eq!(n(GeoKind::Path), 0, "paths excluded from terrain overlay");
        // The terrain surface still dominates (hypsometric tint carries it).
        assert!(
            n(GeoKind::Other) > kinds.len() / 2,
            "most of the surface is bare terrain: {} of {}",
            n(GeoKind::Other),
            kinds.len()
        );
        assert!(kinds.iter().all(|&k| (k as usize) < GeoKind::PALETTE.len()));
    }
}
