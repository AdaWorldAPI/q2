//! `osm_bake` — read a Geofabrik `.osm.pbf` extract, extrude its building
//! footprints, and bake them into an SPM1 mesh keyed by HHTL tile GUIDs.
//!
//! The geo counterpart of `fma/src/bin/cockpit_bake.rs`: where that bakes the
//! anatomy body from BodyParts3D OBJ, this bakes a city from OSM `way`s — same
//! SPM1 wire out, so the cockpit's `FmaBody.tsx` decoder renders it unchanged.
//!
//! ```text
//! usage: osm_bake <in.osm.pbf> <out.mesh> [zoom]
//! ```
//!
//! Two passes over the extract: pass 1 collects every node's `(lon, lat)`; pass
//! 2 resolves each `building=*` way's node refs into a ground-plane ring,
//! projects it to a local metric frame, and extrudes by `building:height`
//! (or `building:levels × 3 m`, default 6 m). The region is projected
//! equirectangularly about its centre — good enough for a city block render.

use std::collections::HashMap;
use std::io::BufWriter;
use std::path::Path;

use geo_hhtl::{bake, point_to_hhtl, Footprint, Layer};
use osmpbf::{Element, ElementReader};

/// Metres per degree of latitude (WGS84 mean) — the equirectangular scale.
const M_PER_DEG: f64 = 111_320.0;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: osm_bake <in.osm.pbf> <out.mesh> [zoom]");
        std::process::exit(2);
    };
    let zoom: u32 = args.next().and_then(|z| z.parse().ok()).unwrap_or(16);

    match run(&input, &output, zoom) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("osm_bake: {e}");
            std::process::exit(1);
        }
    }
}

/// One OSM building way, before projection: its node ref ring + parsed height.
struct RawWay {
    refs: Vec<i64>,
    height: f32,
}

fn run(input: &str, output: &str, zoom: u32) -> Result<(), Box<dyn std::error::Error>> {
    // ── Pass 1: every node id → (lon, lat). ──
    let mut nodes: HashMap<i64, (f64, f64)> = HashMap::new();
    ElementReader::from_path(input)?.for_each(|el| match el {
        Element::Node(n) => {
            nodes.insert(n.id(), (n.lon(), n.lat()));
        }
        Element::DenseNode(n) => {
            nodes.insert(n.id(), (n.lon(), n.lat()));
        }
        _ => {}
    })?;
    eprintln!("pass 1: {} nodes indexed", nodes.len());

    // ── Pass 2: every building=* way → its node ref ring + height. ──
    let mut ways: Vec<RawWay> = Vec::new();
    ElementReader::from_path(input)?.for_each(|el| {
        if let Element::Way(w) = el {
            let mut is_building = false;
            let mut height: Option<f32> = None;
            let mut levels: Option<f32> = None;
            for (k, v) in w.tags() {
                match k {
                    "building" if v != "no" => is_building = true,
                    "height" | "building:height" => height = parse_leading_f32(v),
                    "building:levels" => levels = parse_leading_f32(v),
                    _ => {}
                }
            }
            if is_building {
                let refs: Vec<i64> = w.refs().collect();
                if refs.len() >= 4 {
                    // height tag > levels × 3 m > 6 m default (≈ two storeys).
                    let h = height.or_else(|| levels.map(|l| l * 3.0)).unwrap_or(6.0);
                    ways.push(RawWay { refs, height: h });
                }
            }
        }
    })?;
    eprintln!("pass 2: {} building ways", ways.len());
    if ways.is_empty() {
        return Err("no building ways found in extract".into());
    }

    // ── Region centre (projection origin) = mean of all building-node coords. ──
    let (mut sum_lon, mut sum_lat, mut cnt) = (0.0f64, 0.0f64, 0u64);
    for way in &ways {
        for id in &way.refs {
            if let Some(&(lon, lat)) = nodes.get(id) {
                sum_lon += lon;
                sum_lat += lat;
                cnt += 1;
            }
        }
    }
    if cnt == 0 {
        return Err("building ways reference no indexed nodes".into());
    }
    if ways.len() > usize::from(u16::MAX) {
        eprintln!(
            "warning: {} ways exceed SPM1 node_row's u16 range — rows >65535 \
             saturate to 65535 (use osm_helix / BSO2 for a u32 feature id)",
            ways.len()
        );
    }
    let (lon0, lat0) = (sum_lon / cnt as f64, sum_lat / cnt as f64);
    let cos_lat0 = lat0.to_radians().cos();
    // Equirectangular projection of (lon, lat) → local ground-plane metres.
    let project = |lon: f64, lat: f64| -> [f32; 2] {
        let x = (lon - lon0) * cos_lat0 * M_PER_DEG;
        let z = (lat - lat0) * M_PER_DEG;
        [x as f32, z as f32]
    };

    // ── Build the (Footprint, lon, lat) list the lib's bake() consumes. ──
    let mut features: Vec<(Footprint, f64, f64)> = Vec::with_capacity(ways.len());
    for (row, way) in ways.iter().enumerate() {
        // Resolve refs → ring; OSM closes the way (first == last node), so drop
        // the repeated last point. Skip any way with an unresolved node.
        let mut ring = Vec::with_capacity(way.refs.len());
        let mut ok = true;
        let (mut clon, mut clat) = (0.0f64, 0.0f64);
        for id in &way.refs[..way.refs.len().saturating_sub(1)] {
            match nodes.get(id) {
                Some(&(lon, lat)) => {
                    ring.push(project(lon, lat));
                    clon += lon;
                    clat += lat;
                }
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok || ring.len() < 3 {
            continue;
        }
        let n = ring.len() as f64;
        let footprint = Footprint {
            ring,
            height: way.height,
            // SPM1 node_row is u16; saturate (don't wrap) past 65535 — a lossy
            // back-ref for the throwaway SPM1 path. BSO2 carries a u32 row.
            layer: Layer::Building,
            node_row: u16::try_from(row).unwrap_or(u16::MAX),
        };
        features.push((footprint, clon / n, clat / n));
    }

    if features.is_empty() {
        return Err("no building ways with resolvable geometry".into());
    }
    let (mesh, baked) = bake(&features, zoom);

    // Distinct HHTL keys = how many tiles the city spreads across at this zoom
    // — the addressing working on real data, not a synthetic point.
    let distinct: std::collections::HashSet<_> = baked
        .iter()
        .map(|b| (b.key.heel, b.key.hip, b.key.twig))
        .collect();
    let sample = &baked[0].key;
    eprintln!(
        "baked {} buildings → {} verts / {} tris ({} bytes SPM1), {} distinct HHTL tiles @ z{zoom}",
        baked.len(),
        mesh.verts.len(),
        mesh.tris.len(),
        mesh.spm1_len(),
        distinct.len(),
    );
    eprintln!(
        "sample key: classid 0x0F0? | HEEL {:04x} | HIP {:04x} | TWIG {:04x}  (also via point_to_hhtl: {:?})",
        sample.heel,
        sample.hip,
        sample.twig,
        point_to_hhtl(lon0, lat0, zoom),
    );

    let file = std::fs::File::create(Path::new(output))?;
    let mut w = BufWriter::new(file);
    mesh.write_spm1(&mut w)?;
    eprintln!("wrote {output}");
    Ok(())
}

/// Parse the leading floating-point number from an OSM measurement tag, tolerant
/// of a trailing unit (`"12 m"`, `"3.5"`, `"20;25"` → `12`, `3.5`, `20`).
fn parse_leading_f32(s: &str) -> Option<f32> {
    let t = s.trim();
    let end = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .unwrap_or(t.len());
    t[..end].parse().ok()
}
