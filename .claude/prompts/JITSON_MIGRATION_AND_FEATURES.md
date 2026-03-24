# JITSON Migration + Features

## Part 1: Migration — rustynum → ndarray (URGENT)

jitson lives in `AdaWorldAPI/rustynum` which is being retired.
If rustynum dies, jitson dies. Nobody else has JSON-to-native-code compilation.

### What jitson IS

JSON config values become CPU instructions. Not interpreted. Not evaluated.
COMPILED. `threshold: 500` becomes `CMP reg, 500` as an immediate operand.
No memory load. The config IS the code.

```json
{
  "kernel": "hamming_distance",
  "scan": { "threshold": 2048, "record_size": 256, "top_k": 10 },
  "pipeline": [
    { "stage": "xor",    "avx512": "vpxord" },
    { "stage": "popcnt", "avx512": "vpopcntd" },
    { "stage": "reduce", "avx512": "vpord" }
  ],
  "cranelift": { "preset": "sapphire_rapids", "opt_level": "speed" }
}
```

This JSON compiles to a native function pointer via Cranelift JIT.
The function scans a packed database with all parameters baked as immediates.
Cache hit: 455ns. Cold compile: 521µs. Engine init: 47µs.

### Files to migrate

```
FROM rustynum:
  rustynum-core/src/jitson.rs     → ndarray hpc module (JSON parser + validator + template)
  rustynum-core/src/jit_scan.rs   → ndarray hpc module (ScanConfig, SIMD trampolines, scan_hamming)
  rustynum-core/src/packed.rs     → ndarray hpc module (PackedDatabase, stroke-aligned cascade)

  jitson/src/ir.rs                → ndarray jitson crate or feature (ScanParams, PhilosopherIR, RecipeIR)
  jitson/src/engine.rs            → ndarray jitson crate or feature (JitEngine, Cranelift module, cache)
  jitson/src/scan_jit.rs          → ndarray jitson crate or feature (build_scan_ir Cranelift codegen)
  jitson/src/detect.rs            → ndarray jitson crate or feature (CpuCaps → Cranelift ISA)

EXTERNAL DEP:
  AdaWorldAPI/wasmtime (patched Cranelift with AVX-512 VPOPCNTDQ, VNNI, VPTERNLOG, BITALG)
```

### Migration structure in ndarray

```
AdaWorldAPI/ndarray/
  src/
    hpc/
      jitson/
        mod.rs          ← re-exports
        parser.rs       ← from rustynum-core/src/jitson.rs (no_std JSON parser)
        validator.rs    ← from rustynum-core/src/jitson.rs (schema validation)
        template.rs     ← from rustynum-core/src/jitson.rs (JitsonTemplate, conversion)
        scan_config.rs  ← from rustynum-core/src/jit_scan.rs (ScanConfig, trampolines)
        packed.rs       ← from rustynum-core/src/packed.rs (PackedDatabase, cascade)
      jitson_cranelift/ ← feature-gated behind "jit-native"
        mod.rs
        ir.rs           ← from jitson/src/ir.rs
        engine.rs       ← from jitson/src/engine.rs
        scan_jit.rs     ← from jitson/src/scan_jit.rs
        detect.rs       ← from jitson/src/detect.rs
```

Feature flags:
```toml
[features]
jitson = []                    # parser + validator + template + packed (no Cranelift)
jit-native = ["jitson", "dep:cranelift-codegen", "dep:cranelift-frontend", "dep:cranelift-jit", "dep:cranelift-module"]
```

`jitson` alone gives you the JSON→ScanConfig pipeline, PackedDatabase,
SIMD trampolines, and the non-JIT scan path. Zero Cranelift dependency.
`jit-native` adds Cranelift compilation to native function pointers.

### Rust 1.94 unlocks to apply during migration

