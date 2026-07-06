//! /sculpt — server-rendered STL sculpting. Dep-free std HTTP/1.1 (the fma
//! server pattern); every mutation invalidates a single cached [`RenderOut`],
//! which `/view.png` lazily re-renders. `/stroke` picks the surface against
//! that render's depth buffer, so the brush lands exactly where the cursor is.
//!
//!   GET  /                 the FieldView page
//!   GET  /view.png         current render (900×700)
//!   POST /stroke?x&y&dx&dy  pick at (x,y); Grab uses (dx,dy) as the drag
//!   POST /camera?dyaw&dpitch&ddist   orbit / dolly
//!   POST /brush?tool|radius|strength|color|detail   set a brush field
//!   POST /undo             pop the last stroke
//!   POST /reset?model=sphere|cube    load a built-in model
//!   GET  /model.stl        download the sculpted mesh (binary STL)
//!   POST /model.stl        upload an STL (raw body, ≤ 32 MB) to sculpt

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::time::Duration;

use askama::Template;
use sculpt::mesh::{weld, Mesh};
use sculpt::raster::{drag_world, pick, render, Camera, RenderOut};
use sculpt::sculpt::{apply, revert, Stroke, Tool, Undo};
use sculpt::stl::{icosphere, printer_cube, read_stl, write_binary_stl};
use sculpt::view::{BrushState, SculptPage};

const W: u32 = 900;
const H: u32 = 700;
const UNDO_CAP: usize = 64;
const UPLOAD_CAP: usize = 32 << 20;

struct App {
    mesh: Mesh,
    cam: Camera,
    brush: BrushState,
    undo: Vec<Undo>,
    model: String,
    ro: Option<RenderOut>,
}

impl App {
    fn load(model: &str) -> App {
        let soup = match model {
            "cube" => printer_cube(1.0, 0.18),
            _ => icosphere(4),
        };
        let mut mesh = weld(&soup);
        mesh.normalize_unit();
        App {
            mesh,
            cam: Camera::default(),
            brush: BrushState::default(),
            undo: Vec::new(),
            model: if model == "cube" {
                "cube".into()
            } else {
                "sphere".into()
            },
            ro: None,
        }
    }

    fn adopt(&mut self, mesh: Mesh, name: String) {
        self.mesh = mesh;
        self.undo.clear();
        self.model = name;
        self.ro = None;
    }

    /// Borrow the current render, producing it on demand. Positions/camera
    /// mutations set `ro = None`, so this always reflects live state.
    fn render_ref(&mut self) -> &RenderOut {
        if self.ro.is_none() {
            self.ro = Some(render(&self.mesh, &self.cam, W, H));
        }
        self.ro.as_ref().unwrap()
    }

    fn dirty(&mut self) {
        self.ro = None;
    }

    fn push_undo(&mut self, u: Undo) {
        if u.verts.is_empty() {
            return;
        }
        self.undo.push(u);
        if self.undo.len() > UNDO_CAP {
            self.undo.remove(0);
        }
    }
}

fn main() {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8090);
    let app = Mutex::new(App::load("sphere"));
    let listener = TcpListener::bind(("0.0.0.0", port)).expect("bind");
    eprintln!("[/sculpt] http://0.0.0.0:{port}/  (grab · inflate · smooth · spray · ruler)");
    for s in listener.incoming().flatten() {
        let _ = handle(s, &app);
    }
}

struct Req {
    method: String,
    path: String,
    query: HashMap<String, String>,
    body: Vec<u8>,
}

