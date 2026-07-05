//! `osm_helix` — read a `.osm.pbf`, bake its buildings into the `BSO2` `/helix`
//! wire (F16 pos + `helix_orient` normals + `NodeGuid` concepts) + a `.blocks`
//! LOD sidecar. The geo counterpart of `body-soa-wire`; output drops straight
//! into `cockpit/public/` + `body.manifest.json` for `BodyHelix.tsx` to render.
//!
//! ```text
//! usage: osm_helix <in.osm.pbf> <out.soa>   (writes <out>.soa + <out>.blocks)
//! ```
//! Gzip `<out>.soa` afterwards (BodyHelix fetches a gzip'd artifact).

use geo_hhtl::bso2::encode_bso2;
use geo_hhtl::osm_read::read_buildings;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: osm_helix <in.osm.pbf> <out.soa>");
        std::process::exit(2);
    };

    let scene = match read_buildings(&input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("osm_helix: {e}");
            std::process::exit(1);
        }
    };

    // Key zoom is fixed deep inside encode_bso2 (the 4-tier cascade); the arg is
    // (the 4-tier cascade uses a fixed deep KEY_ZOOM inside encode_bso2).
    let (soa, blocks) = encode_bso2(&scene);

    let nc = u32::from_le_bytes(soa[6..10].try_into().unwrap());
    let nv = u32::from_le_bytes(soa[10..14].try_into().unwrap());
    let nt = u32::from_le_bytes(soa[14..18].try_into().unwrap());
    eprintln!(
        "BSO2: {nc} concepts · {nv} verts · {nt} tris · {} B soa · {} B blocks",
        soa.len(),
        blocks.len()
    );

    let blocks_path = format!("{}.blocks", output.strip_suffix(".soa").unwrap_or(&output));
    if let Err(e) = std::fs::write(&output, &soa).and_then(|()| std::fs::write(&blocks_path, &blocks))
    {
        eprintln!("osm_helix: write: {e}");
        std::process::exit(1);
    }
    eprintln!("wrote {output} + {blocks_path}");
}
