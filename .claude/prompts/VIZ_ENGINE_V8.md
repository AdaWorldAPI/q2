# Server-Side V8 JIT Visualization Engine

## FIRST: Read .claude/rules/architectural-compliance.md
## SECOND: Read .claude/rules/borrow-strategy.md

## What This Is

deno_core V8 is already in the q2 binary (`quarto-system-runtime`).
Currently it renders EJS templates. We extend it to JIT-compile
visualization logic: force layout, activation overlays, particle
physics, glow computation. The Rust binary computes every frame.
The cockpit is a thin Canvas that paints pre-computed frame data
received over SSE.

## Why Server-Side

1. **Same rendering in cockpit AND PDF export** — server computes frames,
   cockpit paints live, publisher renders to SVG. One renderer, two outputs.
2. **Thinking graph writes its own viz rules** — Layer 10 (Meta-Cognition)
   outputs JS strings that V8 JIT-compiles. The brain decides how to
   display its own thinking. Rules evolve as the graph learns.
3. **Cockpit is dumb** — ~50 lines of Canvas. No vis-network, no d3,
   no library. Just `drawImage(frameData)`. Any browser, any device.

## Architecture

```
┌─────────────────────────────────────────────────┐
│ Rust Binary (q2)                                │
│                                                 │
│  StreamingRunner (rs-graph-llm)                 │
│    │                                            │
│    ├→ Layer 5: semiring reasoning               │
│    ├→ Layer 6: NARS revision, seal check        │
│    ├→ Layer 10: meta-cognition → viz_rules JS   │
│    │                                            │
│    ▼                                            │
│  VizEngine (new)                                │
│    ├─ V8 JsRuntime (deno_core, already in dep)  │
│    ├─ Force simulation (JS, JIT-compiled)       │
│    ├─ Activation overlay (JS, from Layer 10)    │
│    ├─ Particle physics (JS, JIT-compiled)       │
│    │                                            │
│    ▼                                            │
│  Frame Producer                                 │
│    ├─ 60fps tick → FrameData struct             │
│    ├─ SSE stream → cockpit (live)               │
│    └─ SVG render → publisher (export)           │
└─────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────┐
│ Cockpit (browser, thin Canvas)                  │
│                                                 │
│  EventSource('/mcp/sse')                        │
│    │                                            │
│    ▼                                            │
│  requestAnimationFrame loop                     │
│    ├─ Read latest FrameData from SSE buffer     │
│    ├─ ctx.clearRect()                           │
│    ├─ Draw edges (color, width, opacity)        │
│    ├─ Draw nodes (position, color, glow radius) │
│    ├─ Draw particles (position, trail)          │
│    ├─ Draw epiphanies (flash, dashed line)      │
│    └─ No physics. No layout. Just paint.        │
└─────────────────────────────────────────────────┘
```

## FrameData — What SSE Streams

```rust
#[derive(Serialize)]
struct FrameData {
    /// Monotonic frame counter
    frame: u64,
    /// Current temporal version being processed
    version: usize,
    
    /// Node states (position + visual)
    nodes: Vec<NodeFrame>,
    /// Edge states (visual only, topology from initial load)
    edges: Vec<EdgeFrame>,
    /// In-flight particles
    particles: Vec<ParticleFrame>,
    /// Active epiphanies (edge births)
    epiphanies: Vec<EpiphanyFrame>,
    /// Global state
    global_dim: f32,           // 0.2 = spotlight, 1.0 = normal
    breathing_phase: f32,      // sine wave 0.0 → 1.0
    active_version_label: String,
    seal_status: String,       // "Staunen" | "Wisdom"
}

#[derive(Serialize)]
struct NodeFrame {
    id: String,
    x: f32,
    y: f32,
    color: [u8; 3],           // RGB
    glow_radius: f32,         // 0 = no glow, 40 = max
    glow_color: [u8; 3],
    opacity: f32,
    icon_url: Option<String>, // noun-*.png path
    label_visible: bool,
}

#[derive(Serialize)]
struct EdgeFrame {
    source_idx: usize,        // index into nodes array
    target_idx: usize,
    color: [u8; 3],
    width: f32,
    opacity: f32,
    dashed: bool,             // inferred edge
}

#[derive(Serialize)]
struct ParticleFrame {
    x: f32,
    y: f32,
    color: [u8; 3],
    radius: f32,
    trail_opacity: f32,
}

#[derive(Serialize)]
struct EpiphanyFrame {
    x: f32,
    y: f32,
    flash_radius: f32,        // expanding ring, 0 when done
    flash_opacity: f32,
    edge_progress: f32,       // 0.0 → 1.0 as dashed line grows
    label: String,
    truth_f: f32,
    truth_c: f32,
}
```

