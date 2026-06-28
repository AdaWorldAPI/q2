//! Body SoA-wire builder (option-2 rebake, stage 2). Reads bake_body_soa.py's
//! columns + concept table, mints the address GUID (classid 0x1000 + (part_of:is_a)
//! 8:8 cascade + identity), encodes the 2 helices via ndarray helix_orient
//! (helix-pos = dir from origin; helix-normal = self orientation), and writes one
//! BSO2 SoA wire. Native x86 → helix encode runs full F32x16 SIMD.
//!
//! BSO2 layout (LE): magic | ver u16 | n_concepts u32 | n_verts u32 | n_tris u32
//!   | concept GUID col [16·NC] | material u8 [NC] | label u32 [NC]
//!   | centroid 3f [NC] | (v_start,v_count) 2u32 [NC]
//!   | pos 3×F16 [NV] | helix 6B [NV] (pos3|nrm3) | row u32 [NV] | idx 3u32 [NT]
//!   | labels_json u32 len + bytes | materials_json u32 len + bytes
//!
//! Position precision history (per-vertex `pos`, all 6 B/vertex below ver 3):
//!   ver 3 = 3× f32 (12 B).
//!   ver 4 = 3× BF16 (7-bit mantissa). HALF the size, but BF16's step is ~2⁻⁸ of a
//!     coordinate's magnitude → ~3 mm near the head (y≈0.85) → a visible STAIRCASE
//!     (Treppeneffekt) on small smooth structures like the eye/brain. REJECTED for
//!     quality.
//!   ver 5 = 3× F16 / IEEE half (10-bit mantissa). SAME 6 B/vertex as BF16, but the
//!     step is ~2⁻¹¹ of magnitude → ~0.4 mm near the head, sub-visual — no staircase.
//!     Coords are all in [-1,1], well inside F16's range. The renderer widens back to
//!     f32 via a 64K half→f32 LUT. Conversion uses ndarray's tested `F16::from_f32`.
use lance_graph_contract::canonical_node::{classid_read_mode, NodeGuid};
use ndarray::hpc::quantized::F16;
use ndarray::hpc::splat3d::helix_orient;

const CLASSID_FMA: u32 = NodeGuid::CLASSID_FMA_V3; // 0x1000_0A01

fn tile(part_of: u8, is_a: u8) -> u16 { ((part_of as u16) << 8) | is_a as u16 }

// 8 compartment layers (skin/muscle/organ/skeleton/vessel/nerve/connective/other) —
// the toggle granularity, separate from the Doppler material. Maps the finer is_a
// tissue onto the same scheme /fma-body uses.
fn layer_of(tissue: &str) -> u8 {
    match tissue {
        "skin" | "flesh" => 1,
        "muscle" => 2,
        "heart" | "lung" | "liver" | "kidney" | "gi" | "gland" | "viscus" => 3,
        "bone" | "cartilage" => 4,
        "artery" | "vein" | "vessel" => 5,
        "nerve" => 6,
        _ => 8,
    }
}

fn read_f32s(path: &str) -> Vec<f32> {
    let b = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}