- `array_windows::<N>()` → SIMD read path over packed stroke data
- `element_offset` → pointer math in PackedDatabase without manual arithmetic
- `LazyLock::get_mut` → JitEngine kernel cache (mutable during build, immutable during compute)
- `f32::mul_add` const → compile-time threshold constants with FMA precision

### Rules
- SIMD stays on slices. Never copy data for SIMD operations.
- PackedDatabase stroke layout must remain contiguous for prefetcher.
- Cranelift is ALWAYS behind feature gate. Never mandatory.
- The no_std JSON parser must remain no_std (works in embedded/WASM).

---

## Part 2: Pumpkin Shopping List

### Prompt for AdaWorldAPI/Pumpkin CC session

```
# What ndarray jitson can do for Pumpkin

Read this, then give me a SHOPPING LIST of where Pumpkin would benefit.

## What jitson offers

jitson compiles JSON config into native function pointers via Cranelift JIT.
Config values become CPU immediates — no memory loads, no interpretation.
It also provides PackedDatabase: stroke-aligned memory layout where data
for the same operation across all candidates is contiguous, enabling
perfect sequential prefetching and 90% early rejection per stroke.

## SIMD primitives available

- Hamming distance (VPOPCNTDQ accelerated): compare two bitfields
- Packed scan with cascade rejection: 3-stroke progressive search
  Stroke 1 (128B): coarse filter, 90% eliminated
  Stroke 2 (384B): medium filter, 90% of survivors eliminated
  Stroke 3 (1536B): precise ranking of final candidates
- Focus masks: VPANDQ bitmask selects which dimensions participate
- Prefetch hints: PREFETCHT0 baked into the scan loop
- Top-K heap: early termination when K candidates found

## Where this maps to Minecraft server operations

Think about ANY place Pumpkin does:

1. **Chunk palette lookup** — block state IDs packed as bits.
   Palette indices are small integers packed into longs.
   Unpacking = bitshift + mask. jitson bakes palette width as immediate.
   4096 blocks per section × 16 sections = 65536 lookups per chunk.

2. **Light propagation** — BFS over block neighbors.
   Each block checks 6 neighbors. Light level is 4 bits.
   Packed into u64s, the propagation is XOR/AND/compare.
   jitson bakes light decay as immediate, sky light mask as bitmask.

3. **Collision detection** — AABB tests over block shapes.
   Pumpkin has collision_shape.rs. Entity vs block grid.
   PackedDatabase layout: all X coords contiguous, all Y contiguous.
   SIMD compares 16 AABBs simultaneously.

4. **Entity search** — find entities near a position.
   Currently linear scan over entity list.
   PackedDatabase: pack entity positions as stroke-aligned layout.
   Stroke 1: coarse grid cell match (90% rejected).
   Stroke 2: distance check on survivors.

5. **Block state matching** — "is this block waterlogged AND facing north?"
   Multiple property checks = multiple bit tests.
   jitson compiles the property query as a single VPTERNLOGD
   (3-input truth table instruction, checks 3 properties in 1 cycle).

6. **NBT parsing** — Pumpkin reads NBT from Anvil region files.
   NBT tag scanning is sequential. PackedDatabase layout for tag types
   enables batch scanning: find all "Entities" tags across 1024 chunks
   in one SIMD pass.

7. **World generation noise** — Perlin/simplex noise evaluation.
   The noise parameters (octaves, frequency, amplitude) could be
   jitson-compiled as immediates. Each biome gets its own compiled
   noise function. No parameter loading per sample.

8. **Tick scheduling** — which blocks need random ticks?
   Block categories (crops, liquids, redstone) = bitmask.
   jitson compiles tick eligibility as VPANDQ + VPOPCNTDQ.
   "Count crops needing tick in this chunk" = one instruction.

## What I need from you

Read Pumpkin's source. Find the actual hotspots. Give me a shopping list:

| Hotspot | File | Current approach | What jitson would do | Estimated speedup |
|---------|------|-----------------|---------------------|-------------------|
| ...     | ...  | ...             | ...                 | ...               |

Focus on:
- Anything that loops over 4096+ items (chunk-scale operations)
- Anything that unpacks bit-packed data
- Anything that does property matching / filtering
- Anything that searches collections (entities, blocks, chunks)
- Anything where parameters are loaded from config per iteration

Don't guess. READ the code. The branches `exp/storage-lance-abtest`
and `claude/compare-simd-implementations-btTgj` may already have
relevant work.
```

