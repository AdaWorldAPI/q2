//! `iceland_dem` — bake a real Iceland DEM heightfield into the BSO2 `/helix`
//! wire so `/ice` renders SOLID terrain (not the sparse scatter).
//!
//! Input is a `.demgrid` produced by `scripts/fetch_iceland_dem.py` (keyless AWS
//! Terrarium terrain-RGB tiles → stitched, downsampled elevation raster). We turn
//! the W×H elevation grid into a shared-vertex heightfield mesh (two triangles
//! per cell, per-vertex normals from the terrain gradient), normalize it to the
//! viewer's `[-1,1]` frame TRUE-SCALE (elevation in the same metric units as the
//! horizontal extent — the shader applies `uExag`, so we do NOT pre-exaggerate),
//! and run it through the SAME `bso2::encode_mesh_bso2` path the OSM bake uses
//! (F16 pos + Signed360 normals + HXFL trailer). Sea level (elev ≤ 0) → y = 0,
//! which the shader paints as ocean.
//!
//! ```text
//! usage: iceland_dem <in.demgrid> <out.soa>   (writes <out>.soa + <out>.blocks)
//! ```
//! Gzip `<out>.soa` afterwards (BodyHelix fetches a gzip'd artifact).

use geo_hhtl::bso2::{encode_mesh_bso2, MeshConcept, CLASSID_GEO_V3};
use geo_hhtl::hhtl::point_to_hhtl4;
use geo_hhtl::osm_read::M_PER_DEG;

use lance_graph_contract::canonical_node::{NodeGuid, TailVariant};

/// Deep key zoom for the 4-tier HHTL cascade (matches the OSM bake's KEY_ZOOM).
const KEY_ZOOM: u32 = 32;
/// Terrain basin tier (family) — a natural-area basin, distinct from the OSM
/// building/road/area families (1/2/3); 4 = "terrain".
const FAMILY_TERRAIN: u32 = 4;

struct Dem {
    w: usize,
    h: usize,
    west: f64,
    east: f64,
    lats: Vec<f64>,  // len h, row 0 = north
    elev: Vec<f32>,  // len w*h, row-major, row 0 = north, col 0 = west (metres)
}

