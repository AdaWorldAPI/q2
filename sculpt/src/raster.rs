//! Software rasterizer — a pinhole camera, a z-buffer, Gouraud shading, PNG out.
//! No WebGL: the browser only ever shows the `<img>` this produces.
//!
//! The load-bearing invariant is that [`pick`] is the EXACT algebraic inverse of
//! [`render`]'s projection. Both derive from one orthonormal basis
//! (`right`/`up`/`fwd`) stored in [`RenderOut`], so a screen pixel + its depth-
//! buffer value round-trips back to the world surface point the brush needs.
//! The projection is a textbook pinhole:
//!
//! ```text
//! rel = p - eye
//! vx = rel·right   vy = rel·up   vz = rel·fwd     (vz > 0 == in front)
//! sx = cx + focal·vx/vz          sy = cy − focal·vy/vz
//! ```
//!
//! and [`pick`] inverts it with the SAME `cx, cy, focal, basis`:
//! `vx = (sx−cx)·vz/focal`, `vy = −(sy−cy)·vz/focal`, `p = eye + vx·right + vy·up + vz·fwd`.

use crate::mesh::Mesh;

const BG: [u8; 3] = [0x0c, 0x0f, 0x14]; // the cockpit dark

pub struct Camera {
    pub yaw: f32,
    pub pitch: f32,
    pub dist: f32,
    pub focal: f32,
}

impl Default for Camera {
    fn default() -> Self {
        // focal is set per-render from height; a sentinel <= 0 means "derive it".
        Camera {
            yaw: 0.6,
            pitch: 0.35,
            dist: 3.0,
            focal: 0.0,
        }
    }
}

pub struct RenderOut {
    pub png: Vec<u8>,
    pub depth: Vec<f32>, // view-space z per pixel; f32::MAX == background
    pub w: u32,
    pub h: u32,
    pub eye: [f32; 3],
    pub right: [f32; 3],
    pub up: [f32; 3],
    pub fwd: [f32; 3],
    pub focal: f32,
}

/// The orbit camera basis: eye orbits the ORIGIN (the mesh is unit-normalized),
/// world up is +Y. Returned as `(eye, right, up, fwd)`, all unit, right-handed.
fn basis(cam: &Camera) -> ([f32; 3], [f32; 3], [f32; 3], [f32; 3]) {
    let (cp, sp) = (cam.pitch.cos(), cam.pitch.sin());
    let (cy, sy) = (cam.yaw.cos(), cam.yaw.sin());
    let eye = [cam.dist * cp * sy, cam.dist * sp, cam.dist * cp * cy];
    let fwd = normalize([-eye[0], -eye[1], -eye[2]]); // look at origin
    let mut right = cross(fwd, [0.0, 1.0, 0.0]);
    if dot(right, right) < 1e-12 {
        right = [1.0, 0.0, 0.0]; // looking straight up/down: pick any right
    }
    let right = normalize(right);
    let up = cross(right, fwd); // already unit (right ⟂ fwd, both unit)
    (eye, right, up, fwd)
}

