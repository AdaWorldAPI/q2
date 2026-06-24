// Turntable prerender — N frames of the body rotating 360°, parallel across all
// cores (x86-64-v4 build). Frames -> <out>/frame_NNNN.png. Default 270 (3s@90fps).
//
//   usage: turntable <parts_dir> <element_parts> <isa_inc> <out_dir> [mode] [nframes] [w] [h]

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufWriter;
use std::sync::atomic::{AtomicUsize, Ordering};

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn norm(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}
fn normalize(a: [f32; 3]) -> [f32; 3] {
    let n = norm(a);
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
    [[r[0], r[1], r[2], -dot(r, eye)], [u[0], u[1], u[2], -dot(u, eye)], [f[0], f[1], f[2], -dot(f, eye)], [0.0, 0.0, 0.0, 1.0]]
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
fn read_tree(path: &str) -> (HashMap<String, String>, HashMap<String, String>) {
    let txt = std::fs::read_to_string(path).unwrap_or_default();
    let (mut parent, mut name) = (HashMap::new(), HashMap::new());
    for (i, line) in txt.lines().enumerate() {
        if i == 0 {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() >= 4 {
            parent.insert(f[2].to_string(), f[0].to_string());
            name.insert(f[2].to_string(), f[3].to_string());
            name.entry(f[0].to_string()).or_insert_with(|| f[1].to_string());
        }
    }
    (parent, name)
}
const TISSUE: &[(&str, [f32; 3], &[&str])] = &[
    ("bone", [0.92, 0.88, 0.78], &["bone", "skeletal", "osseous", "vertebra", "sesamoid"]),
    ("cartilage", [0.62, 0.72, 0.85], &["cartilage", "chondral"]),
    ("ligament", [0.90, 0.90, 0.86], &["ligament"]),
    ("tendon", [0.88, 0.86, 0.80], &["tendon", "aponeurosis"]),
    ("muscle", [0.74, 0.36, 0.34], &["muscle", "musculature", "musculus"]),
    ("vessel", [0.80, 0.22, 0.22], &["artery", "arterial", "vein", "venous", "vascular", "capillary"]),
    ("nerve", [0.92, 0.82, 0.32], &["nerve", "neural", "ganglion", "plexus", "nervous"]),
    ("organ", [0.80, 0.58, 0.52], &["organ", "viscus", "gland"]),
    ("skin", [0.86, 0.66, 0.54], &["skin", "integument"]),
];

struct Tri {
    p: [[f32; 3]; 3],
    n: [[f32; 3]; 3],
    c: [f32; 3],
}

fn rasterize(tris: &[Tri], eye: [f32; 3], center: [f32; 3], w: u32, h: u32, focal: f32, light: [f32; 3]) -> Vec<u8> {
    let view = look_at(eye, center, [0.0, 0.0, 1.0]);
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let mut fb = vec![0.0f32; (3 * w * h) as usize];
    for i in (0..fb.len()).step_by(3) {
        fb[i] = 0.05;
        fb[i + 1] = 0.06;
        fb[i + 2] = 0.08;
    }
    let mut zbuf = vec![f32::MAX; (w * h) as usize];
    let edge = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| (c[0] - a[0]) * (b[1] - a[1]) - (c[1] - a[1]) * (b[0] - a[0]);
    for t in tris {
        let cs = [xform(&view, t.p[0]), xform(&view, t.p[1]), xform(&view, t.p[2])];
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
        let minx = s[0][0].min(s[1][0]).min(s[2][0]).floor().max(0.0) as i32;
        let maxx = s[0][0].max(s[1][0]).max(s[2][0]).ceil().min(w as f32 - 1.0) as i32;
        let miny = s[0][1].min(s[1][1]).min(s[2][1]).floor().max(0.0) as i32;
        let maxy = s[0][1].max(s[1][1]).max(s[2][1]).ceil().min(h as f32 - 1.0) as i32;
        for py in miny..=maxy {
            for px in minx..=maxx {
                let pc = [px as f32 + 0.5, py as f32 + 0.5];
                let mut w0 = edge(s[1], s[2], pc) * inv;
                let mut w1 = edge(s[2], s[0], pc) * inv;
                let mut w2 = edge(s[0], s[1], pc) * inv;
                if !((w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0) || (w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0)) {
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
                let nrm = normalize([w0 * t.n[0][0] + w1 * t.n[1][0] + w2 * t.n[2][0], w0 * t.n[0][1] + w1 * t.n[1][1] + w2 * t.n[2][1], w0 * t.n[0][2] + w1 * t.n[1][2] + w2 * t.n[2][2]]);
                let shade = 0.30 + 0.70 * dot(nrm, light).abs();
                for k in 0..3 {
                    fb[idx * 3 + k] = (t.c[k] * shade).clamp(0.0, 1.0);
                }
            }
        }
    }
    let mut rgb = vec![0u8; (3 * w * h) as usize];
    for y in 0..h as usize {
        let srow = (h as usize - 1 - y) * w as usize;
        let drow = y * w as usize;
        for x in 0..w as usize {
            for k in 0..3 {
                rgb[(drow + x) * 3 + k] = (fb[(srow + x) * 3 + k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
        }
    }
    rgb
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let parts_dir = a.get(1).cloned().unwrap_or_else(|| "data/isa_parts/isa_BP3D_4.0_obj_99".into());
    let elem_path = a.get(2).cloned().unwrap_or_else(|| "data/combined_element_parts.txt".into());
    let isa_path = a.get(3).cloned().unwrap_or_else(|| "data/isa_inclusion.txt".into());
    let out_dir = a.get(4).cloned().unwrap_or_else(|| "fma_frames".into());
    let mode = a.get(5).cloned().unwrap_or_else(|| "tissues".into());
    let nframes: usize = a.get(6).and_then(|s| s.parse().ok()).unwrap_or(270);
    let w: u32 = a.get(7).and_then(|s| s.parse().ok()).unwrap_or(600);
    let h: u32 = a.get(8).and_then(|s| s.parse().ok()).unwrap_or(800);

    let (isa_parent, isa_name) = read_tree(&isa_path);
    let tissue_of = |fma: &str| -> (&'static str, [f32; 3]) {
        let mut cur = fma.to_string();
        let mut seen = HashSet::new();
        for _ in 0..64 {
            if !seen.insert(cur.clone()) {
                break;
            }
            if let Some(nm) = isa_name.get(&cur) {
                let l = nm.to_lowercase();
                for (lab, col, kws) in TISSUE {
                    if kws.iter().any(|k| l.contains(k)) {
                        return (lab, *col);
                    }
                }
            }
            match isa_parent.get(&cur) {
                Some(p) => cur = p.clone(),
                None => break,
            }
        }
        ("other", [0.6, 0.6, 0.64])
    };
    let elem = std::fs::read_to_string(&elem_path).unwrap_or_default();
    let mut fj_fma: HashMap<String, Vec<String>> = HashMap::new();
    for (i, line) in elem.lines().enumerate() {
        if i == 0 {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() >= 3 {
            fj_fma.entry(f[2].into()).or_default().push(f[0].into());
        }
    }

    // Load geometry ONCE.
    let mut tris: Vec<Tri> = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&parts_dir).expect("parts").filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort();
    for path in &entries {
        if path.extension().and_then(|s| s.to_str()) != Some("obj") {
            continue;
        }
        let fj = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let (mut label, mut col) = ("other", [0.6, 0.6, 0.64]);
        if let Some(set) = fj_fma.get(&fj) {
            for c in set {
                let (l, cc) = tissue_of(c);
                if l != "other" {
                    label = l;
                    col = cc;
                    break;
                }
            }
        }
        let keep = match mode.as_str() {
            "bones" => label == "bone" || label == "cartilage",
            "tissues" => label != "skin" && label != "other",
            _ => true,
        };
        if !keep {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else { continue };
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
        for nrm in &mut vn {
            *nrm = normalize(*nrm);
        }
        for f in &faces {
            if f[0] >= verts.len() || f[1] >= verts.len() || f[2] >= verts.len() {
                continue;
            }
            tris.push(Tri { p: [verts[f[0]], verts[f[1]], verts[f[2]]], n: [vn[f[0]], vn[f[1]], vn[f[2]]], c: col });
        }
    }
    // normalize: feet z=0, center xy, 1.7 m.
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
    let rad = ((hi[0] - lo[0]).max(hi[1] - lo[1]) * 0.5 * sc).max(half_h * 0.6);
    let center = [0.0f32, 0.0, half_h];
    eprintln!("[turntable] {} triangles, {nframes} frames @ {w}x{h}, mode={mode}", tris.len());

    std::fs::create_dir_all(&out_dir).ok();
    let focal = w.max(h) as f32 * 1.05;
    let light = normalize([-0.35, -0.9, 0.55]);
    let dist = (half_h.max(rad)) * 3.4;
    let ncpu = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let done = AtomicUsize::new(0);
    let t0 = std::time::Instant::now();
    std::thread::scope(|s| {
        for tid in 0..ncpu {
            let (tris, done, out_dir) = (&tris, &done, &out_dir);
            s.spawn(move || {
                let mut f = tid;
                while f < nframes {
                    let az = f as f32 / nframes as f32 * std::f32::consts::TAU;
                    let eye = [center[0] + dist * az.sin(), center[1] - dist * az.cos(), center[2] + half_h * 0.12];
                    let rgb = rasterize(tris, eye, center, w, h, focal, light);
                    let file = format!("{out_dir}/frame_{f:04}.png");
                    let fh = File::create(&file).unwrap();
                    let mut enc = png::Encoder::new(BufWriter::new(fh), w, h);
                    enc.set_color(png::ColorType::Rgb);
                    enc.set_depth(png::BitDepth::Eight);
                    enc.write_header().unwrap().write_image_data(&rgb).unwrap();
                    let c = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if c % 30 == 0 {
                        eprintln!("  {c}/{nframes} frames ({:.1}s)", t0.elapsed().as_secs_f32());
                    }
                    f += ncpu;
                }
            });
        }
    });
    eprintln!("[turntable] {nframes} frames in {:.1}s on {ncpu} cores -> {out_dir}/", t0.elapsed().as_secs_f32());
}
