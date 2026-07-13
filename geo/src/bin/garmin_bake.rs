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
        eprintln!(
            "usage: garmin_bake <in.img> <out.soa> [--level N] [--contour-level N] [--dim WxH] [--metres] [--arid]"
        );
        std::process::exit(2);
    };
    let mut level: u8 = 4;
    let mut dim: Option<(usize, usize)> = None;
    let mut feet = true; // US-topo default; --metres for OTM
    let mut arid = false; // --arid: desert scene — dry-wash drainage is rust-brown, not blue
    let mut contour_level: u8 = 3; // --contour-level N: contour overlay LOD (3 ≈ 8k lines, OTM-dense)
    // --dem <file.demgrid>: source the heightfield from a real dense DEM (Terrarium)
    // instead of the sparse Garmin contours — the "surfel" density. A v2 demgrid also
    // carries a raw ESRI satellite drape, baked per-vertex as the ver-9 photoreal skin.
    let mut dem_path: Option<String> = None;
    // --crop W,S,E,N (decimal degrees): bake ONLY this sub-window of the tile — cut the
    // dead plateau, keep the canyon. HHTL-safe (keys are absolute lon/lat). Skips the
    // Garmin drape/contour (those are tile-bbox features; a cropped photoreal scene is
    // terrain + skin). Pairs with --dem.
    let mut crop: Option<(f64, f64, f64, f64)> = None; // (west, south, east, north)
    let rest: Vec<String> = args.collect();
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--level" => level = it.next().and_then(|s| s.parse().ok()).unwrap_or(4),
            "--contour-level" => {
                contour_level = it.next().and_then(|s| s.parse().ok()).unwrap_or(3);
            }
            "--metres" | "--meters" => feet = false,
            "--arid" => arid = true,
            "--dem" => dem_path = it.next().cloned(),
            "--crop" => {
                if let Some(s) = it.next() {
                    let v: Vec<f64> = s.split(',').filter_map(|x| x.parse().ok()).collect();
                    if v.len() == 4 {
                        crop = Some((v[0], v[1], v[2], v[3]));
                    }
                }
            }
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

    // --dem: source the heightfield (+ raw ESRI skin for a v2 demgrid) from a real
    // dense DEM instead of the sparse Garmin contours. Sampled per grid cell below.
    let dem = match &dem_path {
        Some(p) => match read_dem(p) {
            Ok(d) => {
                eprintln!(
                    "DEM {}x{} · lon {:.4}..{:.4} · lat {:.4}..{:.4} · {} skin",
                    d.w, d.h, d.west, d.east, d.lats[0], d.lats[d.h - 1],
                    if d.rgb.is_empty() { "no" } else { "raw ESRI" }
                );
                Some(d)
            }
            Err(e) => {
                eprintln!("garmin_bake: --dem: {e}");
                std::process::exit(1);
            }
        },
        None => None,
    };

    // The grid's geographic window: the full Garmin tile, OR the --crop sub-window
    // (cut the dead plateau). Everything downstream (dims, projection, HHTL keys) reads
    // these four degrees, so the crop is a pure bbox change — no HHTL remapping.
    let (gw, gs, ge, gn) = match crop {
        Some((w, s, e, n)) => (w, s, e, n),
        None => (
            mu2deg(bbox.west),
            mu2deg(bbox.south),
            mu2deg(bbox.east),
            mu2deg(bbox.north),
        ),
    };

    // Grid dims: explicit --dim wins; else with a DEM match its native resolution
    // over the window (dense surfels, ~1:1 sampling); else the sparse-contour
    // ~1024-cell long axis.
    let (w, h) = dim.unwrap_or_else(|| {
        let deg_w = (ge - gw).abs();
        let deg_h = (gn - gs).abs();
        if let Some(d) = &dem {
            let dem_dlon = (d.east - d.west).abs() / (d.w - 1).max(1) as f64;
            let dem_dlat = (d.lats[0] - d.lats[d.h - 1]).abs() / (d.h - 1).max(1) as f64;
            let cw = (deg_w / dem_dlon.max(1e-9)).round().max(2.0) as usize;
            let ch = (deg_h / dem_dlat.max(1e-9)).round().max(2.0) as usize;
            (cw, ch)
        } else {
            let cos_lat = ((gn + gs) * 0.5).to_radians().cos();
            let aspect = (deg_w * cos_lat / deg_h.max(1e-9)).clamp(0.25, 4.0);
            if aspect >= 1.0 {
                (1024, (1024.0 / aspect).round() as usize)
            } else {
                ((1024.0 * aspect).round() as usize, 1024)
            }
        }
    });
    eprintln!(
        "window N{gn:.4} E{ge:.4} S{gs:.4} W{gw:.4} · level {level} · grid {w}x{h}{}",
        if crop.is_some() { " (CROPPED)" } else { "" }
    );

    // ── Terrain heightfield (metres): the sparse labeled contours of this LOD level,
    //    UNLESS a dense DEM was supplied (then it is sampled per-cell below). ──
    let hf = dem
        .is_none()
        .then(|| terrain::heightfield_for_level(&dec, &lbl, level, w, h, feet));

    // ── KIND overlay: natural landcover (rivers / lakes / forest) on bare
    //    terrain — roads/paths are excluded so the mountain scene reads as
    //    terrain, not a street grid. The rasterization bbox is the GRID's window
    //    (the --crop sub-window when cropping), NOT the tile bbox — otherwise the
    //    Water mask lands misregistered on a cropped grid and the river material
    //    (ver-9 aWet) paints the wrong cells. deg→mu is the inverse of mu2deg. ──
    let deg2mu = |d: f64| (d * (1u32 << 24) as f64 / 360.0).round() as i32;
    let kbbox = geo_hhtl::garmin::tre::Bbox {
        north: deg2mu(gn),
        east: deg2mu(ge),
        south: deg2mu(gs),
        west: deg2mu(gw),
    };
    let mut kinds = terrain::kind_grid(&dec, kbbox, w, h, &terrain::LANDCOVER);
    // On an arid scene, blue is reserved for the ONE persistent water body — the
    // flowing Colorado (Garmin river-fill type 0x4c). Its stamp rasterizes to a
    // ~1-cell hairline, so widen ONLY the river ×2 (ribbon, the way it dominates
    // the real canyon). The still "lakes" (stock tanks / ephemeral ponds) are NOT
    // blue on an arid landscape: retag them to the SUBTLE slot — a barely-there
    // grey-blue fleck, deliberately below the shader's blue-dominance `wet`
    // threshold so no vivid water treatment fires. (Non-arid scenes keep the raw
    // stamp for everything.)
    if arid {
        let wtag = geo_hhtl::garmin::GeoKind::Water.tag();
        // Keep only LARGE connected components of the river fill before widening:
        // Garmin's 0x4c class also stamps hundreds of isolated 1–4-cell flecks across
        // the plateau (intermittent pools) — at altitude the region must read DRY
        // (operator art direction), and the ver-9 river material must paint ONLY the
        // Colorado. The ribbon is thousands of connected cells; 25 kills the flecks
        // at every grid scale. Dropped flecks fall through to the still-water branch
        // below (→ LAKE_TAG subtle), exactly like the stock tanks.
        let raw_river = terrain::river_fill_grid(&dec, kbbox, w, h);
        let mut big = vec![0u8; w * h];
        let mut seen = vec![false; w * h];
        let mut stack = Vec::new();
        for start in 0..w * h {
            if raw_river[start] != wtag || seen[start] {
                continue;
            }
            let mut comp = vec![start];
            seen[start] = true;
            stack.push(start);
            while let Some(i) = stack.pop() {
                let (r, c) = (i / w, i % w);
                for (nr, nc) in [
                    (r.wrapping_sub(1), c),
                    (r + 1, c),
                    (r, c.wrapping_sub(1)),
                    (r, c + 1),
                ] {
                    if nr < h && nc < w {
                        let j = nr * w + nc;
                        if raw_river[j] == wtag && !seen[j] {
                            seen[j] = true;
                            stack.push(j);
                            comp.push(j);
                        }
                    }
                }
            }
            if comp.len() >= 25 {
                for i in comp {
                    big[i] = wtag;
                }
            }
        }
        let river = terrain::dilate_kind(&big, w, h, wtag, 2);
        let (mut river_cells, mut widened, mut lakes) = (0usize, 0usize, 0usize);
        for (i, k) in kinds.iter_mut().enumerate() {
            if river[i] == wtag {
                river_cells += 1;
                if *k != wtag {
                    widened += 1;
                    *k = wtag;
                }
            } else if *k == wtag {
                lakes += 1;
                *k = LAKE_TAG; // still water → subtle fleck, not bright blue
            }
        }
        eprintln!(
            "arid river: widened Colorado (type 0x4c ×2) to {river_cells} cells (+{widened}); {lakes} still-water cells → subtle tag {LAKE_TAG}"
        );
    }

    // ── Equirectangular metric projection about the tile centre (matches osm_read /
    //    iceland_dem so the cockpit decoder is shared). ──
    let lon0 = (gw + ge) * 0.5;
    let lat0 = (gn + gs) * 0.5;
    let cos_lat0 = lat0.to_radians().cos();
    let lon_at = |c: usize| gw + (ge - gw) * c as f64 / (w - 1).max(1) as f64;
    let lat_at = |r: usize| gn - (gn - gs) * r as f64 / (h - 1).max(1) as f64;

    // Metric x/z per cell, plus per-cell height (metres) and — for a v2 DEM — the
    // raw ESRI satellite colour (the ver-9 photoreal skin). Heights come from the
    // dense DEM (bilinear at the cell's true lon/lat) or the sparse contour field.
    let mut mx = vec![0.0f32; w * h];
    let mut mz = vec![0.0f32; w * h];
    let mut heights_m = vec![0.0f32; w * h];
    let want_skin = dem.as_ref().is_some_and(|d| !d.rgb.is_empty());
    let mut colors: Vec<[u8; 3]> = if want_skin { vec![[0u8; 3]; w * h] } else { Vec::new() };
    for r in 0..h {
        let lat = lat_at(r);
        let z = ((lat - lat0) * M_PER_DEG) as f32;
        for c in 0..w {
            let i = r * w + c;
            let lon = lon_at(c);
            mx[i] = ((lon - lon0) * cos_lat0 * M_PER_DEG) as f32;
            mz[i] = z;
            heights_m[i] = match &dem {
                Some(d) => d.elev_at(lon, lat),
                None => hf.as_ref().unwrap().z[i],
            };
            if want_skin {
                colors[i] = dem.as_ref().unwrap().rgb_at(lon, lat);
            }
        }
    }
    let (elev_lo, elev_hi) = heights_m
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    eprintln!(
        "heightfield {w}x{h} · elevation {elev_lo:.0}..{elev_hi:.0} m · {}",
        if dem.is_some() { "DEM surfels" } else { "Garmin contours" }
    );
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
        *p = [(mx[i] - cx) * inv, heights_m[i] * inv, (mz[i] - cz) * inv];
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

    // ── ver-8 radix-grid encode: height F16 + kind u8 + GeoKind palette. On an
    //    arid desert scene the dendritic `Stream` network is DRY drainage (washes /
    //    gullies), so recolour it rust-brown and keep blue for the actual `Water`
    //    bodies (the Colorado). One palette feeds BOTH the terrain KIND block and the
    //    DRP1 drape below, so the drainage browns consistently across both. ──
    let mut palette: Vec<[u8; 3]> = if arid {
        geo_hhtl::garmin::GeoKind::arid_palette()
    } else {
        geo_hhtl::garmin::GeoKind::PALETTE
            .iter()
            .map(|k| k.color())
            .collect()
    };
    if arid {
        // Slot 9 = LAKE_TAG. The ver-8 wire carries its palette count (nK u8) and the
        // client indexes palette[kind] from the wire, so a 10th entry is decode-safe.
        // Hard assert (release bakes too): if GeoKind::PALETTE ever grows, LAKE_TAG
        // would silently alias a real slot and corrupt the shipped palette.
        assert_eq!(
            palette.len() as u8,
            LAKE_TAG,
            "GeoKind::PALETTE length drifted from LAKE_TAG — update the slot constant"
        );
        palette.push(LAKE_SUBTLE);
        eprintln!(
            "arid palette: Stream drainage → rust-brown, Water river-blue, still lakes → subtle"
        );
    }
    let ymax = pos.iter().map(|p| p[1]).fold(1e-9f32, f32::max);
    let heights: Vec<f32> = pos.iter().map(|p| p[1] / ymax).collect();
    let x0 = pos[0][0];
    let dx = if w > 1 { pos[1][0] - pos[0][0] } else { 0.0 };
    let zrow: Vec<f32> = (0..h).map(|r| pos[r * w][2]).collect();
    let labels = br#"{"names":["garmin-terrain"]}"#;

    // ver-9 (raw satellite skin) when `colors` is populated, else ver-8 (palette).
    let (soa, blocks) = encode_grid_bso2(
        w as u32, h as u32, x0, dx, ymax, &zrow, &heights, &kinds, &palette, &concepts, labels,
        &colors,
    );
    eprintln!(
        "BSO2 ver{}: {} concepts · {} verts · {} tris · {} B soa · {} B blocks",
        if colors.is_empty() { 8 } else { 9 },
        concepts.len(),
        w * h,
        (w - 1) * (h - 1) * 2,
        soa.len(),
        blocks.len()
    );

    let stem = output.strip_suffix(".soa").unwrap_or(&output);
    let blocks_path = format!("{stem}.blocks");
    if let Err(e) =
        std::fs::write(&output, &soa).and_then(|()| std::fs::write(&blocks_path, &blocks))
    {
        eprintln!("garmin_bake: write: {e}");
        std::process::exit(1);
    }
    let mut wrote = format!("{output} + {blocks_path}");

    // ── OSM ⊕ Garmin drape + contour overlays: the typed line network (roads / trails /
    //    rivers) and the labeled contour polylines, lifted onto the terrain surface as
    //    DRP1 sidecars. Emitted ONLY for the full tile — a --crop window is a photoreal
    //    terrain+skin scene, and the Garmin features are addressed in the full-tile frame
    //    (would not co-register with the cropped grid), so they are skipped. ──
    if crop.is_none() {
        let drape_lines = drape::build_drape(&dec, bbox, &pos, w, h, level, &drape::DRAPE_KINDS);
        let drape_bytes = drape::encode_drape(&drape_lines, &palette);
        let drape_pts: usize = drape_lines.iter().map(|l| l.pts.len()).sum();
        eprintln!(
            "DRP1 drape: {} lines · {} pts · {} B (level {level} road/trail/river network)",
            drape_lines.len(),
            drape_pts,
            drape_bytes.len(),
        );
        let contour_palette = {
            let mut p = palette.clone();
            p[geo_hhtl::garmin::GeoKind::Contour.tag() as usize] = CONTOUR_LINE;
            p
        };
        let contour_lines = drape::build_drape(
            &dec,
            bbox,
            &pos,
            w,
            h,
            contour_level,
            &[geo_hhtl::garmin::GeoKind::Contour],
        );
        let contour_bytes = drape::encode_drape(&contour_lines, &contour_palette);
        let contour_pts: usize = contour_lines.iter().map(|l| l.pts.len()).sum();
        eprintln!(
            "DRP1 contours: {} lines · {} pts · {} B (level {contour_level} topo lines)",
            contour_lines.len(),
            contour_pts,
            contour_bytes.len(),
        );
        let drape_path = format!("{stem}.drape.soa");
        let contour_path = format!("{stem}.contour.soa");
        if let Err(e) = std::fs::write(&drape_path, &drape_bytes)
            .and_then(|()| std::fs::write(&contour_path, &contour_bytes))
        {
            eprintln!("garmin_bake: write: {e}");
            std::process::exit(1);
        }
        wrote = format!("{wrote} + {drape_path} + {contour_path}");
    }
    eprintln!("wrote {wrote}");
}