fn read_demgrid(path: &str) -> Result<Dem, Box<dyn std::error::Error>> {
    let b = std::fs::read(path)?;
    if b.len() < 24 || &b[0..4] != b"DEMG" {
        return Err("not a DEMG file".into());
    }
    let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let f64_at = |o: usize| f64::from_le_bytes(b[o..o + 8].try_into().unwrap());
    let ver = u32_at(4);
    if ver != 1 {
        return Err(format!("unsupported DEMG version {ver}").into());
    }
    let w = u32_at(8) as usize;
    let h = u32_at(12) as usize;
    let west = f64_at(16);
    let east = f64_at(24);
    let mut o = 32;
    let mut lats = Vec::with_capacity(h);
    for _ in 0..h {
        lats.push(f64_at(o));
        o += 8;
    }
    let n = w * h;
    let mut elev = Vec::with_capacity(n);
    for _ in 0..n {
        elev.push(f32::from_le_bytes(b[o..o + 4].try_into().unwrap()));
        o += 4;
    }
    Ok(Dem { w, h, west, east, lats, elev })
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: iceland_dem <in.demgrid> <out.soa>");
        std::process::exit(2);
    };
    let dem = match read_demgrid(&input) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("iceland_dem: {e}");
            std::process::exit(1);
        }
    };
    let (w, h) = (dem.w, dem.h);
    eprintln!(
        "DEM {w}x{h} verts · lon {:.4}..{:.4} · lat {:.4}..{:.4}",
        dem.west, dem.east, dem.lats[0], dem.lats[h - 1]
    );

    // ── Equirectangular projection about the grid centre (matches osm_read). ──
    let lon0 = (dem.west + dem.east) * 0.5;
    let lat0 = (dem.lats[0] + dem.lats[h - 1]) * 0.5;
    let cos_lat0 = lat0.to_radians().cos();
    let lon_at = |c: usize| dem.west + (dem.east - dem.west) * c as f64 / (w - 1).max(1) as f64;
    // metric (x = east, z = north) in metres, y = elevation (sea clamped to 0).
    let mut mx = vec![0.0f32; w * h];
    let mut mz = vec![0.0f32; w * h];
    let mut my = vec![0.0f32; w * h];
    for r in 0..h {
        let z = ((dem.lats[r] - lat0) * M_PER_DEG) as f32;
        for c in 0..w {
            let x = ((lon_at(c) - lon0) * cos_lat0 * M_PER_DEG) as f32;
            let i = r * w + c;
            mx[i] = x;
            mz[i] = z;
            // Sea level & bathymetry → 0; clamp the top to Iceland's physical max (Hvannadalshnúkur
            // ≈ 2110 m) so a Terrarium nodata sentinel or tile-decode outlier can't become a needle
            // spike in the heightfield. A real heightfield is continuous, so the only way to needle
            // is an out-of-range vertex — this removes that failure mode at the source.
            my[i] = dem.elev[i].clamp(0.0, 2200.0);
        }
    }

    // ── Normalize to [-1,1] by horizontal extent; elevation TRUE-SCALE (÷half). ──
    let (mut lox, mut hix, mut loz, mut hiz) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for i in 0..w * h {
        lox = lox.min(mx[i]);
        hix = hix.max(mx[i]);
        loz = loz.min(mz[i]);
        hiz = hiz.max(mz[i]);
    }
    let cx = (lox + hix) * 0.5;
    let cz = (loz + hiz) * 0.5;
    let half = ((hix - lox).max(hiz - loz) * 0.5).max(1.0);
    let inv = 1.0 / half;
    // DISPLAY frame (Dx, Dy=up=elevation, Dz). No vertical exaggeration — the
    // shader raises the true-scale island by uExag=10 + the Kurvenlineal.
    let mut pos = vec![[0.0f32; 3]; w * h];
    for i in 0..w * h {
        pos[i] = [(mx[i] - cx) * inv, my[i] * inv, (mz[i] - cz) * inv];
    }

    // ── Per-vertex normals from the terrain gradient (display frame). ──
    // du ≈ +east (col+), dv ≈ +north (row-, since row 0 is north). cross(dv,du)
    // points +Dy on flat ground and tilts with the slope. One-sided at edges.
    let idx = |r: usize, c: usize| r * w + c;
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
    };
    let mut nrm = vec![[0.0f32, 1.0, 0.0]; w * h];
    for r in 0..h {
        let rn = r.saturating_sub(1); // north neighbour (smaller row)
        let rs = (r + 1).min(h - 1); // south neighbour
        for c in 0..w {
            let cw = c.saturating_sub(1);
            let ce = (c + 1).min(w - 1);
            let du = sub(pos[idx(r, ce)], pos[idx(r, cw)]); // east tangent
            let dv = sub(pos[idx(rn, c)], pos[idx(rs, c)]); // north tangent
            let mut n = cross(dv, du);
            let m = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if m > 1e-12 {
                n = [n[0] / m, n[1] / m, n[2] / m];
                if n[1] < 0.0 {
                    n = [-n[0], -n[1], -n[2]]; // keep the up-facing hemisphere
                }
            } else {
                n = [0.0, 1.0, 0.0];
            }
            nrm[idx(r, c)] = n;
        }
    }

    // ── Triangles: two per grid cell, shared vertices (watertight surface). ──
    let mut tris: Vec<[u32; 3]> = Vec::with_capacity((w - 1) * (h - 1) * 2);
    for r in 0..h - 1 {
        for c in 0..w - 1 {
            let a = idx(r, c) as u32;
            let b = idx(r, c + 1) as u32;
            let d = idx(r + 1, c) as u32;
            let e = idx(r + 1, c + 1) as u32;
            // Wound so the THREE.js face normal (v1-v0)×(v2-v0) points +Dy (up):
            // the terrain top is the FrontSide face an overhead camera sees.
            tris.push([a, b, d]);
            tris.push([b, e, d]);
        }
    }

    // ── Concepts = grid rows (contiguous vertex ranges; the per-concept
    //    max-diameter clamp stays a thin full-width strip → drops nothing). ──
    let rows: Vec<u32> = (0..h).flat_map(|r| std::iter::repeat(r as u32).take(w)).collect();
    let mid = w / 2;
    let mut concepts = Vec::with_capacity(h);
    for r in 0..h {
        let v_start = (r * w) as u32;
        let mut c = [0.0f32; 3];
        for i in r * w..(r + 1) * w {
            c[0] += pos[i][0];
            c[1] += pos[i][1];
            c[2] += pos[i][2];
        }
        let invn = 1.0 / w as f32;
        let centroid = [c[0] * invn, c[1] * invn, c[2] * invn];
        let key4 = point_to_hhtl4(lon_at(mid), dem.lats[r], KEY_ZOOM);
        let key = NodeGuid::mint_for(
            TailVariant::V3,
            CLASSID_GEO_V3,
            key4.heel,
            key4.hip,
            key4.twig,
            key4.leaf,
            FAMILY_TERRAIN,
            r as u32,
        );
        concepts.push(MeshConcept {
            key,
            layer: 4, // default-on geo toggle group
            label: 0, // names[0] = "iceland-terrain"
            centroid,
            v_start,
            v_count: w as u32,
        });
    }

    let labels = br#"{"names":["iceland-terrain"]}"#;
    let (soa, blocks) = encode_mesh_bso2(&pos, &nrm, &rows, &tris, &concepts, labels);

    let nc = u32::from_le_bytes(soa[6..10].try_into().unwrap());
    let nv = u32::from_le_bytes(soa[10..14].try_into().unwrap());
    let nt = u32::from_le_bytes(soa[14..18].try_into().unwrap());
    eprintln!(
        "BSO2: {nc} concepts · {nv} verts · {nt} tris · {} B soa · {} B blocks",
        soa.len(),
        blocks.len()
    );

    let blocks_path = format!("{}.blocks", output.strip_suffix(".soa").unwrap_or(&output));
    if let Err(e) =
        std::fs::write(&output, &soa).and_then(|()| std::fs::write(&blocks_path, &blocks))
    {
        eprintln!("iceland_dem: write: {e}");
        std::process::exit(1);
    }
    eprintln!("wrote {output} + {blocks_path}");
}
