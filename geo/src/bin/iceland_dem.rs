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

// THE KURVENLINEAL — the real golden-spiral curve-ruler (stride-4-over-17). Same crate the
// `sculpt` tool + the anatomy body use; ported here so the terrain carries the residue.
use helix::CurveRuler;

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
    lats: Vec<f64>,     // len h, row 0 = north
    elev: Vec<f32>,     // len w*h, row-major, row 0 = north, col 0 = west (metres)
    rgb: Vec<[u8; 3]>,  // len w*h (DEMG v2) or empty (v1) — ESRI imagery drape, same order as elev
}

fn read_demgrid(path: &str) -> Result<Dem, Box<dyn std::error::Error>> {
    let b = std::fs::read(path)?;
    // Need bytes 0..32 for the fixed header (magic/ver/w/h/west/east); `f64_at(24)` reads 24..32.
    if b.len() < 32 || &b[0..4] != b"DEMG" {
        return Err("not a DEMG file".into());
    }
    let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    let f64_at = |o: usize| f64::from_le_bytes(b[o..o + 8].try_into().unwrap());
    let ver = u32_at(4);
    if ver != 1 && ver != 2 {
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
    // DEMG v2: a per-cell RGB imagery drape (ESRI World Imagery) after the elevation block,
    // same row-major order. The baker classifies each cell's colour into a surface KIND.
    let mut rgb = Vec::new();
    if ver == 2 {
        if b.len() < o + n * 3 {
            return Err("DEMG v2 truncated: rgb block short".into());
        }
        rgb.reserve(n);
        for _ in 0..n {
            rgb.push([b[o], b[o + 1], b[o + 2]]);
            o += 3;
        }
    }
    Ok(Dem { w, h, west, east, lats, elev, rgb })
}

/// Surface KIND — the terrain node's CLASS axis (the "kind" that sits beside the HHTL position
/// address and the Helix residue). Classified from the ESRI imagery colour + elevation; each kind
/// carries a canonical palette so the map reads as clean ocean / green / rock / scree / ice / lava
/// rather than a muddy raw photo.
#[derive(Clone, Copy)]
enum Kind {
    Ocean,
    Green, // vegetation / moss / tundra
    Rock,  // brown highland rock
    Scree, // grey bare
    Ice,   // snow / glacier
    Lava,  // dark volcanic basalt / black sand
}

impl Kind {
    /// Canonical base colour (sRGB, 0..255) for the kind.
    fn color(self) -> [f32; 3] {
        match self {
            Kind::Ocean => [16.0, 42.0, 74.0],
            Kind::Green => [74.0, 108.0, 52.0],
            Kind::Rock => [116.0, 96.0, 74.0],
            Kind::Scree => [142.0, 140.0, 134.0],
            Kind::Ice => [234.0, 240.0, 248.0],
            Kind::Lava => [40.0, 28.0, 26.0],
        }
    }
}

/// Classify a vertex from its imagery colour + elevation. Elevation gates ocean (y == 0); the
/// imagery luminance/saturation separates ice (bright, low-sat), lava (very dark), green
/// (green-dominant), scree (bright grey) and rock (the mid-tone default).
fn classify_kind(rgb: [u8; 3], elev: f32) -> Kind {
    if elev <= 0.0 {
        return Kind::Ocean;
    }
    let (r, g, b) = (rgb[0] as f32, rgb[1] as f32, rgb[2] as f32);
    let lum = 0.299 * r + 0.587 * g + 0.114 * b;
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let sat = if mx > 0.0 { (mx - mn) / mx } else { 0.0 };
    if lum > 172.0 && sat < 0.18 {
        Kind::Ice // bright + desaturated → snow / glacier
    } else if lum < 60.0 {
        Kind::Lava // very dark → volcanic basalt / black sand
    } else if g > r * 1.04 && g > b * 1.10 && g > 58.0 {
        Kind::Green // green channel clearly dominant → vegetation / moss
    } else if lum > 128.0 && sat < 0.22 {
        Kind::Scree // bright-ish grey → scree / bare
    } else {
        Kind::Rock // mid-tone default → highland rock
    }
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
    // DISPLAY frame (Dx, Dy=up=elevation, Dz). No macro exaggeration here — the shader raises the
    // true-scale island by uExag; the Kurvenlineal golden-spiral residue is baked in below.
    let mut pos = vec![[0.0f32; 3]; w * h];
    for i in 0..w * h {
        pos[i] = [(mx[i] - cx) * inv, my[i] * inv, (mz[i] - cz) * inv];
    }

    let idx = |r: usize, c: usize| r * w + c;

    // Helix CurveRuler detail frequency — used for the within-KIND colour texture below (NOT the
    // geometry). At the full 16.5M-vert native resolution the heightfield already carries the fine
    // relief, so the golden-spiral residue is NOT displaced into the geometry: `ruler_phase` is a
    // per-cell STEP function, and displacing a dense mesh by it strips the surface into stepped
    // ridges. The residue instead rides the COLOUR (within-kind variation), where a step reads as
    // texture, not spikes. (On the sparse/anatomy meshes the geometry displacement is fine — cells
    // are large relative to the mesh; on this dense grid it is not.)
    const RES_DETAIL: f32 = 48.0;
    let nrm = terrain_normals(&pos, w, h);

    // ── Per-vertex COLOUR = the KIND axis. Classify each vertex's surface class from the ESRI
    //    imagery colour + elevation, take the kind's canonical palette colour, then texture it by
    //    (a) the real imagery luminance (light/shadow within the kind) and (b) the Helix residue
    //    phase (deterministic within-kind variation from the address). Together with the HHTL
    //    address (per-concept key) and the Helix residue (displacement above), this completes the
    //    node's three axes: WHERE (HHTL) · SURFACE (Helix) · WHAT (kind). No imagery → a neutral
    //    moss default so a v1 demgrid still bakes. ──
    let has_img = !dem.rgb.is_empty();
    let mut colors: Vec<[u8; 3]> = Vec::with_capacity(w * h);
    for i in 0..w * h {
        let px = if has_img { dem.rgb[i] } else { [96, 110, 92] };
        let base = classify_kind(px, my[i]).color();
        let lum = (0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32) / 255.0;
        let lmod = 0.62 + 0.55 * lum; // imagery luminance → light/shadow within the kind
        let rmod = 1.0 + 0.10 * ruler_phase(pos[i], RES_DETAIL); // helix residue → within-kind texture
        let q = |c: f32| (c * lmod * rmod).round().clamp(0.0, 255.0) as u8;
        colors.push([q(base[0]), q(base[1]), q(base[2])]);
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
    let (soa, blocks) = encode_mesh_bso2(&pos, &nrm, &rows, &tris, &concepts, labels, &colors);

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

/// Per-vertex up-facing gradient normals for a `w×h` heightfield in the display frame.
/// `du ≈ +east` (col+), `dv ≈ +north` (row−, since row 0 is north); `cross(dv, du)` points
/// +Dy on flat ground and tilts with the slope. One-sided at the edges. Called twice: once for
/// the residue displacement direction, once to re-derive shading from the displaced surface.
fn terrain_normals(pos: &[[f32; 3]], w: usize, h: usize) -> Vec<[f32; 3]> {
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
    nrm
}

// ── THE KURVENLINEAL (ported verbatim from `sculpt::sculpt` — the same helix::CurveRuler the
//    anatomy body uses). Deterministic golden-spiral relief; the phase is regenerated from the
//    address, NEVER stored. ──
const MIX_X: u64 = 0x9E37_79B9_7F4A_7C15;
const MIX_Y: u64 = 0xC2B2_AE3D_27D4_EB4F;
const MIX_Z: u64 = 0x1656_67B1_9E37_79F9;

/// Decorrelate a lattice cell into a u64 place anchor (wrapping_mul + xor fold).
fn mix(c: [i64; 3]) -> u64 {
    (c[0] as u64).wrapping_mul(MIX_X)
        ^ (c[1] as u64).wrapping_mul(MIX_Y)
        ^ (c[2] as u64).wrapping_mul(MIX_Z)
}

/// Deterministic bipolar relief in [-1, 1] from a vertex's lattice address. `cell =
/// floor(pos·detail)` → `place = mix(cell)`; a finer 3× sub-lattice picks `k = mix(sub) mod 17`;
/// value = `(CurveRuler::from_place(place).index(k) / 16)·2 − 1`. Same `pos` + `detail` → same
/// value on every bake, every session (phase is convention, not data — OGAR D-QUANTGATE).
fn ruler_phase(pos: [f32; 3], detail: f32) -> f32 {
    let cell = [
        (pos[0] * detail).floor() as i64,
        (pos[1] * detail).floor() as i64,
        (pos[2] * detail).floor() as i64,
    ];
    let sub = [
        (pos[0] * detail * 3.0).floor() as i64,
        (pos[1] * detail * 3.0).floor() as i64,
        (pos[2] * detail * 3.0).floor() as i64,
    ];
    let k = (mix(sub) % 17) as u32;
    let idx = CurveRuler::from_place(mix(cell)).index(k);
    // index ∈ [0,17) → idx/16 ∈ [0,1] → bipolar [-1,1] with 8 → 0.
    (idx as f32 / 16.0) * 2.0 - 1.0
}