SSE sends one FrameData per tick. At 30fps that's ~33ms per frame.
For 221 nodes: FrameData ≈ 15KB JSON. At 30fps = 450KB/s. Fine for
localhost. For Railway: drop to 10fps (150KB/s) or send deltas.

## V8 Visualization Runtime

### VizEngine

```rust
// crates/quarto/src/viz_engine.rs

use deno_core::JsRuntime;
use serde_json;

pub struct VizEngine {
    runtime: JsRuntime,
}

impl VizEngine {
    pub fn new() -> Self {
        let mut runtime = JsRuntime::new(deno_core::RuntimeOptions {
            ..Default::default()
        });
        
        // Load the force simulation + rendering kernel
        runtime.execute_script(
            "viz_kernel.js",
            include_str!("viz_kernel.js").into(),
        ).expect("viz kernel load");
        
        Self { runtime }
    }
    
    /// Load graph data into the V8 simulation
    pub fn load_graph(&mut self, nodes_json: &str, edges_json: &str) {
        let script = format!(
            "globalThis.vizKernel.loadGraph({}, {})",
            nodes_json, edges_json
        );
        self.runtime.execute_script("load", script.into()).unwrap();
    }
    
    /// Inject a visualization rule from the thinking graph
    pub fn inject_rule(&mut self, rule_js: &str) {
        let script = format!(
            "globalThis.vizKernel.addRule({})",
            serde_json::to_string(rule_js).unwrap()
        );
        self.runtime.execute_script("rule", script.into()).unwrap();
    }
    
    /// Set activation state (from StreamChunk)
    pub fn set_activation(&mut self, layer_id: &str, node_ids: &[String], intensity: f32) {
        let script = format!(
            "globalThis.vizKernel.activate('{}', {}, {})",
            layer_id,
            serde_json::to_string(node_ids).unwrap(),
            intensity
        );
        self.runtime.execute_script("activate", script.into()).unwrap();
    }
    
    /// Spawn particles along edges
    pub fn spawn_particles(&mut self, edge_indices: &[usize]) {
        let script = format!(
            "globalThis.vizKernel.spawnParticles({})",
            serde_json::to_string(edge_indices).unwrap()
        );
        self.runtime.execute_script("particles", script.into()).unwrap();
    }
    
    /// Trigger epiphany (inferred edge birth)
    pub fn trigger_epiphany(&mut self, source_id: &str, target_id: &str, truth_f: f32, truth_c: f32) {
        let script = format!(
            "globalThis.vizKernel.epiphany('{}', '{}', {}, {})",
            source_id, target_id, truth_f, truth_c
        );
        self.runtime.execute_script("epiphany", script.into()).unwrap();
    }
    
    /// Compute one frame — runs force tick + activation overlay + particles
    pub fn tick(&mut self) -> FrameData {
        self.runtime.execute_script("tick", "globalThis.vizKernel.tick()".into()).unwrap();
        
        let result = self.runtime.execute_script(
            "frame",
            "JSON.stringify(globalThis.vizKernel.getFrame())".into(),
        ).unwrap();
        
        let scope = &mut self.runtime.handle_scope();
        let local = v8::Local::new(scope, result);
        let json_str = local.to_rust_string_lossy(scope);
        
        serde_json::from_str(&json_str).unwrap()
    }
}
```

### viz_kernel.js (JIT-compiled by V8)

