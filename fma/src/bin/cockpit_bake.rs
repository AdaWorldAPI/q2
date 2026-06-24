// cockpit_bake.rs — bake the full-body FMA mesh for MY cockpit page (/fma-body),
// additive to the other session's /torso (their torso.mesh is untouched).
//
// Emits an SPM1 indexed triangle mesh (the SAME wire the cockpit already decodes), but
// the per-vertex `opacity` byte carries a clean LAYER id (skin/muscle/organ/skeleton/
// vessel/nerve/…) instead of a continuous alpha — so the viewer can toggle each layer
// exactly with a button. Color is the converged `tissue` byte (is_a); geometry is the
// BodyParts3D is_a OBJ set, vertex-cluster decimated (the curve-ruler smoothing) the
// same way bake_torso_mesh.py does it.
//
// SPM1 (little-endian), byte-identical to bake_torso_mesh.py:
//   header 40 B: "SPM1" | vert_count u32 | tri_count u32 | node_count u32 | bbox_min 3f | bbox_max 3f
//   vertex 21 B: pos 3f | normal 3i8 | rgb 3u8 | opacity(=LAYER id) u8 | node_row u16
//   index  12 B: 3x u32
// Positions normalized to [-1,1]; orientation (x,-z,y) + i8-normal dequant happen in
// the renderer (FmaBody.tsx), same as torso.mesh.
//
//   usage: cockpit_bake <parts_dir> <element_parts> <converged.tsv> <out.mesh> [cell_mm]

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn normalize(a: [f32; 3]) -> [f32; 3] {
    let n = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    if n < 1e-9 { [0.0, 0.0, 1.0] } else { [a[0] / n, a[1] / n, a[2] / n] }
}
fn parse_obj(text: &str) -> (Vec<[f32; 3]>, Vec<[usize; 3]>) {
    let (mut verts, mut tris) = (Vec::new(), Vec::new());
    for line in text.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("v") => {
                let c: Vec<f32> = it.take(3).filter_map(|t| t.parse().ok()).collect();
                if c.len() == 3 {
                    verts.push([c[0], c[1], c[2]]);
                }
            }
            Some("f") => {
                let idx: Vec<usize> = it
                    .map(|t| t.split(['/', ' ']).next().unwrap_or(""))
                    .filter_map(|s| s.parse::<i32>().ok())
                    .map(|i| if i < 0 { (verts.len() as i32 + i) as usize } else { (i - 1) as usize })
                    .collect();
                for k in 1..idx.len().saturating_sub(1) {
                    tris.push([idx[0], idx[k], idx[k + 1]]);
                }
            }
            _ => {}
        }
    }
    (verts, tris)
}

type DecimatedMesh = (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[usize; 3]>);

// Vertex clustering on a global grid (cell = cell_mm): collapse verts in a cell to one
// representative (mean position + mean normal), remap faces, drop degenerates. Averaging
// the normals per cell keeps the surface smooth (port of bake_torso_mesh.py).
fn cluster_decimate(verts: &[[f32; 3]], normals: &[[f32; 3]], faces: &[[usize; 3]], inv_h: f32, o: [f32; 3]) -> DecimatedMesh {
    let mut cell_of: HashMap<(i32, i32, i32), usize> = HashMap::new();
    let mut acc: Vec<([f64; 6], u32)> = Vec::new();
    let mut remap = vec![0usize; verts.len()];
    for (i, v) in verts.iter().enumerate() {
        let key = (((v[0] - o[0]) * inv_h) as i32, ((v[1] - o[1]) * inv_h) as i32, ((v[2] - o[2]) * inv_h) as i32);
        let n = normals[i];
        let j = *cell_of.entry(key).or_insert_with(|| {
            acc.push(([0.0; 6], 0));
            acc.len() - 1
        });
        let a = &mut acc[j];
        a.0[0] += v[0] as f64;
        a.0[1] += v[1] as f64;
        a.0[2] += v[2] as f64;
        a.0[3] += n[0] as f64;
        a.0[4] += n[1] as f64;
        a.0[5] += n[2] as f64;
        a.1 += 1;
        remap[i] = j;
    }
    let (mut nv, mut nn) = (Vec::with_capacity(acc.len()), Vec::with_capacity(acc.len()));
    for (s, c) in &acc {
        let c = *c as f64;
        nv.push([(s[0] / c) as f32, (s[1] / c) as f32, (s[2] / c) as f32]);
        let nl = (s[3] * s[3] + s[4] * s[4] + s[5] * s[5]).sqrt().max(1.0);
        nn.push([(s[3] / nl) as f32, (s[4] / nl) as f32, (s[5] / nl) as f32]);
    }
    let mut nf = Vec::new();
    for f in faces {
        let (ra, rb, rc) = (remap[f[0]], remap[f[1]], remap[f[2]]);
        if ra != rb && rb != rc && ra != rc {
            nf.push([ra, rb, rc]);
        }
    }
    (nv, nn, nf)
}

