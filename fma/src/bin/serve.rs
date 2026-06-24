// /FMA — serves the FMA-addressed 3D body under one route surface.
//   GET /FMA                 viewer (renders + tissue legend + GUID lookup)
//   GET /FMA/body.png        full tissue body (triangle raster)
//   GET /FMA/skeleton.png    solid skeleton
//   GET /FMA/manifest        GUID manifest (TSV)
//   GET /FMA/guid/<FMAID>    {container, guid, distinguished_name}  (JSON)
//   GET /FMA/turntable       360° turntable, 90 fps autoplay (LazyLock-prebuffered)
//   GET /FMA/live            interactive drag-to-rotate over the same frames
//   GET /FMA/frame/<i>       one turntable frame (PNG, from the RAM prebuffer)
//   GET /FMA/info            {nframes, fps}  (JSON)
//
// One route surface, all under /FMA — no case-only /fma vs /FMA overlap.
// Dep-free std HTTP/1.1. Bind 0.0.0.0:$PORT (default 8088).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::LazyLock;

// LazyLock prebuffer — turntable frames loaded into RAM once, on first /FMA/turntable hit.
static FRAMES: LazyLock<Vec<Vec<u8>>> = LazyLock::new(|| {
    let mut v = Vec::new();
    let mut i = 0usize;
    while let Ok(b) = std::fs::read(format!("fma_frames/frame_{i:04}.png")) {
        v.push(b);
        i += 1;
    }
    eprintln!("[/FMA] LazyLock prebuffered {} frames ({} MB)", v.len(), v.iter().map(|f| f.len()).sum::<usize>() / 1_000_000);
    v
});