---

## Part 3: Prerendered Flythrough (Amiga-style)

### The Concept

Like Amiga demos: precompute what you can't render in real time.
The cockpit graph has 221 nodes. In real time: Canvas 2D, 30fps, fine.
But for the CLOSER — the satellite zoom, the drone follow, the 8-direction
orbital flight — prerender to video frames server-side, play back as
animation. GPU-quality visuals without GPU.

### What to prerender

```
1. SATELLITE ZOOM
   Start: entire graph as a dim cloud, 2000px above
   Camera descends over 5 seconds
   Graph resolves: first clusters, then nodes, then labels
   Final frame: cockpit view at working distance
   Frames: 150 @ 30fps = 5 seconds

2. DRONE FOLLOW (inference chain)
   Camera follows a signal particle along an inference path
   Palantir → US_DoD → Gotham → NSO_Group → ...
   Each node fills the frame as the drone passes
   Properties flash briefly (name, type, truth value)
   Then pull back to see the connection glow
   Frames: 300 @ 30fps = 10 seconds

3. ORBITAL FLIGHT (8 directions)
   Camera orbits the settled graph at fixed distance
   8 cardinal directions × 45° increments
   At each position, hold for 1 second
   Edges face-on → edges edge-on → edges face-on
   The graph structure reveals differently from each angle
   Frames: 240 @ 30fps = 8 seconds

4. TEMPORAL GROWTH (the full enrichment playback)
   Fixed camera, overhead view
   v00 → v42 plays out
   Nodes appear, clusters form, connections grow
   Staunen bursts as amber flashes
   Wisdom phases as green calm
   Full 42-version playback compressed to 30 seconds
   Frames: 900 @ 30fps = 30 seconds
```

### How to prerender

The VizEngine (V8 JIT, server-side) already computes frames.
For prerendering, instead of streaming via SSE, write frames to disk:

```rust
async fn prerender_flythrough(
    engine: &mut VizEngine,
    camera_path: &[CameraKeyframe],
    output_dir: &Path,
    fps: u32,
) -> Vec<PathBuf> {
    let mut frames = Vec::new();
    
    for (i, keyframe) in camera_path.iter().enumerate() {
        // Set camera position in viz_kernel.js
        engine.set_camera(keyframe.x, keyframe.y, keyframe.zoom, keyframe.rotation);
        
        // Tick physics (or skip if graph should be frozen)
        if keyframe.physics_active {
            engine.tick();
        }
        
        // Get frame data
        let frame = engine.get_frame();
        
        // Render to SVG (vector) or PNG (raster via resvg)
        let svg = frame_to_svg(&frame, keyframe);
        let path = output_dir.join(format!("frame_{:05}.svg", i));
        std::fs::write(&path, &svg)?;
        frames.push(path);
    }
    
    frames
}

// Convert SVG sequence to video (ffmpeg)
fn encode_video(frames_dir: &Path, output: &Path, fps: u32) {
    std::process::Command::new("ffmpeg")
        .args([
            "-framerate", &fps.to_string(),
            "-i", &frames_dir.join("frame_%05d.svg").to_string_lossy(),
            "-c:v", "libx264",
            "-pix_fmt", "yuv420p",
            "-crf", "18",
            output.to_str().unwrap(),
        ])
        .output()
        .expect("ffmpeg");
}
```

### SVG → PNG path (for raster quality)