```javascript
// This runs inside V8 on the server, NOT in the browser

globalThis.vizKernel = (() => {
    let nodes = [];
    let edges = [];
    let particles = [];
    let epiphanies = [];
    let rules = [];
    let activations = new Map();
    let globalDim = 1.0;
    let breathPhase = 0;
    let frame = 0;
    let sealStatus = 'idle';
    let versionLabel = '';
    
    // Force simulation state
    let alpha = 0;
    const REPULSION = -300;
    const SPRING_LEN = 120;
    const SPRING_K = 0.005;
    const DAMPING = 0.6;
    const CENTER_GRAVITY = 0.001;
    const WIDTH = 1200;
    const HEIGHT = 800;
    
    function loadGraph(nodeData, edgeData) {
        nodes = nodeData.map((n, i) => ({
            ...n, 
            x: WIDTH/2 + 350 * Math.cos(2 * Math.PI * i / nodeData.length),
            y: HEIGHT/2 + 250 * Math.sin(2 * Math.PI * i / nodeData.length),
            vx: 0, vy: 0,
            color: [0x33, 0x41, 0x55],  // dim grey default
            glow_radius: 0,
            glow_color: [0, 0, 0],
            opacity: 0.85,
            label_visible: false,
        }));
        edges = edgeData.map(e => ({
            ...e,
            source_idx: nodeData.findIndex(n => n.id === e.source),
            target_idx: nodeData.findIndex(n => n.id === e.target),
            color: [0x1e, 0x2a, 0x3a],  // dim border
            width: 1,
            opacity: 0.4,
            dashed: false,
        }));
        alpha = 1.0;
    }
    
    function activate(layerId, nodeIds, intensity) {
        const colorMap = {
            'cascade_search_foveal':   [0x00, 0xbc, 0xd4],  // teal
            'cascade_search_parafoveal': [0x7c, 0x4d, 0xff], // purple
            'semiring_reason':         [0x7c, 0x4d, 0xff],   // purple (bright)
            'memory_consolidate_staunen': [0xff, 0xc1, 0x07], // amber
            'memory_consolidate_wisdom':  [0x4c, 0xaf, 0x50], // green
        };
        const color = colorMap[layerId] || [0x00, 0xbc, 0xd4];
        
        const idSet = new Set(nodeIds);
        nodes.forEach(n => {
            if (idSet.has(n.id)) {
                n.color = color;
                n.glow_radius = intensity * 40;
                n.glow_color = color;
                n.opacity = 0.4 + intensity * 0.6;
                n.label_visible = true;
            }
        });
        
        // Dim non-active nodes
        if (nodeIds.length > 0) {
            globalDim = 0.2;
        }
        
        activations.set(layerId, { nodeIds, intensity, color });
    }
    
    function spawnParticles(edgeIndices) {
        edgeIndices.forEach(idx => {
            const e = edges[idx];
            if (!e) return;
            particles.push({
                edge_idx: idx,
                progress: 0,
                speed: 0.03 + Math.random() * 0.02,
                color: [0xff, 0xff, 0xff],
                radius: 3,
            });
        });
    }
    
    function epiphany(sourceId, targetId, truthF, truthC) {
        const si = nodes.findIndex(n => n.id === sourceId);
        const ti = nodes.findIndex(n => n.id === targetId);
        if (si < 0 || ti < 0) return;
        
        const mx = (nodes[si].x + nodes[ti].x) / 2;
        const my = (nodes[si].y + nodes[ti].y) / 2;
        
        epiphanies.push({
            source_idx: si,
            target_idx: ti,
            x: mx, y: my,
            flash_radius: 0,
            flash_opacity: 1,
            edge_progress: 0,
            label: 'inferred',
            truth_f: truthF,
            truth_c: truthC,
            age: 0,
        });
    }
    
    function addRule(ruleCode) {
        try {
            const fn = new Function('nodes', 'edges', 'particles', 'frame', ruleCode);
            rules.push(fn);
        } catch(e) {
            // Invalid rule, skip
        }
    }
    
    function tick() {
        frame++;
        breathPhase = (Math.sin(frame / 60) + 1) / 2;
        
        // Force simulation
        if (alpha > 0.001) {
            const cx = WIDTH / 2, cy = HEIGHT / 2;
            
            // Center gravity
            nodes.forEach(n => {
                n.vx += (cx - n.x) * CENTER_GRAVITY * alpha;
                n.vy += (cy - n.y) * CENTER_GRAVITY * alpha;
            });
            
            // Repulsion
            for (let i = 0; i < nodes.length; i++) {
                for (let j = i + 1; j < nodes.length; j++) {
                    let dx = nodes[j].x - nodes[i].x;
                    let dy = nodes[j].y - nodes[i].y;
                    let dist = Math.sqrt(dx*dx + dy*dy) || 1;
                    let f = REPULSION * alpha / (dist * dist);
                    let fx = (dx/dist) * f, fy = (dy/dist) * f;
                    nodes[i].vx -= fx; nodes[i].vy -= fy;
                    nodes[j].vx += fx; nodes[j].vy += fy;
                }
            }
            
            // Springs
            edges.forEach(e => {
                if (e.source_idx < 0 || e.target_idx < 0) return;
                const s = nodes[e.source_idx], t = nodes[e.target_idx];
                let dx = t.x - s.x, dy = t.y - s.y;
                let dist = Math.sqrt(dx*dx + dy*dy) || 1;
                let f = (dist - SPRING_LEN) * SPRING_K * alpha;
                let fx = (dx/dist) * f, fy = (dy/dist) * f;
                s.vx += fx; s.vy += fy;
                t.vx -= fx; t.vy -= fy;
            });
            
            // Integrate
            nodes.forEach(n => {
                n.vx *= DAMPING; n.vy *= DAMPING;
                n.x += n.vx; n.y += n.vy;
                n.x = Math.max(40, Math.min(WIDTH-40, n.x));
                n.y = Math.max(40, Math.min(HEIGHT-40, n.y));
            });
            
            alpha *= 0.995;
        }
        
        // Particle physics
        particles.forEach(p => { p.progress += p.speed; });
        particles = particles.filter(p => p.progress < 1.0);
        
        // Epiphany animation
        epiphanies.forEach(e => {
            e.age++;
            if (e.age < 15) {
                e.flash_radius = e.age * 3;
                e.flash_opacity = 1.0 - (e.age / 15);
            } else {
                e.flash_radius = 0;
                e.flash_opacity = 0;
                e.edge_progress = Math.min(1.0, (e.age - 15) / 20);
            }
        });
        epiphanies = epiphanies.filter(e => e.age < 60);
        
        // Glow decay
        nodes.forEach(n => {
            if (n.glow_radius > 0) {
                n.glow_radius *= 0.98;
                if (n.glow_radius < 0.5) n.glow_radius = 0;
            }
        });
        
        // Global dim recovery
        if (globalDim < 1.0) globalDim += 0.005;
        
        // Apply custom rules (from thinking graph Layer 10)
        rules.forEach(fn => {
            try { fn(nodes, edges, particles, frame); } catch(e) {}
        });
    }
    
    function getFrame() {
        // Compute particle positions from edge geometry
        const particleFrames = particles.map(p => {
            const e = edges[p.edge_idx];
            if (!e || e.source_idx < 0 || e.target_idx < 0) return null;
            const s = nodes[e.source_idx], t = nodes[e.target_idx];
            return {
                x: s.x + (t.x - s.x) * p.progress,
                y: s.y + (t.y - s.y) * p.progress,
                color: p.color,
                radius: p.radius,
                trail_opacity: 1.0 - p.progress * 0.7,
            };
        }).filter(Boolean);
        
        return {
            frame,
            version: 0,
            nodes: nodes.map(n => ({
                id: n.id,
                x: n.x, y: n.y,
                color: n.color,
                glow_radius: n.glow_radius,
                glow_color: n.glow_color,
                opacity: n.opacity * (globalDim < 1.0 && n.glow_radius === 0 ? globalDim : 1.0),
                icon_url: n.image || null,
                label_visible: n.label_visible,
            })),
            edges: edges.map(e => ({
                source_idx: e.source_idx,
                target_idx: e.target_idx,
                color: e.color,
                width: e.width,
                opacity: e.opacity * (globalDim < 1.0 ? 0.3 : 1.0),
                dashed: e.dashed,
            })),
            particles: particleFrames,
            epiphanies: epiphanies.map(e => ({
                x: e.x, y: e.y,
                flash_radius: e.flash_radius,
                flash_opacity: e.flash_opacity,
                edge_progress: e.edge_progress,
                label: e.label,
                truth_f: e.truth_f,
                truth_c: e.truth_c,
            })),
            global_dim: globalDim,
            breathing_phase: breathPhase,
            active_version_label: versionLabel,
            seal_status: sealStatus,
        };
    }
    
    return { loadGraph, activate, spawnParticles, epiphany, addRule, tick, getFrame };
})();
```