fn handle(mut s: TcpStream, app: &Mutex<App>) -> std::io::Result<()> {
    // Sequential accept loop: a stalled client must not wedge every other one.
    let _ = s.set_read_timeout(Some(Duration::from_secs(20)));
    let _ = s.set_write_timeout(Some(Duration::from_secs(20)));
    let Some(req) = read_request(&mut s)? else {
        return Ok(());
    };
    let mut a = app.lock().unwrap();
    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") => {
            let page = SculptPage::new(
                &a.brush,
                a.mesh.vertex_count(),
                a.mesh.tri_count(),
                model_name(&a.model),
            );
            let html = page
                .render()
                .unwrap_or_else(|e| format!("template error: {e}"));
            respond(
                &mut s,
                "200 OK",
                "text/html; charset=utf-8",
                html.as_bytes(),
            )
        }
        ("GET", "/view.png") => {
            let png = a.render_ref().png.clone();
            respond(&mut s, "200 OK", "image/png", &png)
        }
        ("POST", "/stroke") => {
            do_stroke(&mut a, &req.query);
            let png = a.render_ref().png.clone();
            respond(&mut s, "200 OK", "image/png", &png)
        }
        ("POST", "/camera") => {
            move_camera(&mut a.cam, &req.query);
            a.dirty();
            respond(&mut s, "200 OK", "text/plain", b"ok")
        }
        ("POST", "/brush") => {
            set_brush(&mut a.brush, &req.query);
            respond(&mut s, "200 OK", "text/plain", b"ok")
        }
        ("POST", "/undo") => {
            if let Some(u) = a.undo.pop() {
                revert(&mut a.mesh, &u);
                a.dirty();
            }
            respond(&mut s, "200 OK", "text/plain", b"ok")
        }
        ("POST", "/reset") => {
            let model = req
                .query
                .get("model")
                .map(String::as_str)
                .unwrap_or("sphere");
            *a = App::load(model);
            respond(&mut s, "200 OK", "text/plain", b"ok")
        }
        ("GET", "/model.stl") => {
            // Export at source scale — an uploaded 100 mm part comes back 100 mm,
            // not shrunk into the unit display box (mesh.denorm inverts the fit).
            let stl = write_binary_stl(&a.mesh.export_positions(), &a.mesh.tris);
            respond_attach(&mut s, "sculpt.stl", &stl)
        }
        ("POST", "/model.stl") | ("PUT", "/model.stl") => match read_stl(&req.body) {
            Ok(soup) => {
                let mut mesh = weld(&soup);
                mesh.normalize_unit();
                a.adopt(mesh, "upload".into());
                respond(&mut s, "200 OK", "text/plain", b"loaded")
            }
            Err(e) => respond(&mut s, "400 Bad Request", "text/plain", e.as_bytes()),
        },
        _ => respond(&mut s, "404 Not Found", "text/plain", b"not found"),
    }
}

/// Pick the surface at (x,y); apply the current brush there. Grab reads the
/// per-move drag; the other tools need only the center. Background picks are
/// no-ops (you were dragging off the model).
fn do_stroke(a: &mut App, q: &HashMap<String, String>) {
    let x = fget(q, "x", -1.0);
    let y = fget(q, "y", -1.0);
    let dx = fget(q, "dx", 0.0);
    let dy = fget(q, "dy", 0.0);
    let ro = a.render_ref();
    let Some(center) = pick(ro, x, y) else { return };
    let dir = drag_world(ro, center, dx, dy);
    let b = &a.brush;
    let stroke = Stroke {
        tool: b.tool,
        center,
        dir,
        radius: b.radius,
        strength: b.strength,
        color: b.color,
        detail: b.detail,
    };
    let u = apply(&mut a.mesh, &stroke);
    a.push_undo(u);
    a.dirty();
}

fn move_camera(cam: &mut Camera, q: &HashMap<String, String>) {
    cam.yaw += fget(q, "dyaw", 0.0);
    cam.pitch = (cam.pitch + fget(q, "dpitch", 0.0)).clamp(-1.4, 1.4);
    cam.dist = (cam.dist + fget(q, "ddist", 0.0)).clamp(1.2, 8.0);
}

fn set_brush(b: &mut BrushState, q: &HashMap<String, String>) {
    if let Some(t) = q.get("tool").and_then(|s| s.parse::<Tool>().ok()) {
        b.tool = t;
    }
    if let Some(v) = q.get("radius").and_then(|s| s.parse().ok()) {
        b.radius = v;
    }
    if let Some(v) = q.get("strength").and_then(|s| s.parse().ok()) {
        b.strength = v;
    }
    if let Some(v) = q.get("detail").and_then(|s| s.parse().ok()) {
        b.detail = v;
    }
    if let Some(c) = q.get("color").and_then(|s| parse_hex(s)) {
        b.color = c;
    }
}

fn model_name(m: &str) -> &str {
    match m {
        "cube" => "cube",
        "upload" => "upload",
        _ => "sphere",
    }
}

// ── tiny HTTP/1.1 ────────────────────────────────────────────────────────────