/// Perspective project + z-buffer fill + per-vertex (Gouraud) shading → PNG.
/// `focal <= 0` in the camera derives `focal = 1.2·h` (a ~45° vertical FOV).
pub fn render(mesh: &Mesh, cam: &Camera, w: u32, h: u32) -> RenderOut {
    let w = w.clamp(1, 1600);
    let h = h.clamp(1, 1600);
    let focal = if cam.focal > 0.0 {
        cam.focal
    } else {
        1.2 * h as f32
    };
    let (eye, right, up, fwd) = basis(cam);
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);

    // Per-vertex shade → rgb (Gouraud): the light rig is headlight + one fill
    // from the upper-left + ambient. Colors are the sculpt paint (mesh.col).
    let fill = normalize([-0.5, 0.8, 0.6]);
    let shade = |n: [f32; 3]| -> f32 {
        let head = dot(n, [-fwd[0], -fwd[1], -fwd[2]]).max(0.0); // light from the eye
        let f = dot(n, fill).max(0.0);
        0.12 + 0.75 * head + 0.35 * f
    };
    let vcol: Vec<[f32; 3]> = mesh
        .col
        .iter()
        .zip(&mesh.nrm)
        .map(|(c, n)| {
            let s = shade(*n);
            [c[0] as f32 * s, c[1] as f32 * s, c[2] as f32 * s]
        })
        .collect();

    // Project every vertex once: (screen x, screen y, view depth vz).
    let proj: Vec<[f32; 3]> = mesh
        .pos
        .iter()
        .map(|p| {
            let rel = [p[0] - eye[0], p[1] - eye[1], p[2] - eye[2]];
            let vz = dot(rel, fwd);
            if vz <= 1e-4 {
                return [f32::NAN, f32::NAN, vz]; // behind/at the pinhole → skip
            }
            let vx = dot(rel, right);
            let vy = dot(rel, up);
            [cx + focal * vx / vz, cy - focal * vy / vz, vz]
        })
        .collect();

    let npix = (w * h) as usize;
    let mut depth = vec![f32::MAX; npix];
    let mut rgb = vec![BG; npix];

    for t in &mesh.tris {
        let (i0, i1, i2) = (t[0] as usize, t[1] as usize, t[2] as usize);
        let (a, b, c) = (proj[i0], proj[i1], proj[i2]);
        if a[2] <= 1e-4 || b[2] <= 1e-4 || c[2] <= 1e-4 {
            continue; // any vertex behind the pinhole → drop the whole tri
        }
        // Signed screen area; skip back-faces (CCW-from-outside → negative here
        // because screen-y is flipped) and degenerate slivers.
        let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
        if area >= -1e-7 {
            continue;
        }
        let inv_area = 1.0 / area;

        let minx = a[0].min(b[0]).min(c[0]).floor().max(0.0) as i32;
        let maxx = a[0].max(b[0]).max(c[0]).ceil().min(w as f32 - 1.0) as i32;
        let miny = a[1].min(b[1]).min(c[1]).floor().max(0.0) as i32;
        let maxy = a[1].max(b[1]).max(c[1]).ceil().min(h as f32 - 1.0) as i32;
        if minx > maxx || miny > maxy {
            continue;
        }
        let (ca, cb, cc) = (vcol[i0], vcol[i1], vcol[i2]);

        for py in miny..=maxy {
            for px in minx..=maxx {
                let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                // Barycentrics via edge functions (same winding as `area`).
                let w0 = ((b[0] - fx) * (c[1] - fy) - (b[1] - fy) * (c[0] - fx)) * inv_area;
                let w1 = ((c[0] - fx) * (a[1] - fy) - (c[1] - fy) * (a[0] - fx)) * inv_area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let vz = w0 * a[2] + w1 * b[2] + w2 * c[2];
                let idx = (py as u32 * w + px as u32) as usize;
                if vz >= depth[idx] {
                    continue;
                }
                depth[idx] = vz;
                // Depth cue: mesh radius ≈ 1, eye at `dist`, so vz ∈ ~[dist−1, dist+1];
                // fade the far half by up to 10%.
                let cue = 1.0 - 0.10 * (((vz - (cam.dist - 1.0)) * 0.5).clamp(0.0, 1.0));
                let r = (w0 * ca[0] + w1 * cb[0] + w2 * cc[0]) * cue;
                let g = (w0 * ca[1] + w1 * cb[1] + w2 * cc[1]) * cue;
                let bl = (w0 * ca[2] + w1 * cb[2] + w2 * cc[2]) * cue;
                rgb[idx] = [clamp8(r), clamp8(g), clamp8(bl)];
            }
        }
    }

    RenderOut {
        png: encode_png(&rgb, w, h),
        depth,
        w,
        h,
        eye,
        right,
        up,
        fwd,
        focal,
    }
}

/// Unproject a screen pixel through the stored basis + its depth-buffer value →
/// the world surface point. `None` on background. Exact inverse of `render`.
pub fn pick(ro: &RenderOut, x: f32, y: f32) -> Option<[f32; 3]> {
    let (px, py) = (x.floor(), y.floor());
    if px < 0.0 || py < 0.0 || px >= ro.w as f32 || py >= ro.h as f32 {
        return None;
    }
    let vz = ro.depth[(py as u32 * ro.w + px as u32) as usize];
    if vz == f32::MAX {
        return None; // background
    }
    // Sample at the pixel CENTER — render used (px+0.5) when it wrote depth here.
    let (cx, cy) = (ro.w as f32 * 0.5, ro.h as f32 * 0.5);
    let sx = px + 0.5;
    let sy = py + 0.5;
    let vx = (sx - cx) * vz / ro.focal;
    let vy = -(sy - cy) * vz / ro.focal;
    Some([
        ro.eye[0] + vx * ro.right[0] + vy * ro.up[0] + vz * ro.fwd[0],
        ro.eye[1] + vx * ro.right[1] + vy * ro.up[1] + vz * ro.fwd[1],
        ro.eye[2] + vx * ro.right[2] + vy * ro.up[2] + vz * ro.fwd[2],
    ])
}