## Cockpit (thin Canvas consumer)

```typescript
// CognitionCanvas.tsx — ~60 lines of render logic

const canvas = canvasRef.current;
const ctx = canvas.getContext('2d');

// SSE buffer — always read latest frame
let latestFrame: FrameData | null = null;
const sse = new EventSource('/mcp/sse');
sse.addEventListener('frame', (e) => {
    latestFrame = JSON.parse(e.data);
});

function paint() {
    if (!latestFrame) { requestAnimationFrame(paint); return; }
    const f = latestFrame;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    
    // Edges
    f.edges.forEach(e => {
        const s = f.nodes[e.source_idx], t = f.nodes[e.target_idx];
        ctx.strokeStyle = `rgba(${e.color[0]},${e.color[1]},${e.color[2]},${e.opacity})`;
        ctx.lineWidth = e.width;
        if (e.dashed) ctx.setLineDash([4, 4]);
        ctx.beginPath();
        ctx.moveTo(s.x, s.y);
        ctx.lineTo(t.x, t.y);
        ctx.stroke();
        ctx.setLineDash([]);
    });
    
    // Nodes
    f.nodes.forEach(n => {
        // Glow
        if (n.glow_radius > 0) {
            const grad = ctx.createRadialGradient(n.x, n.y, 0, n.x, n.y, n.glow_radius);
            grad.addColorStop(0, `rgba(${n.glow_color[0]},${n.glow_color[1]},${n.glow_color[2]},0.4)`);
            grad.addColorStop(1, 'transparent');
            ctx.fillStyle = grad;
            ctx.beginPath();
            ctx.arc(n.x, n.y, n.glow_radius, 0, Math.PI * 2);
            ctx.fill();
        }
        // Node circle
        ctx.globalAlpha = n.opacity;
        ctx.fillStyle = `rgb(${n.color[0]},${n.color[1]},${n.color[2]})`;
        ctx.beginPath();
        ctx.arc(n.x, n.y, 8, 0, Math.PI * 2);
        ctx.fill();
        ctx.globalAlpha = 1;
    });
    
    // Particles
    f.particles.forEach(p => {
        ctx.globalAlpha = p.trail_opacity;
        ctx.fillStyle = `rgb(${p.color[0]},${p.color[1]},${p.color[2]})`;
        ctx.shadowBlur = 10;
        ctx.shadowColor = `rgb(${p.color[0]},${p.color[1]},${p.color[2]})`;
        ctx.beginPath();
        ctx.arc(p.x, p.y, p.radius, 0, Math.PI * 2);
        ctx.fill();
        ctx.shadowBlur = 0;
        ctx.globalAlpha = 1;
    });
    
    // Epiphanies
    f.epiphanies.forEach(e => {
        if (e.flash_radius > 0) {
            ctx.strokeStyle = `rgba(0,229,255,${e.flash_opacity})`;
            ctx.lineWidth = 2;
            ctx.beginPath();
            ctx.arc(e.x, e.y, e.flash_radius, 0, Math.PI * 2);
            ctx.stroke();
        }
    });
    
    requestAnimationFrame(paint);
}
requestAnimationFrame(paint);
```