// converged tissue (is_a low byte) → (layer id [opacity byte], layer name, rgb).
// The layer id is the exact gating key the viewer's buttons toggle.
fn layer_of(tissue: &str) -> (u8, &'static str, [u8; 3]) {
    match tissue {
        "skin" => (1, "skin", [219, 168, 138]),
        "muscle" => (2, "muscle", [189, 92, 87]),
        "organ" => (3, "organ", [204, 148, 132]),
        "bone" => (4, "skeleton", [235, 224, 199]),
        "cartilage" => (4, "skeleton", [159, 184, 217]),
        "vessel" => (5, "vessel", [204, 56, 56]),
        "nerve" => (6, "nerve", [235, 209, 82]),
        "ligament" | "tendon" | "fascia" => (7, "connective", [224, 219, 204]),
        _ => (8, "other", [150, 150, 160]),
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let parts_dir = a.get(1).cloned().unwrap_or_else(|| "data/isa_parts/isa_BP3D_4.0_obj_99".into());
    let elem_path = a.get(2).cloned().unwrap_or_else(|| "data/combined_element_parts.txt".into());
    let conv_path = a.get(3).cloned().unwrap_or_else(|| "guid/guid_converged.tsv".into());
    let out_path = a.get(4).cloned().unwrap_or_else(|| "cockpit/public/fma_body.mesh".into());
    let cell_mm: f32 = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(3.6);

    // converged key: FMA → (tissue, part_of depth, row, name).
    let mut fma_tissue: HashMap<String, String> = HashMap::new();
    let mut fma_depth: HashMap<String, usize> = HashMap::new();
    let mut fma_row: HashMap<String, u16> = HashMap::new();
    let mut fma_name: HashMap<String, String> = HashMap::new();
    for (i, line) in std::fs::read_to_string(&conv_path).unwrap_or_default().lines().enumerate() {
        if i == 0 {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() >= 7 {
            fma_row.insert(f[0].into(), (fma_tissue.len() & 0xFFFF) as u16);
            fma_tissue.insert(f[0].into(), f[4].into());
            fma_depth.insert(f[0].into(), f[6].matches(" / ").count());
            // name = the deepest segment of the part_of distinguished name.
            fma_name.insert(f[0].into(), f[6].rsplit(" / ").next().unwrap_or(f[0]).trim().to_string());
        }
    }
    let mut fj_fma: HashMap<String, Vec<String>> = HashMap::new();
    for (i, line) in std::fs::read_to_string(&elem_path).unwrap_or_default().lines().enumerate() {
        if i == 0 {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() >= 3 {
            fj_fma.entry(f[2].into()).or_default().push(f[0].into());
        }
    }

    // global accumulators
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nrm: Vec<[f32; 3]> = Vec::new();
    let mut col: Vec<[u8; 3]> = Vec::new();
    let mut lay: Vec<u8> = Vec::new();
    let mut row: Vec<u16> = Vec::new();
    let mut tris: Vec<[u32; 3]> = Vec::new();
    let mut layer_hist: HashMap<&str, usize> = HashMap::new();
    let inv_h = 1.0 / cell_mm;

    let mut entries: Vec<_> = std::fs::read_dir(&parts_dir).expect("parts").filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort();
    for path in &entries {
        if path.extension().and_then(|s| s.to_str()) != Some("obj") {
            continue;
        }
        let fj = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let Some(fma) = fj_fma.get(&fj).and_then(|v| v.iter().filter(|f| fma_tissue.contains_key(*f)).max_by_key(|f| fma_depth.get(*f).copied().unwrap_or(0))) else { continue };
        let tissue = fma_tissue[fma].as_str();
        let (layer_id, layer_name, rgb) = layer_of(tissue);
        let r = fma_row.get(fma).copied().unwrap_or(0);
        let Ok(text) = std::fs::read_to_string(path) else { continue };
        let (verts, faces) = parse_obj(&text);
        if verts.is_empty() || faces.is_empty() {
            continue;
        }
        // per-vertex normals from faces (smooth after clustering averages them)
        let mut vn = vec![[0.0f32; 3]; verts.len()];
        for f in &faces {
            if f[0] >= verts.len() || f[1] >= verts.len() || f[2] >= verts.len() {
                continue;
            }
            let fnv = cross(sub(verts[f[1]], verts[f[0]]), sub(verts[f[2]], verts[f[0]]));
            for &vi in f {
                for k in 0..3 {
                    vn[vi][k] += fnv[k];
                }
            }
        }
        for n in &mut vn {
            *n = normalize(*n);
        }
        let o = [
            verts.iter().map(|v| v[0]).fold(f32::MAX, f32::min),
            verts.iter().map(|v| v[1]).fold(f32::MAX, f32::min),
            verts.iter().map(|v| v[2]).fold(f32::MAX, f32::min),
        ];
        let (nv, nn, nf) = cluster_decimate(&verts, &vn, &faces, inv_h, o);
        let base = pos.len() as u32;
        for (v, n) in nv.iter().zip(nn.iter()) {
            pos.push(*v);
            nrm.push(*n);
            col.push(rgb);
            lay.push(layer_id);
            row.push(r);
        }
        for f in &nf {
            tris.push([base + f[0] as u32, base + f[1] as u32, base + f[2] as u32]);
        }
        *layer_hist.entry(layer_name).or_insert(0) += 1;
    }
    if pos.is_empty() {
        eprintln!("[cockpit_bake] no geometry — check parts_dir/element_parts/converged.tsv");
        return;
    }

    // normalize positions to [-1,1] (center + uniform scale), like bake_torso_mesh.py
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let c = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5, (lo[2] + hi[2]) * 0.5];
    let half = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(hi[2] - lo[2]) * 0.5;
    let inv = 1.0 / half.max(1e-6);
    for p in &mut pos {
        for (pk, ck) in p.iter_mut().zip(c.iter()) {
            *pk = (*pk - ck) * inv;
        }
    }
    let (mut bmin, mut bmax) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &pos {
        for k in 0..3 {
            bmin[k] = bmin[k].min(p[k]);
            bmax[k] = bmax[k].max(p[k]);
        }
    }

    // emit SPM1
    let qi8 = |v: f32| -> i8 { (v * 127.0).round().clamp(-127.0, 127.0) as i8 };
    let mut buf: Vec<u8> = Vec::with_capacity(40 + pos.len() * 21 + tris.len() * 12);
    buf.extend_from_slice(b"SPM1");
    buf.extend_from_slice(&(pos.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(tris.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(fma_tissue.len() as u32).to_le_bytes());
    for v in &bmin {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    for v in &bmax {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    for i in 0..pos.len() {
        for c in pos[i] {
            buf.extend_from_slice(&c.to_le_bytes());
        }
        buf.push(qi8(nrm[i][0]) as u8);
        buf.push(qi8(nrm[i][1]) as u8);
        buf.push(qi8(nrm[i][2]) as u8);
        buf.extend_from_slice(&col[i]);
        buf.push(lay[i]); // opacity byte = LAYER id
        buf.extend_from_slice(&row[i].to_le_bytes());
    }
    for t in &tris {
        for &x in t {
            buf.extend_from_slice(&x.to_le_bytes());
        }
    }
    if let Some(parent) = std::path::Path::new(&out_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    File::create(&out_path).unwrap().write_all(&buf).unwrap();

    // manifest (layer histogram drives the viewer's buttons)
    let manifest_path = format!("{out_path}.manifest.json");
    let mut layers: Vec<(&str, usize)> = layer_hist.into_iter().collect();
    layers.sort_by_key(|&(_, v)| std::cmp::Reverse(v));
    let layers_json: String = layers.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect::<Vec<_>>().join(",");
    let manifest = format!(
        "{{\"source\":\"BodyParts3D 4.0 (DBCLS) is_a OBJ, vertex-cluster decimated\",\"attribution\":\"BodyParts3D, (c) The Database Center for Life Science, CC-BY 4.0 / CC-BY-SA 2.1 JP\",\"format\":\"SPM1; opacity byte = LAYER id (1 skin·2 muscle·3 organ·4 skeleton·5 vessel·6 nerve·7 connective·8 other)\",\"verts\":{},\"tris\":{},\"cell_mm\":{cell_mm},\"layers\":{{{layers_json}}}}}",
        pos.len(),
        tris.len()
    );
    File::create(&manifest_path).unwrap().write_all(manifest.as_bytes()).unwrap();

    // search index: row → {fma, name, tissue} for the nodes present in the mesh. Drives the
    // /fma-body search bar; per-node centroids are computed client-side from `node_row`.
    let used: std::collections::HashSet<u16> = row.iter().copied().collect();
    let mut node_entries: Vec<(u16, &String, &String, &String)> = fma_row
        .iter()
        .filter(|(_, r)| used.contains(r))
        .filter_map(|(fma, r)| Some((*r, fma, fma_name.get(fma)?, fma_tissue.get(fma)?)))
        .collect();
    node_entries.sort_by_key(|&(r, ..)| r);
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let nodes_json: String = node_entries
        .iter()
        .map(|(r, fma, name, tissue)| format!("{{\"row\":{r},\"fma\":\"{}\",\"name\":\"{}\",\"tissue\":\"{}\"}}", esc(fma), esc(name), esc(tissue)))
        .collect::<Vec<_>>()
        .join(",");
    let nodes_path = format!("{out_path}.nodes.json");
    File::create(&nodes_path).unwrap().write_all(format!("{{\"nodes\":[{nodes_json}]}}").as_bytes()).unwrap();

    eprintln!("[cockpit_bake] {} verts, {} tris -> {out_path} ({} MB) + manifest + {} search nodes", pos.len(), tris.len(), buf.len() / 1_000_000, node_entries.len());
    eprintln!("[cockpit_bake] opacity byte = LAYER id; layers: {layers:?}");
}