fn player_html(n: usize, live: bool) -> Vec<u8> {
    let tmpl = r##"<!doctype html><html><head><meta charset="utf-8"><title>__TITLE__</title>
<style>html,body{margin:0;height:100%;background:#0c0f14;overflow:hidden}
#c{display:block;margin:0 auto;height:100vh;cursor:__CURSOR__}
#h{position:fixed;top:12px;left:0;right:0;text-align:center;color:#8aaab8;font:14px system-ui;pointer-events:none}</style></head>
<body><div id="h">__HINT__</div><canvas id="c" width="600" height="800"></canvas>
<script>
const N=__N__, LIVE=__LIVE__;
const ctx=document.getElementById('c').getContext('2d');
const imgs=new Array(N); let loaded=0, f=0, last=0;
for(let i=0;i<N;i++){const im=new Image();im.onload=()=>{loaded++;if(loaded===1)draw();if(loaded===N&&!LIVE)auto();};im.src='/FMA/frame/'+i;imgs[i]=im;}
function idx(){return ((f%N)+N)%N;}
function draw(){const im=imgs[idx()];if(im&&im.complete)ctx.drawImage(im,0,0,600,800);}
function auto(){requestAnimationFrame(function loop(t){if(t-last>=1000/90){f++;draw();last=t;}requestAnimationFrame(loop);});}
if(LIVE){let drag=false,lx=0;const cv=document.getElementById('c');
 const dn=x=>{drag=true;lx=x;};const mv=x=>{if(drag){f=Math.round(f-(x-lx)*N/window.innerWidth);draw();lx=x;}};
 cv.onmousedown=e=>dn(e.clientX);window.onmouseup=()=>drag=false;window.onmousemove=e=>mv(e.clientX);
 cv.ontouchstart=e=>dn(e.touches[0].clientX);window.ontouchend=()=>drag=false;window.ontouchmove=e=>{mv(e.touches[0].clientX);e.preventDefault();};
 setInterval(()=>{if(!drag){f++;draw();}},1000/24);}
</script></body></html>"##;
    tmpl.replace("__TITLE__", if live { "/FMA/live — interactive" } else { "/FMA/turntable — 360 turntable" })
        .replace("__HINT__", if live { "drag to rotate (270 prerendered frames)" } else { "360 turntable - 90 fps - 270 frames" })
        .replace("__CURSOR__", if live { "grab" } else { "default" })
        .replace("__N__", &n.to_string())
        .replace("__LIVE__", if live { "true" } else { "false" })
        .into_bytes()
}

fn load_manifest(path: &str) -> HashMap<String, (String, String, String)> {
    let mut m = HashMap::new();
    let txt = std::fs::read_to_string(path).unwrap_or_default();
    for (i, line) in txt.lines().enumerate() {
        if i == 0 {
            continue;
        }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() >= 4 {
            // container(system) | identity(fma) | guid | distinguished_name
            m.insert(f[1].to_string(), (f[0].to_string(), f[2].to_string(), f[3].to_string()));
        }
    }
    m
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn index_html(n: usize) -> Vec<u8> {
    let body = format!(r##"<!doctype html><html><head><meta charset="utf-8">
<title>/FMA — Open 3D Body</title>
<style>
 body{{background:#0c0f14;color:#cdd6e0;font:15px/1.5 system-ui,sans-serif;margin:0;padding:24px;max-width:1100px;margin:auto}}
 h1{{font-weight:600}} a{{color:#6db3ff}} code{{color:#9fd0a0;background:#161b22;padding:1px 5px;border-radius:4px}}
 .row{{display:flex;gap:18px;flex-wrap:wrap}} .row img{{height:420px;background:#0a0d12;border-radius:8px}}
 .leg span{{display:inline-block;padding:2px 9px;margin:3px;border-radius:10px;color:#0c0f14;font-weight:600}}
 input{{background:#161b22;border:1px solid #2a313c;color:#cdd6e0;padding:7px;border-radius:6px;width:200px}}
 button{{background:#264;color:#cdeccd;border:0;padding:7px 12px;border-radius:6px;cursor:pointer}}
 pre{{background:#161b22;padding:12px;border-radius:8px;overflow:auto}}
</style></head><body>
<h1>/FMA — FMA-addressed 3D body</h1>
<p>{n} FMA nodes · canonical GUID <code>classid::HEEL::HIP::TWIG::F4::F5:IDENTITY</code>
 from part_of distinguished-name parent resolution · golden-stride (φ×γ) identity mint.
 Geometry = real mesh triangles (z-buffered, Gouraud), not splats.</p>
<div class="row">
 <figure><img src="/FMA/skeleton.png" alt="skeleton"><figcaption>skeleton · 602K triangles</figcaption></figure>
 <figure><img src="/FMA/body.png" alt="body"><figcaption>tissues · 6.2M triangles</figcaption></figure>
</div>
<p>animated: <a href="/FMA/turntable">/FMA/turntable</a> (90 fps autoplay) ·
 <a href="/FMA/live">/FMA/live</a> (drag to rotate)</p>
<p class="leg">tissue:
 <span style="background:#ebe0c7">bone</span><span style="background:#9fb8d9">cartilage</span>
 <span style="background:#bd5c57">muscle</span><span style="background:#cc3838">vessel</span>
 <span style="background:#ebd152">nerve</span><span style="background:#cc9488">organ</span>
 <span style="background:#e6e6db">ligament/tendon</span></p>
<h3>Resolve a part</h3>
<p><input id="q" value="FMA3734" placeholder="FMA id"> <button onclick="go()">GET /FMA/guid/&lt;id&gt;</button></p>
<pre id="out">aorta → try it</pre>
<p><a href="/FMA/manifest">/FMA/manifest</a> (TSV, all GUIDs)</p>
<script>
async function go(){{const id=document.getElementById('q').value.trim();
 const r=await fetch('/FMA/guid/'+id);document.getElementById('out').textContent=await r.text();}}
</script></body></html>"##);
    body.into_bytes()
}

fn respond(stream: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) {
    let head = format!("HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n", body.len());
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
}

fn handle(mut stream: TcpStream, manifest: &HashMap<String, (String, String, String)>, n: usize) {
    let mut buf = [0u8; 2048];
    let Ok(read) = stream.read(&mut buf) else { return };
    let req = String::from_utf8_lossy(&buf[..read]);
    let path = req.split_whitespace().nth(1).unwrap_or("/");

    match path {
        "/" | "/FMA" | "/FMA/" => respond(&mut stream, "200 OK", "text/html; charset=utf-8", &index_html(n)),
        "/FMA/body.png" => match std::fs::read("mesh/mesh_tissues.png") {
            Ok(b) => respond(&mut stream, "200 OK", "image/png", &b),
            Err(_) => respond(&mut stream, "404 Not Found", "text/plain", b"render mesh_tissues.png first"),
        },
        "/FMA/skeleton.png" => match std::fs::read("mesh/mesh_bones.png") {
            Ok(b) => respond(&mut stream, "200 OK", "image/png", &b),
            Err(_) => respond(&mut stream, "404 Not Found", "text/plain", b"render mesh_bones.png first"),
        },
        "/FMA/manifest" | "/FMA/manifest.tsv" => match std::fs::read("guid/guid_manifest.tsv") {
            Ok(b) => respond(&mut stream, "200 OK", "text/tab-separated-values", &b),
            Err(_) => respond(&mut stream, "404 Not Found", "text/plain", b"run guid first"),
        },
        p if p.starts_with("/FMA/guid/") => {
            let id = p.trim_start_matches("/FMA/guid/");
            let json = match manifest.get(id) {
                Some((sys, guid, dn)) => format!(
                    "{{\"fma\":\"{}\",\"container\":\"{}\",\"guid\":\"{}\",\"distinguished_name\":\"{}\"}}",
                    esc(id), esc(sys), esc(guid), esc(dn)
                ),
                None => format!("{{\"fma\":\"{}\",\"error\":\"not found\"}}", esc(id)),
            };
            respond(&mut stream, "200 OK", "application/json", json.as_bytes());
        }
        "/FMA/turntable" | "/FMA/turntable/" => respond(&mut stream, "200 OK", "text/html; charset=utf-8", &player_html(FRAMES.len(), false)),
        "/FMA/live" | "/FMA/live/" => respond(&mut stream, "200 OK", "text/html; charset=utf-8", &player_html(FRAMES.len(), true)),
        "/FMA/info" => respond(&mut stream, "200 OK", "application/json", format!("{{\"nframes\":{},\"fps\":90}}", FRAMES.len()).as_bytes()),
        p if p.starts_with("/FMA/frame/") => {
            let i: usize = p.trim_start_matches("/FMA/frame/").trim_end_matches(".png").parse().unwrap_or(usize::MAX);
            match FRAMES.get(i) {
                Some(b) => respond(&mut stream, "200 OK", "image/png", b),
                None => respond(&mut stream, "404 Not Found", "text/plain", b"frame oob"),
            }
        }
        _ => respond(&mut stream, "404 Not Found", "text/plain", b"routes: /FMA /FMA/turntable /FMA/live /FMA/frame/<i> /FMA/guid/<id> /FMA/manifest"),
    }
}

fn main() {
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8088);
    let manifest = load_manifest("guid/guid_manifest.tsv");
    let n = manifest.len();
    let listener = TcpListener::bind(("0.0.0.0", port)).expect("bind");
    eprintln!("[/FMA] serving {n} GUID-addressed parts on http://0.0.0.0:{port}/FMA");
    for stream in listener.incoming().flatten() {
        handle(stream, &manifest, n);
    }
}
