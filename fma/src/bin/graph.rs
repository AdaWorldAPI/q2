// graph.rs — SLICE 3: the renderer reads the key, as SOLID TRIANGLES.
//
// Renders the filled triangle surface (the same z-buffered Gouraud rasterizer as
// `mesh`), but the render is driven by the converged canonical key:
//   * COLOR  — each mesh is colored by its FMA's `tissue` byte (the is_a low byte of
//              the converged GUID), read straight from `converge`'s manifest.
//   * SELECT — an optional GUID **prefix** picks which triangles to draw: pass
//              `0a010000-0901-0702` and only the parts whose canonical key starts with
//              it render (one part_of/is_a subtree). Prefix-routing the key, made
//              visible — the address selects the geometry. `all` draws the whole body.
//              (Prefix example uses the post-2026-07-02-flip classid form — canon
//              0x0A01 in the HIGH half; `converge.rs` mints this order.)
//
// This is "70k nodes connecting via relationships → 16M triangles": the relationships
// (part_of/is_a) ARE the address, and the address drives which triangles light up.
//
//   usage: graph <parts_dir> <element_parts> <converged.tsv> <out_dir> [all|<tissue>|<guid-prefix>]

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufWriter;

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn normalize(a: [f32; 3]) -> [f32; 3] {
    let n = dot(a, a).sqrt();
    if n < 1e-9 {
        [0.0, 0.0, 1.0]
    } else {
        [a[0] / n, a[1] / n, a[2] / n]
    }
}
fn look_at(eye: [f32; 3], target: [f32; 3], up: [f32; 3]) -> [[f32; 4]; 4] {
    let f = normalize(sub(target, eye));
    let r = normalize(cross(up, f));
    let u = cross(f, r);
    [
        [r[0], r[1], r[2], -dot(r, eye)],
        [u[0], u[1], u[2], -dot(u, eye)],
        [f[0], f[1], f[2], -dot(f, eye)],
        [0.0, 0.0, 0.0, 1.0],
    ]
}
fn xform(m: &[[f32; 4]; 4], p: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2] + m[0][3],
        m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2] + m[1][3],
        m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2] + m[2][3],
    ]
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
                    .map(|i| {
                        if i < 0 {
                            (verts.len() as i32 + i) as usize
                        } else {
                            (i - 1) as usize
                        }
                    })
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

// tissue label → solid color (same palette as mesh/converge — the is_a low byte).
fn tissue_color(t: &str) -> [f32; 3] {
    match t {
        "bone" => [0.92, 0.88, 0.78],
        "cartilage" => [0.62, 0.72, 0.85],
        "ligament" => [0.90, 0.90, 0.86],
        "tendon" => [0.88, 0.86, 0.80],
        "muscle" => [0.74, 0.36, 0.34],
        "vessel" => [0.80, 0.22, 0.22],
        "nerve" => [0.92, 0.82, 0.32],
        "organ" => [0.80, 0.58, 0.52],
        "skin" => [0.86, 0.66, 0.54],
        _ => [0.60, 0.60, 0.64],
    }
}

struct Tri {
    p: [[f32; 3]; 3],
    n: [[f32; 3]; 3],
    c: [f32; 3],
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let parts_dir = a
        .get(1)
        .cloned()
        .unwrap_or_else(|| "data/isa_parts/isa_BP3D_4.0_obj_99".into());
    let elem_path = a
        .get(2)
        .cloned()
        .unwrap_or_else(|| "data/combined_element_parts.txt".into());
    let conv_path = a
        .get(3)
        .cloned()
        .unwrap_or_else(|| "guid/guid_converged.tsv".into());
    let out_dir = a.get(4).cloned().unwrap_or_else(|| "graph".into());
    let sel = a.get(5).cloned().unwrap_or_else(|| "all".into());

