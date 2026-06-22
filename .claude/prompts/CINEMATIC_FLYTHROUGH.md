# Cinematic Flythrough — Live VizEngine Camera Paths

## FIRST: Read .claude/prompts/VIZ_ENGINE_V8.md

The VizEngine already runs at 30fps server-side, streaming FrameData via SSE.
Cinematics are NOT prerendered. They're scripted camera keyframes fed to the
same VizEngine tick loop. The cockpit Canvas paints live. Same engine, same
SSE, different camera.

## Camera Keyframe API

Add to viz_kernel.js:

```javascript
globalThis.vizKernel.setCamera = function(x, y, zoom, rotation) {
    // Transform all node positions relative to camera
    // zoom < 1.0 = zoomed out (satellite), zoom > 1.0 = zoomed in (macro)
    cameraX = x;
    cameraY = y;
    cameraZoom = zoom;
    cameraRotation = rotation;
};
```

getFrame() applies camera transform before returning node positions.
The cockpit Canvas doesn't know or care that the camera moved.

## Four Cinematics

### Satellite Zoom (5 seconds)
```javascript
function satelliteZoom(duration) {
    const frames = duration * 30;
    for (let i = 0; i < frames; i++) {
        const t = i / frames;
        vizKernel.setCamera(600, 400, 0.1 + t * 0.9, 0);
        // zoom: 0.1 → 1.0 — cloud resolves to cockpit
    }
}
```

### Drone Follow (10 seconds)
```javascript
function droneFollow(nodePath, duration) {
    const framesPerNode = (duration * 30) / nodePath.length;
    nodePath.forEach((nodeId, idx) => {
        const node = nodes.find(n => n.id === nodeId);
        // Smooth bezier interpolation to next node
        for (let f = 0; f < framesPerNode; f++) {
            const t = f / framesPerNode;
            vizKernel.setCamera(
                lerp(prevNode.x, node.x, t),
                lerp(prevNode.y, node.y, t),
                2.0, 0  // macro zoom on the chain
            );
        }
    });
}
```

### Orbital Flight (8 seconds)
```javascript
function orbitalFlight(centerX, centerY, radius, duration) {
    const frames = duration * 30;
    for (let i = 0; i < frames; i++) {
        const angle = (i / frames) * 2 * Math.PI;
        vizKernel.setCamera(
            centerX + radius * Math.cos(angle),
            centerY + radius * Math.sin(angle),
            0.8,
            angle * (180 / Math.PI)
        );
    }
}
```

### Temporal Growth (30 seconds, with thinking overlay)
Normal temporal playback via StreamingRunner. Camera stays fixed overhead.
Activation overlays do the visual work — no camera tricks needed.

## Cockpit Integration

Add a cinematic mode button to the graph toolbar:

```typescript
const cinematicModes = ['satellite', 'drone', 'orbital', 'temporal'];

function startCinematic(mode: string) {
    fetch('/mcp/message', {
        method: 'POST',
        body: JSON.stringify({
            jsonrpc: '2.0', id: Date.now(),
            method: 'tools/call',
            params: { name: 'cinematic_start', arguments: { mode } }
        })
    });
    // VizEngine handles camera path, SSE streams frames as usual
}
```

## What NOT to do

- Do NOT prerender to PNG or MP4
- Do NOT use ffmpeg or resvg
- Do NOT add build-time prerender to Dockerfile
- Do NOT create a separate rendering pipeline
- Same VizEngine, same SSE, same Canvas. Just camera keyframes.