```rust
// Use resvg for server-side SVG → PNG rendering
// No browser needed. No GPU needed. Just CPU.
fn svg_to_png(svg: &str, width: u32, height: u32) -> Vec<u8> {
    let tree = resvg::usvg::Tree::from_str(svg, &Default::default()).unwrap();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height).unwrap();
    resvg::render(&tree, Default::default(), &mut pixmap.as_mut());
    pixmap.encode_png().unwrap()
}
```

### Camera keyframe format

```rust
struct CameraKeyframe {
    x: f32,           // camera center X
    y: f32,           // camera center Y
    zoom: f32,        // 0.1 = satellite, 1.0 = working, 3.0 = macro
    rotation: f32,    // degrees, for orbital
    physics_active: bool,
    // For temporal playback
    version: Option<usize>,
    activation: Option<ActivationState>,
}

// Predefined camera paths
fn satellite_zoom(frames: usize) -> Vec<CameraKeyframe> {
    (0..frames).map(|i| {
        let t = i as f32 / frames as f32;
        CameraKeyframe {
            x: 600.0, y: 400.0,
            zoom: 0.1 + t * 0.9,  // 0.1 → 1.0 over duration
            rotation: 0.0,
            physics_active: false,
            version: None,
            activation: None,
        }
    }).collect()
}

fn drone_follow(path_nodes: &[String], frames_per_node: usize) -> Vec<CameraKeyframe> {
    // Camera follows the inference chain node by node
    // Smooth bezier interpolation between node positions
}

fn orbital_flight(center: (f32, f32), radius: f32, frames: usize) -> Vec<CameraKeyframe> {
    (0..frames).map(|i| {
        let angle = (i as f32 / frames as f32) * 2.0 * std::f32::consts::PI;
        CameraKeyframe {
            x: center.0 + radius * angle.cos(),
            y: center.1 + radius * angle.sin(),
            zoom: 0.8,
            rotation: angle.to_degrees(),
            physics_active: false,
            version: None,
            activation: None,
        }
    }).collect()
}
```

### Playback in cockpit

The prerendered video loads as an `<video>` element overlaid on the Canvas.
Press a "cinematic" button → video plays → fades to live cockpit view.

```typescript
// Cinematic mode
function playCinematic(type: 'satellite' | 'drone' | 'orbital' | 'temporal') {
    const video = document.getElementById('cinematic-overlay') as HTMLVideoElement;
    video.src = `/prerender/${type}.mp4`;
    video.style.display = 'block';
    video.play();
    video.onended = () => {
        video.style.display = 'none';
        // Smooth crossfade to live Canvas
    };
}
```

Or, if the prerender is SVG sequence (vector), animate frame-by-frame
in the Canvas itself — no video element needed, infinite resolution.

### When to prerender

At deploy time. The Dockerfile adds:

```dockerfile
# After building the binary, prerender cinematics
RUN q2 prerender --type satellite --frames 150 --output /opt/prerender/satellite/
RUN q2 prerender --type temporal --frames 900 --output /opt/prerender/temporal/
RUN ffmpeg -framerate 30 -i /opt/prerender/satellite/frame_%05d.png -c:v libx264 -crf 18 /opt/prerender/satellite.mp4
RUN ffmpeg -framerate 30 -i /opt/prerender/temporal/frame_%05d.png -c:v libx264 -crf 18 /opt/prerender/temporal.mp4
```

Build once. Ship the video. Play on demand. Zero runtime GPU cost.

### The Amiga parallel

Amiga demos precomputed copper lists, blitter chains, and sample-accurate
audio timing because the hardware couldn't do it in real time. The output
looked impossible for the platform. That was the point.

We precompute flythrough animations because the browser Canvas can't do
satellite zoom with 221 glowing nodes + particles + activation overlays
at 60fps with smooth camera motion. The output looks like Unity. It's
SVG frames assembled by ffmpeg. That's the trick.

The live cockpit runs at 30fps for interaction. The prerendered cinematics
run at 60fps for presentations. Same data. Same colors. Same graph.
Different render budget.