    // ── read the converged key: FMA → (guid, tissue, part_of depth) ──
    let mut fma_guid: HashMap<String, String> = HashMap::new();
    let mut fma_tissue: HashMap<String, String> = HashMap::new();
    let mut fma_depth: HashMap<String, usize> = HashMap::new();
    for (i, line) in std::fs::read_to_string(&conv_path)
        .unwrap_or_default()
        .lines()
        .enumerate()
    {
        if i == 0 {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() >= 7 {
            fma_guid.insert(f[0].into(), f[1].into());
            fma_tissue.insert(f[0].into(), f[4].into());
            fma_depth.insert(f[0].into(), f[6].matches(" / ").count()); // part_of_dn depth
        }
    }
    if fma_guid.is_empty() {
        eprintln!("[graph] no converged manifest at {conv_path} — run `converge` first");
        return;
    }
    // FJ → its FMAs (deepest-first preference handled by manifest membership).
    let mut fj_fma: HashMap<String, Vec<String>> = HashMap::new();
    for (i, line) in std::fs::read_to_string(&elem_path)
        .unwrap_or_default()
        .lines()
        .enumerate()
    {
        if i == 0 {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() >= 3 {
            fj_fma.entry(f[2].into()).or_default().push(f[0].into());
        }
    }

    // `sel` ∈ { all | tissues | <tissue> | <guid-prefix> }. Hex/dash ⇒ prefix.
    let is_prefix = !matches!(sel.as_str(), "all" | "tissues")
        && sel.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    let keep = |guid: &str, tissue: &str| -> bool {
        match sel.as_str() {
            "all" => true,
            "tissues" => tissue != "skin" && tissue != "other",
            _ if is_prefix => guid.starts_with(&sel),
            _ => tissue == sel,
        }
    };

    // ── collect solid triangles, colored by the key's tissue, selected by the key ──
    let mut tris: Vec<Tri> = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&parts_dir)
        .expect("parts")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();
    let mut kept: HashSet<&str> = HashSet::new();
    for path in &entries {
        if path.extension().and_then(|s| s.to_str()) != Some("obj") {
            continue;
        }
        let fj = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        // pick the FJ's DEEPEST addressed FMA (most specific, like guid.rs) so a bone
        // mesh keys to its skeletal leaf, not a soft-tissue parent. Skip unaddressed.
        let Some(fma) = fj_fma.get(&fj).and_then(|v| {
            v.iter()
                .filter(|f| fma_guid.contains_key(*f))
                .max_by_key(|f| fma_depth.get(*f).copied().unwrap_or(0))
        }) else {
            continue;
        };
        let (guid, tissue) = (&fma_guid[fma], fma_tissue[fma].as_str());
        if !keep(guid, tissue) {
            continue;
        }
        let col = tissue_color(tissue);
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let (verts, faces) = parse_obj(&text);
        if verts.is_empty() {
            continue;
        }
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
        for f in &faces {
            if f[0] >= verts.len() || f[1] >= verts.len() || f[2] >= verts.len() {
                continue;
            }
            tris.push(Tri {
                p: [verts[f[0]], verts[f[1]], verts[f[2]]],
                n: [vn[f[0]], vn[f[1]], vn[f[2]]],
                c: col,
            });
        }
        kept.insert(tissue_color_name(tissue));
    }
    eprintln!(
        "[graph] select='{sel}' ({}): {} triangles from addressed meshes",
        if is_prefix {
            "guid-prefix"
        } else if sel == "all" {
            "whole body"
        } else {
            "tissue"
        },
        tris.len()
    );
    if tris.is_empty() {
        eprintln!("[graph] nothing matched '{sel}' — try `all`, a tissue (bone/vessel/...), or a guid prefix like 0a010000-0901");
        return;
    }

    // ── normalize + camera + rasterize (the mesh.rs solid path) ──
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for t in &tris {
        for v in &t.p {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
    }
    let sc = 1.7 / (hi[2] - lo[2]).max(1e-3);
    let (mx, my) = ((lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5);
    for t in &mut tris {
        for v in &mut t.p {
            *v = [(v[0] - mx) * sc, (v[1] - my) * sc, (v[2] - lo[2]) * sc];
        }
    }
    let half_h = (hi[2] - lo[2]) * 0.5 * sc;
    let center = [0.0f32, 0.0, half_h];
    let (w, h) = (980u32, 1280u32);
    let dist = half_h * 4.2;
    let eye = [
        center[0] + dist * 0.42,
        center[1] - dist * 0.85,
        center[2] + half_h * 0.12,
    ];
    let view = look_at(eye, center, [0.0, 0.0, 1.0]);
    let focal = w.max(h) as f32 * 1.1;
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let light = normalize([-0.35, -0.9, 0.55]);

    let mut fb = vec![0.0f32; (3 * w * h) as usize];
    for i in (0..fb.len()).step_by(3) {
        fb[i] = 0.05;
        fb[i + 1] = 0.06;
        fb[i + 2] = 0.08;
    }
    let mut zbuf = vec![f32::MAX; (w * h) as usize];
    let edge = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
        (c[0] - a[0]) * (b[1] - a[1]) - (c[1] - a[1]) * (b[0] - a[0])
    };
    for t in &tris {
        let cs: [[f32; 3]; 3] = [
            xform(&view, t.p[0]),
            xform(&view, t.p[1]),
            xform(&view, t.p[2]),
        ];
        if cs[0][2] <= 0.02 || cs[1][2] <= 0.02 || cs[2][2] <= 0.02 {
            continue;
        }
        let proj = |c: [f32; 3]| [focal * c[0] / c[2] + cx, focal * c[1] / c[2] + cy];
        let s = [proj(cs[0]), proj(cs[1]), proj(cs[2])];
        let area = edge(s[0], s[1], s[2]);
        if area.abs() < 1e-6 {
            continue;
        }
        let inv = 1.0 / area;
        let (minx, maxx) = (
            s[0][0].min(s[1][0]).min(s[2][0]).floor().max(0.0) as i32,
            s[0][0].max(s[1][0]).max(s[2][0]).ceil().min(w as f32 - 1.0) as i32,
        );
        let (miny, maxy) = (
            s[0][1].min(s[1][1]).min(s[2][1]).floor().max(0.0) as i32,
            s[0][1].max(s[1][1]).max(s[2][1]).ceil().min(h as f32 - 1.0) as i32,
        );
        for py in miny..=maxy {
            for px in minx..=maxx {
                let pc = [px as f32 + 0.5, py as f32 + 0.5];
                let mut w0 = edge(s[1], s[2], pc) * inv;
                let mut w1 = edge(s[2], s[0], pc) * inv;
                let mut w2 = edge(s[0], s[1], pc) * inv;
                if !((w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0) || (w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0))
                {
                    continue;
                }
                if area < 0.0 {
                    w0 = -w0;
                    w1 = -w1;
                    w2 = -w2;
                }
                let depth = w0 * cs[0][2] + w1 * cs[1][2] + w2 * cs[2][2];
                let idx = (py as u32 * w + px as u32) as usize;
                if depth >= zbuf[idx] {
                    continue;
                }
                zbuf[idx] = depth;
                let mut nrm = [
                    w0 * t.n[0][0] + w1 * t.n[1][0] + w2 * t.n[2][0],
                    w0 * t.n[0][1] + w1 * t.n[1][1] + w2 * t.n[2][1],
                    w0 * t.n[0][2] + w1 * t.n[1][2] + w2 * t.n[2][2],
                ];
                nrm = normalize(nrm);
                let shade = 0.30 + 0.70 * dot(nrm, light).abs();
                for k in 0..3 {
                    fb[idx * 3 + k] = (t.c[k] * shade).clamp(0.0, 1.0);
                }
            }
        }
    }

    std::fs::create_dir_all(&out_dir).ok();
    let mut rgb = vec![0u8; (3 * w * h) as usize];
    for y in 0..h as usize {
        let (srow, drow) = ((h as usize - 1 - y) * w as usize, y * w as usize);
        for x in 0..w as usize {
            for k in 0..3 {
                rgb[(drow + x) * 3 + k] =
                    (fb[(srow + x) * 3 + k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        }
    }
    let tag = sel.replace('-', "");
    let file = format!("{out_dir}/graph_{tag}.png");
    let mut enc = png::Encoder::new(BufWriter::new(File::create(&file).unwrap()), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&rgb).unwrap();
    eprintln!("[graph] solid surface, colored by `tissue` (is_a) + selected by the canonical key -> {file} ({w}x{h})");
}

// identity helper so the kept-set stays &'static (label set is fixed).
fn tissue_color_name(t: &str) -> &'static str {
    match t {
        "bone" => "bone",
        "cartilage" => "cartilage",
        "ligament" => "ligament",
        "tendon" => "tendon",
        "muscle" => "muscle",
        "vessel" => "vessel",
        "nerve" => "nerve",
        "organ" => "organ",
        "skin" => "skin",
        _ => "other",
    }
}