fn read_request(s: &mut TcpStream) -> std::io::Result<Option<Req>> {
    // Read until the header terminator, then Content-Length bytes of body.
    let mut buf = Vec::with_capacity(1024);
    let mut tmp = [0u8; 4096];
    // Linear scan: only search newly-read bytes each round (rewound 3 so a
    // `\r\n\r\n` split across two reads is still caught), so a terminator-less
    // stream costs O(n), not O(n²).
    let mut scanned = 0usize;
    let hdr_end = loop {
        let n = s.read(&mut tmp)?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.len() > UPLOAD_CAP + 8192 {
            return Ok(None); // runaway header/body
        }
        let start = scanned.saturating_sub(3);
        if let Some(p) = find_crlfcrlf(&buf[start..]) {
            break start + p;
        }
        scanned = buf.len();
    };
    let head = String::from_utf8_lossy(&buf[..hdr_end]).to_string();
    let mut lines = head.split("\r\n");
    let mut parts = lines.next().unwrap_or("").split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    let mut clen = 0usize;
    for l in lines {
        if let Some(v) = l
            .strip_prefix("Content-Length:")
            .or_else(|| l.strip_prefix("content-length:"))
        {
            clen = v.trim().parse().unwrap_or(0).min(UPLOAD_CAP);
        }
    }
    let (path, query) = split_query(&target);
    let body_start = hdr_end + 4;
    let mut body = buf[body_start..].to_vec();
    while body.len() < clen {
        let n = s.read(&mut tmp)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(clen);
    Ok(Some(Req {
        method,
        path,
        query,
        body,
    }))
}

fn find_crlfcrlf(b: &[u8]) -> Option<usize> {
    b.windows(4).position(|w| w == b"\r\n\r\n")
}

fn split_query(target: &str) -> (String, HashMap<String, String>) {
    let mut q = HashMap::new();
    match target.split_once('?') {
        None => (target.to_string(), q),
        Some((path, qs)) => {
            for pair in qs.split('&') {
                if let Some((k, v)) = pair.split_once('=') {
                    q.insert(k.to_string(), url_decode(v));
                }
            }
            (path.to_string(), q)
        }
    }
}

/// Minimal percent + '+' decode. Works entirely on bytes: a `%XY` escape is
/// decoded from the raw hex pair (so a multi-byte UTF-8 sequence like `%C3%A9`
/// reassembles correctly), and the result is lossy-UTF-8'd once at the end.
/// Never slices `s` at a byte offset, so a `%` before a multibyte char can't
/// hit a non-char boundary and panic.
fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                match std::str::from_utf8(&b[i + 1..i + 3])
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                {
                    Some(byte) => {
                        out.push(byte);
                        i += 2;
                    }
                    None => out.push(b'%'), // malformed escape → literal '%'
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    Some([
        u8::from_str_radix(&s[0..2], 16).ok()?,
        u8::from_str_radix(&s[2..4], 16).ok()?,
        u8::from_str_radix(&s[4..6], 16).ok()?,
    ])
}

fn fget(q: &HashMap<String, String>, k: &str, d: f32) -> f32 {
    q.get(k).and_then(|s| s.parse().ok()).unwrap_or(d)
}

fn respond(s: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    s.write_all(head.as_bytes())?;
    s.write_all(body)?;
    s.flush()
}

fn respond_attach(s: &mut TcpStream, filename: &str, body: &[u8]) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: model/stl\r\nContent-Disposition: attachment; filename=\"{filename}\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    s.write_all(head.as_bytes())?;
    s.write_all(body)?;
    s.flush()
}

#[cfg(test)]
mod tests {
    use super::url_decode;

    #[test]
    fn url_decode_basics() {
        assert_eq!(url_decode("a+b%20c"), "a b c");
        assert_eq!(url_decode("%23fff"), "#fff");
        assert_eq!(url_decode("-0.5"), "-0.5");
    }

    #[test]
    fn url_decode_multibyte_escape_reassembles() {
        // Two percent-escaped bytes form one UTF-8 char, decoded as bytes.
        assert_eq!(url_decode("%C3%A9"), "é");
    }

    #[test]
    fn url_decode_percent_before_multibyte_does_not_panic() {
        // The old `&s[i+1..i+3]` sliced a str by byte offset and panicked when a
        // '%' sat before a multibyte char. Byte-level decode keeps the '%' literal.
        assert_eq!(url_decode("%é"), "%é");
        assert_eq!(url_decode("100%"), "100%");
        assert_eq!(url_decode("%"), "%");
    }
}