## SSE Frame Streaming

```rust
// In notebook_server.rs, add a frame stream endpoint

async fn viz_frame_stream(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        let mut interval = tokio::time::interval(Duration::from_millis(33)); // 30fps
        loop {
            interval.tick().await;
            let mut engine = state.viz_engine.lock().await;
            engine.tick();
            let frame = engine.get_frame();
            let json = serde_json::to_string(&frame).unwrap();
            yield Ok(Event::default().data(json).event("frame"));
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

// Route
app = app.route("/viz/frames", get(viz_frame_stream));
```

## Thinking Graph → Viz Rules Pipeline

```rust
// When Layer 10 completes, it outputs viz rules as JS strings
// These get injected into V8 at runtime

async fn handle_layer_10_output(
    viz_engine: &mut VizEngine,
    pet_scan: &serde_json::Value,
) {
    // Example: Layer 10 decides that heavily-reasoned clusters
    // should develop persistent glow based on encounter frequency
    if let Some(rules) = pet_scan.get("viz_rules") {
        for rule in rules.as_array().unwrap_or(&vec![]) {
            if let Some(code) = rule.as_str() {
                viz_engine.inject_rule(code);
            }
        }
    }
}

// Layer 10 might output something like:
// "nodes.filter(n => n.encounters > 5).forEach(n => { n.glow_radius = Math.min(30, n.encounters * 3); })"
//
// This gets JIT-compiled by V8 and runs every tick.
// The visualization EVOLVES as the graph learns.
```

