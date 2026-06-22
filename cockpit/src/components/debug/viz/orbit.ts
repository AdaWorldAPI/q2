// Pre-baked camera orbits — demoscene style.
//
// Each orbit is a Float32Array of [x, y, z, lookX, lookY, lookZ] × N frames.
// Computed once at import time (module-level constant). Zero trig at runtime.
// The renderer just indexes: frame % totalFrames → 6 floats → set camera.

export interface CameraFrame {
  x: number; y: number; z: number;
  lx: number; ly: number; lz: number;
}

/** Pre-bake an orbit path. Returns array of camera frames. */
function bakeOrbit(
  frameFn: (t: number) => CameraFrame,
  totalFrames: number,
): CameraFrame[] {
  const frames: CameraFrame[] = new Array(totalFrames);
  for (let i = 0; i < totalFrames; i++) {
    frames[i] = frameFn(i / totalFrames);
  }
  return frames;
}

// ── Render orbit: slow contemplative circle, 3600 frames (~60s at 60fps) ──
export const RENDER_ORBIT = bakeOrbit((t) => {
  const a = t * Math.PI * 2;
  const r = 4.0;
  return {
    x: Math.sin(a) * r,
    y: Math.sin(a * 0.3) * 0.6 + 0.5, // gentle vertical bob
    z: Math.cos(a) * r,
    lx: 0, ly: 0, lz: 0,
  };
}, 3600);

// ── Orbit: tighter path that dips close to each region, 1800 frames (~30s) ──
export const CLOSE_ORBIT = bakeOrbit((t) => {
  const a = t * Math.PI * 2;
  // Elliptical orbit that comes close at 4 cardinal points (one per region).
  const r = 2.5 + Math.sin(a * 4) * 1.2; // pulse in/out 4 times per loop
  const y = Math.sin(a * 2) * 1.0;        // vertical sweep hits top (reasoning) and bottom (memory)
  return {
    x: Math.sin(a) * r,
    y,
    z: Math.cos(a) * r,
    lx: Math.sin(a * 2) * 0.3, // look target drifts toward active region
    ly: y * 0.2,
    lz: Math.cos(a * 2) * 0.3,
  };
}, 1800);

// ── Flight: demoscene flythrough weaving between regions, 3600 frames ──
// Starts far, dives in, threads through perception→memory→reasoning→action→meta,
// pulls back out, loops.
export const FLIGHT_PATH = bakeOrbit((t) => {
  // Phase 1 (0-0.15): approach from far
  // Phase 2 (0.15-0.85): weave through regions
  // Phase 3 (0.85-1.0): pull back to starting position

  if (t < 0.15) {
    // Approach: start far, zoom in
    const p = t / 0.15; // 0→1
    const ease = p * p; // ease-in
    const r = 8.0 - ease * 5.0; // 8 → 3
    return {
      x: Math.sin(p * 0.5) * r,
      y: 2.0 - ease * 1.5,
      z: r,
      lx: 0, ly: 0, lz: 0,
    };
  }

  if (t > 0.85) {
    // Pull back: reverse approach
    const p = (t - 0.85) / 0.15; // 0→1
    const ease = 1.0 - (1.0 - p) * (1.0 - p); // ease-out
    const r = 3.0 + ease * 5.0; // 3 → 8
    return {
      x: Math.sin((1.0 - p) * 0.5) * r,
      y: 0.5 + ease * 1.5,
      z: r,
      lx: 0, ly: 0, lz: 0,
    };
  }

  // Weave phase: figure-8 through the brain regions
  const p = (t - 0.15) / 0.7; // 0→1 within weave phase
  const a = p * Math.PI * 4;   // 2 full loops in the weave

  // Figure-8 (lemniscate of Bernoulli in 3D)
  const scale = 2.2;
  const denom = 1.0 + Math.sin(a) * Math.sin(a);
  const x = scale * Math.cos(a) / denom;
  const z = scale * Math.sin(a) * Math.cos(a) / denom;
  const y = Math.sin(a * 0.5) * 1.2; // vertical weave

  // Look slightly ahead on the path
  const la = a + 0.3;
  const ld = 1.0 + Math.sin(la) * Math.sin(la);
  return {
    x,
    y,
    z,
    lx: scale * Math.cos(la) / ld * 0.5,
    ly: Math.sin(la * 0.5) * 0.6,
    lz: scale * Math.sin(la) * Math.cos(la) / ld * 0.5,
  };
}, 3600);

/** Get frame from a pre-baked orbit, looping. */
export function getFrame(orbit: CameraFrame[], frameIndex: number): CameraFrame {
  return orbit[frameIndex % orbit.length];
}