fn read_u32s(path: &str) -> Vec<u32> {
    let b = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    b.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "soa".into());
    let out = std::env::args().nth(2).unwrap_or_else(|| "body.soa".into());
    let j: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{dir}/body.concepts.json")).unwrap()).unwrap();
    let concepts = j["concepts"].as_array().unwrap();
    let nc = concepts.len();
    let nv = j["verts"].as_u64().unwrap() as usize;
    let nt = j["tris"].as_u64().unwrap() as usize;

    let tail = classid_read_mode(CLASSID_FMA).tail_variant;
    let arr6 = |v: &serde_json::Value, k: &str| -> [u8; 6] {
        let a = v[k].as_array().unwrap();
        std::array::from_fn(|i| a.get(i).and_then(|x| x.as_u64()).unwrap_or(0) as u8)
    };

    // ── per-concept columns: address GUID, material, label, centroid, (v_start,v_count) ──
    let mut guid = Vec::with_capacity(nc * 16);
    let mut material = Vec::with_capacity(nc);
    let mut layer = Vec::with_capacity(nc);
    let mut label = Vec::with_capacity(nc * 4);
    let mut centroid = Vec::with_capacity(nc * 12);
    let mut vrange = Vec::with_capacity(nc * 8);
    for c in concepts {
        let row = c["row"].as_u64().unwrap() as u32;
        let po = arr6(c, "part_of");
        let ia = arr6(c, "is_a");
        // tiers 0..4 = (part_of:is_a) 8:8; identity (tier 5) = row → unique key
        let key = NodeGuid::mint_for(
            tail, CLASSID_FMA,
            tile(po[0], ia[0]), tile(po[1], ia[1]), tile(po[2], ia[2]), tile(po[3], ia[3]),
            u32::from(tile(po[4], ia[4])), row,
        );
        guid.extend_from_slice(key.as_bytes());
        material.push(c["material"].as_u64().unwrap_or(4) as u8);
        layer.push(layer_of(c["tissue"].as_str().unwrap_or("")));
        label.extend_from_slice(&(c["name_idx"].as_u64().unwrap_or(0) as u32).to_le_bytes());
        let cen = c["centroid"].as_array().unwrap();
        for k in 0..3 { centroid.extend_from_slice(&(cen[k].as_f64().unwrap() as f32).to_le_bytes()); }
        vrange.extend_from_slice(&(c["v_start"].as_u64().unwrap() as u32).to_le_bytes());
        vrange.extend_from_slice(&(c["v_count"].as_u64().unwrap() as u32).to_le_bytes());
    }

    // ── per-vertex columns: pos (kept) + helix (2× 3-byte) ──
    let pos = read_f32s(&format!("{dir}/body.pos"));
    let nrm = read_f32s(&format!("{dir}/body.nrm"));
    let row = read_u32s(&format!("{dir}/body.row"));
    let idx = std::fs::read(format!("{dir}/body.idx")).unwrap();
    assert!(pos.len() >= nv * 3 && nrm.len() >= nv * 3);
    let mut helix = Vec::with_capacity(nv * 6);
    let mut max_norm_err = 0.0f64;
    for i in 0..nv {
        let p = [pos[i * 3], pos[i * 3 + 1], pos[i * 3 + 2]];
        let n = [nrm[i * 3], nrm[i * 3 + 1], nrm[i * 3 + 2]];
        // helix-pos: direction from origin (0,0,0) to the vertex (depth = |p|, derived by trig)
        let pm = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-6);
        let dir = [p[0] / pm, p[1] / pm, p[2] / pm];
        helix.extend_from_slice(&helix_orient::encode(dir, 3));
        // helix-normal: self orientation (compressed normal — replaces the f32 nrm column)
        helix.extend_from_slice(&helix_orient::encode(n, 3));
        if i % 97 == 0 { max_norm_err = max_norm_err.max(helix_orient::angle_error_deg(n, 3)); }
    }

    // ── per-concept BlockBounds (centroid + radius) for server-side HHTL LOD ──
    // Recomputed from the `row` column (covers original + vessel-fill verts), so
    // the tiny `{out}.blocks` asset (nc × 16 B) the cockpit-server embeds matches
    // exactly what /api/body/lod's `cascade_blocks` runs over. center3 f32 | radius f32.
    let mut bsum = vec![[0f64; 3]; nc];
    let mut bcnt = vec![0u32; nc];
    for i in 0..nv {
        let r = row[i] as usize;
        if r < nc { bsum[r][0] += pos[i*3] as f64; bsum[r][1] += pos[i*3+1] as f64; bsum[r][2] += pos[i*3+2] as f64; bcnt[r] += 1; }
    }
    let bcen: Vec<[f32; 3]> = (0..nc).map(|r| if bcnt[r] > 0 {
        [(bsum[r][0]/bcnt[r] as f64) as f32, (bsum[r][1]/bcnt[r] as f64) as f32, (bsum[r][2]/bcnt[r] as f64) as f32]
    } else { [0.0; 3] }).collect();
    let mut brad = vec![0f32; nc];
    for i in 0..nv {
        let r = row[i] as usize;
        if r < nc {
            let c = bcen[r];
            let d = (pos[i*3]-c[0]).powi(2) + (pos[i*3+1]-c[1]).powi(2) + (pos[i*3+2]-c[2]).powi(2);
            if d > brad[r] { brad[r] = d; }
        }
    }
    // emit centroids in the RENDERER's display space (the BodyV3 remap (x,y,z)->(-x,z,y)),
    // so the client posts its three.js camera directly — block space == rendered space.
    // radius is reflection/permutation-invariant (max distance), so it is unchanged.
    let mut blocks = Vec::with_capacity(nc * 16);
    for r in 0..nc {
        let c = bcen[r];
        for v in [-c[0], c[2], c[1]] { blocks.extend_from_slice(&v.to_le_bytes()); }
        blocks.extend_from_slice(&brad[r].sqrt().max(1e-4).to_le_bytes());
    }
    let blocks_out = format!("{}.blocks", out.strip_suffix(".soa").unwrap_or(&out));
    std::fs::write(&blocks_out, &blocks).unwrap();

    // ── assemble BSO2 ──
    let mut o = Vec::with_capacity(nc * 40 + nv * 22 + idx.len() + 64);
    o.extend_from_slice(b"BSO2");
    o.extend_from_slice(&5u16.to_le_bytes()); // ver 5 = F16 (IEEE half) positions
    o.extend_from_slice(&(nc as u32).to_le_bytes());
    o.extend_from_slice(&(nv as u32).to_le_bytes());
    o.extend_from_slice(&(nt as u32).to_le_bytes());
    o.extend_from_slice(&guid); o.extend_from_slice(&material); o.extend_from_slice(&layer);
    o.extend_from_slice(&label); o.extend_from_slice(&centroid); o.extend_from_slice(&vrange);
    // pos → F16 / IEEE half (10-bit mantissa; ndarray's tested F16::from_f32 RNE)
    let pos_f16: Vec<u8> = pos[..nv * 3].iter().flat_map(|&f| F16::from_f32(f).0.to_le_bytes()).collect();
    o.extend_from_slice(&pos_f16);
    o.extend_from_slice(&helix);
    o.extend_from_slice(&row[..nv].iter().flat_map(|r| r.to_le_bytes()).collect::<Vec<u8>>());
    o.extend_from_slice(&idx[..nt * 12]);
    for f in ["body.labels.json", "body.materials.json"] {
        let blob = std::fs::read(format!("{dir}/{f}")).unwrap();
        o.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        o.extend_from_slice(&blob);
    }
    std::fs::write(&out, &o).unwrap();

    // verify GUID[0] decodes as V3 (classid 0x1000, 8:8 part_of:is_a tiers, unique identity)
    let le16 = |o: usize| u16::from_le_bytes(guid[o..o + 2].try_into().unwrap());
    println!("── BSO2 ──");
    println!("  concepts {nc} · verts {nv} · tris {nt}");
    println!("  address GUID[0]: classid={:#010x} heel(po:isa)={:#06x} hip={:#06x} identity={}",
        u32::from_le_bytes(guid[0..4].try_into().unwrap()), le16(4), le16(6), le16(14));
    println!("  helix: 2× 3-byte/vertex ({} B col); max normal angle err ~{:.3}°", helix.len(), max_norm_err);
    println!("  pos: F16 (ver 5) — {} B col (was {} B as f32); 10-bit mantissa, no staircase", nv * 6, nv * 12);
    println!("  blocks: wrote {blocks_out} ({} B = {nc}×16 BlockBounds)", blocks.len());
    println!("  wrote {out} ({} B)", o.len());
}