## PDF Export Path

Same VizEngine, different consumer:

```rust
// Instead of streaming frames via SSE, render N frames to SVG

async fn export_viz_to_svg(engine: &mut VizEngine, frames: usize) -> Vec<String> {
    let mut svgs = Vec::new();
    for _ in 0..frames {
        engine.tick();
        let frame = engine.get_frame();
        svgs.push(frame_to_svg(&frame));
    }
    svgs
}

fn frame_to_svg(frame: &FrameData) -> String {
    // Same data, SVG output instead of Canvas commands
    // Used by publisher for PDF export
}
```

## Thread Model

V8's JsRuntime is `!Send + !Sync`. Current q2 creates a fresh engine
per JS operation. For viz, we need a DEDICATED thread:

```rust
// Spawn viz engine on its own thread with a channel
let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(100);
let (frame_tx, frame_rx) = tokio::sync::broadcast::channel(16);

std::thread::spawn(move || {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    
    rt.block_on(async {
        let mut engine = VizEngine::new();
        let mut interval = tokio::time::interval(Duration::from_millis(33));
        
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    engine.tick();
                    let frame = engine.get_frame();
                    let _ = frame_tx.send(frame);
                }
                Some(cmd) = cmd_rx.recv() => {
                    match cmd {
                        VizCmd::LoadGraph(nodes, edges) => engine.load_graph(&nodes, &edges),
                        VizCmd::Activate(layer, ids, intensity) => engine.set_activation(&layer, &ids, intensity),
                        VizCmd::SpawnParticles(indices) => engine.spawn_particles(&indices),
                        VizCmd::Epiphany(src, tgt, f, c) => engine.trigger_epiphany(&src, &tgt, f, c),
                        VizCmd::InjectRule(code) => engine.inject_rule(&code),
                    }
                }
            }
        }
    });
});
```

SSE endpoint subscribes to `frame_rx`. Multiple cockpit clients can
connect — they all receive the same frame broadcast.

## Implementation Order

1. Create `crates/quarto/src/viz_engine.rs` — V8 wrapper
2. Create `crates/quarto/src/viz_kernel.js` — force sim + overlay logic
3. Add `/viz/frames` SSE endpoint to notebook_server.rs
4. Create `cockpit/src/components/CognitionCanvas.tsx` — thin Canvas
5. Wire StreamingRunner → VizCmd channel (activate, particles, epiphanies)
6. Wire Layer 10 → inject_rule (self-evolving visualization)
7. Wire publisher → frame_to_svg (PDF export)

## What NOT to Do

- Do NOT run V8 on the main tokio runtime (it's !Send)
- Do NOT put force simulation in the browser (server owns physics)
- Do NOT use vis-network or d3 in the cockpit (thin Canvas only)
- Do NOT send full node data every frame (send positions + visual state only)
- Do NOT copy graph data for SIMD — slices into backing store