/// A dense DEM grid read from a `.demgrid` (DEMG v1/v2, produced by
/// `scripts/fetch_iceland_dem.py`). Row 0 = north, col 0 = west; `lon` is linear
/// across columns, `lat` tabulated per row (WebMercator spacing). v2 adds a raw
/// ESRI satellite drape (`rgb`), the photoreal skin the ver-9 wire carries.
struct Dem {
    w: usize,
    h: usize,
    west: f64,
    east: f64,
    lats: Vec<f64>,    // len h, decreasing (row 0 = north)
    elev: Vec<f32>,    // len w*h, metres, row-major
    rgb: Vec<[u8; 3]>, // len w*h (v2) or empty (v1)
}

fn read_dem(path: &str) -> Result<Dem, String> {
    let b = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
    if b.len() < 32 || &b[0..4] != b"DEMG" {
        return Err(format!("{path}: not a DEMG file"));
    }
    let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let f64_at = |o: usize| f64::from_le_bytes(b[o..o + 8].try_into().unwrap());
    let ver = u32_at(4);
    if ver != 1 && ver != 2 {
        return Err(format!("{path}: unsupported DEMG version {ver}"));
    }
    let (w, h) = (u32_at(8) as usize, u32_at(12) as usize);
    let (west, east) = (f64_at(16), f64_at(24));
    let n = w * h;
    let mut o = 32usize;
    let need = 32 + h * 8 + n * 4 + if ver == 2 { n * 3 } else { 0 };
    if b.len() < need {
        return Err(format!("{path}: truncated (need {need} B, have {})", b.len()));
    }
    let mut lats = Vec::with_capacity(h);
    for _ in 0..h {
        lats.push(f64_at(o));
        o += 8;
    }
    let mut elev = Vec::with_capacity(n);
    for _ in 0..n {
        elev.push(f32::from_le_bytes(b[o..o + 4].try_into().unwrap()));
        o += 4;
    }
    let mut rgb = Vec::new();
    if ver == 2 {
        rgb.reserve(n);
        for _ in 0..n {
            rgb.push([b[o], b[o + 1], b[o + 2]]);
            o += 3;
        }
    }
    Ok(Dem { w, h, west, east, lats, elev, rgb })
}

