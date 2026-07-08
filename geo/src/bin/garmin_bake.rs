//! `garmin_bake` — bake a Garmin IMG tile into the BSO2 ver-8 radix-grid wire
//! the cockpit `/garmin/<location>` route decodes, via the typed pipeline:
//!
//! ```text
//!   IMG → decode → contours → heightfield (terrain::heightfield_for_level)
//!       → KIND overlay grid (rivers/roads/forest on bare terrain)
//!       → ver-8 wire (height F16 + kind u8 + palette)
//! ```
//!
//! LOD is the TRE level pyramid — pass `--level N` (default 4 = full detail; the
//! canyon carries generalized sets at levels 2/3/4). The rich look (hypsometric
//! elevation tint, the inter-family kurvenlineal brightness, Gouraud normals,
//! sunset light, Ice/Ocean specular) is CLIENT-derived in the shader from the
//! stored height + kind — the ver-8 contract: store only what the address can't
//! reconstruct.
//!
//! Usage: `garmin_bake <in.img> <out.soa> [--level N] [--dim WxH] [--metres]`

use geo_hhtl::bso2::{encode_grid_bso2, MeshConcept, CLASSID_GEO_V3};
use geo_hhtl::garmin::{drape, mu2deg, terrain, Img};
use geo_hhtl::hhtl::point_to_hhtl4;
use geo_hhtl::osm_read::M_PER_DEG;
use lance_graph_contract::canonical_node::{NodeGuid, TailVariant};