/// Screen drag (pixels) → world vector in the view plane at `at`'s depth, so a
/// Grab tracks the cursor 1:1 on the surface it grabbed. Screen-y is down, hence
/// the `-up`.
pub fn drag_world(ro: &RenderOut, at: [f32; 3], dx: f32, dy: f32) -> [f32; 3] {
    let rel = [at[0] - ro.eye[0], at[1] - ro.eye[1], at[2] - ro.eye[2]];
    let vz = dot(rel, ro.fwd).max(1e-4);
    let k = vz / ro.focal;
    [
        (ro.right[0] * dx - ro.up[0] * dy) * k,
        (ro.right[1] * dx - ro.up[1] * dy) * k,
        (ro.right[2] * dx - ro.up[2] * dy) * k,
    ]
}

fn encode_png(rgb: &[[u8; 3]], w: u32, h: u32) -> Vec<u8> {
    let mut flat = Vec::with_capacity(rgb.len() * 3);
    for p in rgb {
        flat.extend_from_slice(p);
    }
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .and_then(|mut wr| wr.write_image_data(&flat))
            .expect("png encode of an in-memory RGB buffer cannot fail");
    }
    out
}

fn clamp8(v: f32) -> u8 {
    v.clamp(0.0, 255.0) as u8
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
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = dot(v, v).sqrt();
    if l > 0.0 {
        [v[0] / l, v[1] / l, v[2] / l]
    } else {
        [0.0, 0.0, 1.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::weld;
    use crate::stl::icosphere;

    fn sphere() -> Mesh {
        let mut m = weld(&icosphere(3));
        m.normalize_unit();
        m
    }

    #[test]
    fn project_pick_round_trips_on_surface() {
        let m = sphere();
        let cam = Camera::default();
        let ro = render(&m, &cam, 500, 400);
        // For every non-background pixel we sample, pick() must land on the unit
        // sphere (|p| ≈ 1) — the projection inverse is exact up to the ½-pixel
        // barycentric sample, which on a unit sphere is well under 2%.
        let mut checked = 0;
        for py in (0..ro.h).step_by(23) {
            for px in (0..ro.w).step_by(23) {
                if let Some(p) = pick(&ro, px as f32, py as f32) {
                    let r = dot(p, p).sqrt();
                    assert!((r - 1.0).abs() < 0.02, "pick off-surface: |p|={r}");
                    checked += 1;
                }
            }
        }
        assert!(checked > 25, "too few surface pixels hit ({checked})");
    }

    #[test]
    fn drag_world_scales_with_depth() {
        let m = sphere();
        let ro = render(&m, &Camera::default(), 400, 400);
        // A drag of `focal` pixels at depth vz spans exactly vz world units.
        let at = [
            ro.eye[0] + ro.fwd[0] * 2.0,
            ro.eye[1] + ro.fwd[1] * 2.0,
            ro.eye[2] + ro.fwd[2] * 2.0,
        ];
        let d = drag_world(&ro, at, ro.focal, 0.0);
        let len = dot(d, d).sqrt();
        assert!((len - 2.0).abs() < 1e-3, "drag length {len} != depth 2.0");
    }

    #[test]
    fn background_pick_is_none() {
        let m = sphere();
        let ro = render(&m, &Camera::default(), 300, 300);
        // The very corner pixel is background for a centered unit sphere.
        assert!(pick(&ro, 0.0, 0.0).is_none());
    }

    #[test]
    fn front_pixel_brighter_than_silhouette() {
        // The center pixel faces the camera (headlight full); an edge pixel is
        // near-silhouette. Decode luminance straight from the depth+shade by
        // re-rendering a white sphere and comparing center vs a rim column.
        let mut m = sphere();
        for c in &mut m.col {
            *c = [255, 255, 255];
        }
        let ro = render(&m, &Camera::default(), 401, 401);
        // Find the rim: scan a row for the last surface pixel before background.
        let row = 200u32;
        let mut rim = None;
        for px in (0..ro.w).rev() {
            if ro.depth[(row * ro.w + px) as usize] != f32::MAX {
                rim = Some(px);
                break;
            }
        }
        let center = decode_lum(&ro, 200, 200);
        let rim_lum = decode_lum(&ro, rim.unwrap(), row);
        assert!(
            center > rim_lum,
            "center {center} not brighter than rim {rim_lum}"
        );
    }

    // Pull luminance back out of the PNG-free path: re-derive from depth is not
    // possible, so decode the encoded PNG's pixel via a tiny reader.
    fn decode_lum(ro: &RenderOut, x: u32, y: u32) -> u32 {
        let dec = png::Decoder::new(std::io::Cursor::new(&ro.png));
        let mut reader = dec.read_info().unwrap();
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).unwrap();
        let i = (y * info.width + x) as usize * 3;
        buf[i] as u32 + buf[i + 1] as u32 + buf[i + 2] as u32
    }
}
