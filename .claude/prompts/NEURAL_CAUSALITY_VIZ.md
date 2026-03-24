# Neural Causality Graph — The Closer

## What It Looks Like

A live animated neural network visualization in the cockpit.
Not a heatmap bar. Not a progress indicator. A BRAIN THINKING.

```
┌─────────────────────────────────────────────────────────┐
│                    COGNITION TRACE                       │
│                                                         │
│        ╭──╮                                             │
│    ●━━▶│L1│━━━━●                                        │
│   input╰──╯    │    ╭──╮         ╭──╮                   │
│                ├━━▶│L2│━━━●━━━▶│L3│━━━╮                │
│                │   ╰──╯   │     ╰──╯   │               │
│                │    fp     │   cascade  │               │
│                │           │            │               │
│                │       ╭───╯     FOVEAL?│               │
│                │       │         ╱    ╲ │               │
│                │       │        ╱      ╲│               │
│                │    ╭──╮      ╱     ╭──╮│    ╭──╮       │
│                │    │L5│◀━━━╱      │L6│◀━━━▶│L7│       │
│                │    ╰──╯ reason    ╰──╯     ╰──╯       │
│                │    ████████       memory    plan       │
│                │    ACTIVE         ██         │         │
│                │       │         STAUNEN!     │         │
│                │       │         ★ ★ ★        │         │
│                │       ╰━━━━━━╮    │    ╭━━━━╯         │
│                │              ▼    ▼    ▼               │
│                │           ╭──╮  ╭──╮  ╭──╮  ╭───╮     │
│                │           │L8│━▶│L9│━▶│10│━▶│out│     │
│                │           ╰──╯  ╰──╯  ╰──╯  ╰───╯     │
│                │           action  gen   meta           │
│                │                                        │
│  ● = signal particle (animated along edges)             │
│  ████ = layer activation intensity                      │
│  ★ = NARS epiphany (inferred edge pops into existence)  │
│  dendrite growth = new connection appears during Staunen│
└─────────────────────────────────────────────────────────┘
```

## The Five Visual Primitives

### 1. Signal Particles (information flow)

Glowing dots that travel along edges between layers.
Speed = processing time. Brightness = data volume.

```javascript
// Particle moves from L1 to L2 along the edge
particle = {
  edge: [L1, L2],
  progress: 0.0 → 1.0,  // animated over 200ms
  color: '#00bcd4',       // teal
  radius: 3,
  glow: true,
  trail: 0.3,             // fading trail behind particle
}
```

When Layer 1 completes, a particle launches toward Layer 2.
When Layer 2 completes, particles fan out toward Layer 3.
Multiple particles can be in flight simultaneously.

### 2. Layer Nodes (activation pulses)

Each layer is a circle that PULSES when active.

```
Idle:     dim outline, dark fill
Running:  expanding ring animation (like sonar), bright fill
Complete: solid fill, intensity = processing time
Skipped:  hollow, dashed outline (Foveal skip path)
```

Foveal pattern: L1 → L2 → L3 bright. Then L5 goes DASHED (skipped).
Signal particle takes the shortcut edge directly to L6.
The visual difference between "I know this" (skip) and "I need to think"
(full path) is immediately obvious.

Parafoveal pattern: L1 → L2 → L3 → L5 all bright. L5 burns longest
(semiring reasoning). L6 pulses with amber if Staunen.

### 3. Staunen Dendrite Growth (surprise = new connections)

When Layer 6 detects Staunen (new learning), the visualization
GROWS NEW DENDRITES:

```javascript
// A new edge materializes between two existing nodes
dendrite = {
  from: L6,          // memory consolidation
  to: L7,            // triggers replanning
  growthDuration: 600, // ms to fully appear
  style: {
    start: { opacity: 0, width: 0, color: '#ffc107' },  // amber
    end:   { opacity: 1, width: 2, color: '#ffc107' },
  },
  // The edge literally grows from L6 toward L7
  // like a neural dendrite extending
}
```

The animation: a thin amber line GROWS outward from L6,
reaching toward L7. When it connects, L7 pulses (replanning triggered).
This is the "brain making a new connection" cliché — but earned,
because it's showing REAL graph topology change.

Wisdom (no new learning): no new dendrites. The existing connections
pulse gently. Stability.

### 4. NARS Epiphanies (inferred edges popping into existence)

When semiring reasoning (L5) or NARS revision (L6) produces an
inferred edge, it appears as a BURST:

```javascript
epiphany = {
  // In the MAIN graph panel (not the cognition trace)
  sourceNode: 'Palantir',
  targetNode: 'Gotham',
  inferredLabel: 'USES',
  truth: { f: 0.86, c: 0.71 },
  
  // Animation
  appearAnimation: {
    // 1. Flash: bright teal circle expands from midpoint
    flash: { radius: 0 → 40, opacity: 1 → 0, duration: 300 },
    // 2. Edge materializes as dashed line
    edge: { dashOffset: 100 → 0, opacity: 0 → 0.7, duration: 400 },
    // 3. Label fades in
    label: { opacity: 0 → 1, duration: 200 },
    // 4. Truth badge appears
    badge: { scale: 0 → 1, duration: 150 },
  }
}
```

The sequence: FLASH at the midpoint → dashed line grows outward
in both directions → label fades in → small badge shows `f:0.86 c:0.71`.

Multiple epiphanies during a reasoning burst create a "popcorn" effect
— connections popping into existence across the graph.

If a later version CONFIRMS the inference (truth gets revised UP),
the dashed line solidifies with a brief glow.
If CONTRADICTED (truth revised DOWN), the line fades with a red flash.

### 5. PET Scan Heatmap (accumulated trace)

Below the neural graph, a horizontal heatmap strip accumulates
across versions:

```
Version:  v00  v01  v31  v32  v33  v34  v35  ... v42
L1:       ██   ██   ██   ██   ██   ██   ██       ██
L2:       ██   ██   ██   ██   ██   ██   ██       ██
L3:       ██   ██   ██   ██   ██   ██   ██       ██
L5:       ··   ██   ████ ████ ██   ··   ··       ··
L6:       ██   ██   ██   ██   ██   ██   ██       ██
L7:       ··   ··   ████ ██   ··   ··   ··       ··
L8:       ██   ██   ██   ██   ██   ██   ██       ██
L9:       ██   ██   ██   ██   ██   ██   ██       ██
L10:      ██   ██   ██   ██   ██   ██   ██       ██
          ─────────────────────────────────────────
Seal:     S    S    S    S    S    W    W        W
```

S = Staunen (amber), W = Wisdom (green), ·· = skipped (dim)
Intensity = processing time at that layer for that version.

The pattern tells a story: v31-v33 light up Layer 5 and 7 heavily
(Epstein enrichment triggers reasoning and replanning). v35 onward
calms down (Wisdom — the graph stabilized). You can SEE where the
graph "learned the most."

## Implementation

### Technology

Canvas + requestAnimationFrame for particles and animations.
SVG for the layer topology (static structure).
CSS animations for pulses and flashes.
No three.js needed — 2D is more readable for this.

### Data Source

StreamingRunner from rs-graph-llm emits StreamChunks:

```rust
enum StreamChunk {
    LayerStarted { layer_id: String, version: usize },
    LayerCompleted { layer_id: String, elapsed_ms: u64, version: usize },
    LayerSkipped { layer_id: String, reason: String },
    InferredEdge { source: String, target: String, truth: TruthValue },
    SealStatus { version: usize, status: GraphSealStatus },
    DendriteGrowth { from_layer: String, to_layer: String },
}
```

The cockpit component subscribes to SSE and dispatches animations:

```typescript
sse.onmessage = (event) => {
  const chunk = JSON.parse(event.data);
  switch (chunk.type) {
    case 'LayerStarted':
      cogGraph.activateNode(chunk.layer_id);
      break;
    case 'LayerCompleted':
      cogGraph.completeNode(chunk.layer_id, chunk.elapsed_ms);
      cogGraph.launchParticle(chunk.layer_id, nextLayer(chunk.layer_id));
      break;
    case 'LayerSkipped':
      cogGraph.skipNode(chunk.layer_id);
      cogGraph.launchParticle(prevLayer(chunk.layer_id), skipTarget(chunk.layer_id));
      break;
    case 'InferredEdge':
      mainGraph.showEpiphany(chunk.source, chunk.target, chunk.truth);
      break;
    case 'SealStatus':
      heatmap.addColumn(chunk.version, chunk.status);
      if (chunk.status === 'Staunen') {
        cogGraph.growDendrite('memory_consolidate', 'planning');
      }
      break;
  }
};
```

### Cockpit Layout Change

The cognition trace replaces the left rail (or shares it as a tab):

```
┌──────────┬──────────────────┬──────────┐
│ COGNITION│     GRAPH        │INSPECTOR │
│  TRACE   │   (main data)    │          │
│          │                  │          │
│ [neural  │  [force-directed │ [node    │
│  anim]   │   with epiphany  │  props]  │
│          │   bursts]        │          │
│          │                  │          │
│ [PET     │                  │          │
│  heatmap]│                  │          │
├──────────┴──────────────────┴──────────┤
│              RESULT TABLE              │
├────────────────────────────────────────┤
│              CELL STRIP                │
└────────────────────────────────────────┘
```