const KEY_ZOOM: u32 = 32;
const FAMILY_TERRAIN: u32 = 4;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: garmin_bake <in.img> <out.soa> [--level N] [--dim WxH] [--metres]");
        std::process::exit(2);
    };
    let mut level: u8 = 4;
    let mut dim: Option<(usize, usize)> = None;
    let mut feet = true; // US-topo default; --metres for OTM
    let rest: Vec<String> = args.collect();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--level" => level = it.next().and_then(|s| s.parse().ok()).unwrap_or(4),
            "--metres" | "--meters" => feet = false,
            "--dim" => {
                if let Some(d) = it.next() {
                    if let Some((w, h)) = d.split_once('x') {
                        dim = w.parse().ok().zip(h.parse().ok());
                    }
                }
            }
            _ => {}
        }
    }

    let img = match Img::read(&input) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("garmin_bake: {e}");
            std::process::exit(1);
        }
    };
    let dec = match img.decode() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("garmin_bake: decode: {e}");
            std::process::exit(1);
        }
    };
    let lbl_bytes = img.lbl().map(<[u8]>::to_vec).unwrap_or_default();
    let lbl = geo_hhtl::garmin::lbl::parse(&lbl_bytes);
    let bbox = dec.tre.bbox;

    // Grid dims: keep the tile's aspect at a ~1024-cell long axis unless overridden.
    let (w, h) = dim.unwrap_or_else(|| {
        let deg_w = (mu2deg(bbox.east) - mu2deg(bbox.west)).abs();
        let deg_h = (mu2deg(bbox.north) - mu2deg(bbox.south)).abs();
        let cos_lat = ((mu2deg(bbox.north) + mu2deg(bbox.south)) * 0.5)
            .to_radians()
            .cos();
        let aspect = (deg_w * cos_lat / deg_h.max(1e-9)).clamp(0.25, 4.0);
        if aspect >= 1.0 {
            (1024, (1024.0 / aspect).round() as usize)
        } else {
            ((1024.0 * aspect).round() as usize, 1024)
        }
    });
    eprintln!(
        "tile bbox N{:.4} E{:.4} S{:.4} W{:.4} · level {level} · grid {w}x{h}",
        mu2deg(bbox.north),
        mu2deg(bbox.east),
        mu2deg(bbox.south),
        mu2deg(bbox.west),
    );

    // ── Terrain heightfield (metres) from the labeled contours of this LOD level. ──
    let hf = terrain::heightfield_for_level(&dec, &lbl, level, w, h, feet);
    let (elev_lo, elev_hi) = hf.range();
    eprintln!("heightfield {w}x{h} · elevation {elev_lo:.0}..{elev_hi:.0} m");

    // ── KIND overlay: natural landcover (rivers / lakes / forest) on bare
    //    terrain — roads/paths are excluded so the mountain scene reads as
    //    terrain, not a street grid. ──
    let kinds = terrain::kind_grid(&dec, bbox, w, h, &terrain::LANDCOVER);

    // ── Equirectangular metric projection about the tile centre (matches osm_read /
    //    iceland_dem so the cockpit decoder is shared). ──
    let lon0 = (mu2deg(bbox.west) + mu2deg(bbox.east)) * 0.5;
    let lat0 = (mu2deg(bbox.north) + mu2deg(bbox.south)) * 0.5;
    let cos_lat0 = lat0.to_radians().cos();
    let lon_at = |c: usize| {
        mu2deg(bbox.west)
            + (mu2deg(bbox.east) - mu2deg(bbox.west)) * c as f64 / (w - 1).max(1) as f64
    };
    let lat_at = |r: usize| {
        mu2deg(bbox.north)
            - (mu2deg(bbox.north) - mu2deg(bbox.south)) * r as f64 / (h - 1).max(1) as f64
    };

    let mut mx = vec![0.0f32; w * h];
    let mut mz = vec![0.0f32; w * h];
    for r in 0..h {
        let z = ((lat_at(r) - lat0) * M_PER_DEG) as f32;
        for c in 0..w {
            let x = ((lon_at(c) - lon0) * cos_lat0 * M_PER_DEG) as f32;
            mx[r * w + c] = x;
            mz[r * w + c] = z;
        }
    }
    // Normalize horizontal extent to [-1,1]; elevation is TRUE-SCALE (÷ same half)
    // — the shader raises it by uExag, exactly like the Iceland terrain.
    let (mut lox, mut hix, mut loz, mut hiz) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for i in 0..w * h {
        lox = lox.min(mx[i]);
        hix = hix.max(mx[i]);
        loz = loz.min(mz[i]);
        hiz = hiz.max(mz[i]);
    }
    let (cx, cz) = ((lox + hix) * 0.5, (loz + hiz) * 0.5);
    let half = ((hix - lox).max(hiz - loz) * 0.5).max(1.0);
    let inv = 1.0 / half;

    let mut pos = vec![[0.0f32; 3]; w * h];
    for (i, p) in pos.iter_mut().enumerate() {
        *p = [(mx[i] - cx) * inv, hf.z[i] * inv, (mz[i] - cz) * inv];
    }

    // ── Concepts = grid rows (contiguous vertex strips), keyed by HHTL address. ──
    let mid = w / 2;
    let mut concepts = Vec::with_capacity(h);
    for r in 0..h {
        let v_start = (r * w) as u32;
        let mut ctr = [0.0f32; 3];
        for p in &pos[r * w..(r + 1) * w] {
            ctr[0] += p[0];
            ctr[1] += p[1];
            ctr[2] += p[2];
        }
        let invn = 1.0 / w as f32;
        let centroid = [ctr[0] * invn, ctr[1] * invn, ctr[2] * invn];
        let key4 = point_to_hhtl4(lon_at(mid), lat_at(r), KEY_ZOOM);
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
            layer: 4,
            label: 0,
            centroid,
            v_start,
            v_count: w as u32,
        });
    }

    // ── ver-8 radix-grid encode: height F16 + kind u8 + GeoKind palette. ──
    let palette: Vec<[u8; 3]> = geo_hhtl::garmin::GeoKind::PALETTE
        .iter()
        .map(|k| k.color())
        .collect();
    let ymax = pos.iter().map(|p| p[1]).fold(1e-9f32, f32::max);
    let heights: Vec<f32> = pos.iter().map(|p| p[1] / ymax).collect();
    let x0 = pos[0][0];
    let dx = if w > 1 { pos[1][0] - pos[0][0] } else { 0.0 };
    let zrow: Vec<f32> = (0..h).map(|r| pos[r * w][2]).collect();
    let labels = br#"{"names":["garmin-terrain"]}"#;

    let (soa, blocks) = encode_grid_bso2(
        w as u32, h as u32, x0, dx, ymax, &zrow, &heights, &kinds, &palette, &concepts, labels,
    );
    eprintln!(
        "BSO2 ver8: {} concepts · {} verts · {} tris · {} B soa · {} B blocks",
        concepts.len(),
        w * h,
        (w - 1) * (h - 1) * 2,
        soa.len(),
        blocks.len()
    );

    // ── OSM ⊕ Garmin drape: lift the typed line network (roads / trails / rivers)
    //    onto the SAME terrain surface (`pos`), co-registered with the ver-8 grid.
    //    Emitted as a separate DRP1 sidecar so the proven grid wire is untouched. ──
    let drape_lines = drape::build_drape(&dec, bbox, &pos, w, h, level, &drape::DRAPE_KINDS);
    let drape_bytes = drape::encode_drape(&drape_lines, &palette);
    let drape_pts: usize = drape_lines.iter().map(|l| l.pts.len()).sum();
    eprintln!(
        "DRP1 drape: {} lines · {} pts · {} B (level {level} road/trail/river network)",
        drape_lines.len(),
        drape_pts,
        drape_bytes.len(),
    );

    let stem = output.strip_suffix(".soa").unwrap_or(&output);
    let blocks_path = format!("{stem}.blocks");
    let drape_path = format!("{stem}.drape.soa");
    if let Err(e) = std::fs::write(&output, &soa)
        .and_then(|()| std::fs::write(&blocks_path, &blocks))
        .and_then(|()| std::fs::write(&drape_path, &drape_bytes))
    {
        eprintln!("garmin_bake: write: {e}");
        std::process::exit(1);
    }
    eprintln!("wrote {output} + {blocks_path} + {drape_path}");
}