impl Dem {
    /// Fractional column for a longitude (linear west→east), clamped to the grid.
    fn colf(&self, lon: f64) -> f64 {
        let span = (self.east - self.west).abs().max(1e-12);
        ((lon - self.west) / span * (self.w - 1) as f64).clamp(0.0, (self.w - 1) as f64)
    }

    /// Fractional row for a latitude — the DEM rows are WebMercator (non-uniform),
    /// so bracket the decreasing `lats` table by binary search and interpolate.
    fn rowf(&self, lat: f64) -> f64 {
        if lat >= self.lats[0] {
            return 0.0;
        }
        if lat <= self.lats[self.h - 1] {
            return (self.h - 1) as f64;
        }
        let (mut lo, mut hi) = (0usize, self.h - 1);
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.lats[mid] >= lat {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let f = (self.lats[lo] - lat) / (self.lats[lo] - self.lats[hi]).abs().max(1e-12);
        lo as f64 + f
    }

    /// Bilinear-sample elevation (metres) at a `(lon, lat)`.
    fn elev_at(&self, lon: f64, lat: f64) -> f32 {
        let (cf, rf) = (self.colf(lon), self.rowf(lat));
        let (c0, r0) = (cf.floor() as usize, rf.floor() as usize);
        let (c1, r1) = ((c0 + 1).min(self.w - 1), (r0 + 1).min(self.h - 1));
        let (tx, ty) = ((cf - c0 as f64) as f32, (rf - r0 as f64) as f32);
        let e = |r: usize, c: usize| self.elev[r * self.w + c];
        let top = e(r0, c0) * (1.0 - tx) + e(r0, c1) * tx;
        let bot = e(r1, c0) * (1.0 - tx) + e(r1, c1) * tx;
        top * (1.0 - ty) + bot * ty
    }

    /// Bilinear-sample the raw satellite colour at a `(lon, lat)` (v2 only).
    fn rgb_at(&self, lon: f64, lat: f64) -> [u8; 3] {
        let (cf, rf) = (self.colf(lon), self.rowf(lat));
        let (c0, r0) = (cf.floor() as usize, rf.floor() as usize);
        let (c1, r1) = ((c0 + 1).min(self.w - 1), (r0 + 1).min(self.h - 1));
        let (tx, ty) = ((cf - c0 as f64) as f32, (rf - r0 as f64) as f32);
        let p = |r: usize, c: usize, k: usize| self.rgb[r * self.w + c][k] as f32;
        let mut out = [0u8; 3];
        for (k, o) in out.iter_mut().enumerate() {
            let top = p(r0, c0, k) * (1.0 - tx) + p(r0, c1, k) * tx;
            let bot = p(r1, c0, k) * (1.0 - tx) + p(r1, c1, k) * tx;
            *o = (top * (1.0 - ty) + bot * ty).round().clamp(0.0, 255.0) as u8;
        }
        out
    }
}

/// Topo-brown for the contour overlay — darker/more saturated than the default
/// [`GeoKind::Contour`](geo_hhtl::garmin::GeoKind::Contour) tan so the lines read
/// against the warm terrain (the OpenTopoMap contour look).
const CONTOUR_LINE: [u8; 3] = [128, 94, 60];

/// Palette slot for still-water lakes/tanks on an `--arid` scene — appended AFTER
/// the 9 canonical `GeoKind` entries (the ver-8 wire carries its own palette count,
/// so extra slots are decode-safe). On an arid landscape these are stock tanks and
/// ephemeral ponds, not lakes: bright blue misleads.
const LAKE_TAG: u8 = 9;

/// Barely-there grey-blue for [`LAKE_TAG`] cells. Deliberately BELOW the terrain
/// shader's blue-dominance `wet` threshold (`b − max(r,g) = 4/255 ≪ 0.06`), so none
/// of the vivid water treatment (blue re-assert, sun-glint) fires — the tank reads
/// as a subtle damp fleck in the earth, not a bright blue lake.
const LAKE_SUBTLE: [u8; 3] = [122, 130, 134];