Or as a toggleable overlay on the graph panel itself (press `T` for
trace mode — the neural graph fades in on top of the data graph
with 50% opacity, showing the thinking process overlaid on the data).

### Component: CognitionTrace.tsx

```typescript
interface CognitionTraceProps {
  // Layer topology (static, from thinking graph definition)
  layers: LayerDef[];
  edges: LayerEdge[];
  conditionalEdges: ConditionalEdge[];
  
  // Live state (from SSE)
  activeLayer: string | null;
  completedLayers: Map<string, { elapsed_ms: number }>;
  skippedLayers: Set<string>;
  particles: Particle[];
  dendrites: Dendrite[];
  epiphanies: Epiphany[];
  sealHistory: SealEntry[];
}
```

### Animation Constants

```css
:root {
  --particle-speed: 200ms;
  --pulse-duration: 600ms;
  --dendrite-growth: 600ms;
  --epiphany-flash: 300ms;
  --epiphany-edge: 400ms;
  --trail-fade: 150ms;
  
  --color-foveal: #00bcd4;      /* teal — familiar, fast */
  --color-parafoveal: #7c4dff;  /* purple — novel, reasoning */
  --color-staunen: #ffc107;     /* amber — surprise, growth */
  --color-wisdom: #4caf50;      /* green — stable, known */
  --color-epiphany: #00e5ff;    /* bright cyan — new inference */
  --color-contradiction: #ff1744; /* red — inference disproven */
}
```

## The 10-Second Demo Script

```
0s:   Cockpit loads. Cognition trace panel shows 10 idle layer nodes.
      Main graph shows base aiwar data (221 nodes).
      Play button at bottom: [▶ ⏸] with version timeline.

1s:   Press play. v00 loads.
      L1 pulses → particle launches → L2 pulses → L3 pulses.
      L3 classifies: "Foveal" (familiar base data).
      L5 goes DASHED (skipped). Particle takes shortcut to L6.
      L6: Wisdom (green). No new learning. Clean pass.
      PET heatmap: first column, all green.

3s:   v01 enrichment arrives.
      L1 → L2 → L3: "Parafoveal" (novel data!).
      Particle takes the LONG path through L5.
      L5 burns bright purple — semiring reasoning running.
      Three epiphany bursts in the main graph:
        FLASH → dashed line → "CONNECTED_TO" → f:0.78 c:0.65
        FLASH → dashed line → "DEVELOPED_BY" → f:0.85 c:0.72
        FLASH → dashed line → "DEPLOYED_BY"  → f:0.90 c:0.80
      L6: STAUNEN! Amber dendrite GROWS from L6 → L7.
      L7 pulses (replanning). L8 → L9 → L10.
      PET heatmap: second column, L5 bright, L7 bright.

5s:   v31 (Epstein enrichment) arrives.
      L3: "Parafoveal" — this is VERY novel.
      L5 BURNS — longest activation yet. Multiple inference chains.
      SIX epiphany bursts in quick succession (popcorn effect).
      The audience watches connections literally pop into existence.
      L6: STAUNEN again. Another dendrite grows.
      One of the v01 inferred edges gets CONFIRMED — 
      dashed line solidifies with a glow. truth: c:0.65 → c:0.78.

7s:   v35 arrives. L3: still Parafoveal but calmer.
      L5 runs but shorter. Two epiphanies.
      One v31 inference gets CONTRADICTED —
      dashed line flashes RED and fades. Dead end.
      L6: Wisdom. No new dendrites. Stabilizing.

9s:   v42 arrives. L3: "Foveal" — we've seen this pattern.
      L5 SKIPPED (dashed). Fast path.
      L6: Wisdom. Green column in heatmap.
      The graph has learned everything the data has to offer.
      Status bar: "42 versions · 23 inferred · 19 confirmed · 4 contradicted"

10s:  Play stops. The neural graph dims to a gentle idle pulse.
      The PET heatmap tells the whole story:
      early versions = lots of reasoning (purple L5).
      middle versions = surprises and replanning (amber L6/L7).
      late versions = calm, Foveal, Wisdom (green, L5 skipped).
      The graph learned, stabilized, and is now WISE.
```

That's the closer. The room doesn't need to understand NARS or
semirings or ZeckF64. They see a brain thinking. They see it
surprised. They see it making connections. They see it calm down.
They see it become wise.
