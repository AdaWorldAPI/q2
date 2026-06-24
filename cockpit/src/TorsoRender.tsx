// FMA torso · opaque surfel turntable (the "splat render" page).
//
// A pre-rendered turntable of the OPAQUE surfel render (z-buffer, nearest surface
// wins — no soft-gaussian fog) over the same torso.splat asset the /torso-live page
// orbits live. Frames are rendered bake-side by the scratchpad torso-render driver
// over real BodyParts3D anatomy (FMA-keyed is_a meshes); see crates/osint-bake/tools/
// bake_torso_splat.py. Skin/flesh is gated out so the crisp anatomy shows.
//
// This is the offline companion to /torso-live's live orbit (same geometry); it is
// also the slot for the higher-fidelity CERTIFIED GAUSSIAN render (jc-SPD + Cesium
// SSE/HLOD, the substrate path) when that lands — "opaque now, gaussian later".
//
// Geometry: BodyParts3D, (c) The Database Center for Life Science, CC-BY 4.0.
import { useEffect, useRef, useState } from 'react';

const FRAME_COUNT = 20;
const frameSrc = (i: number) => `/torso-frames/torso_${String(i).padStart(3, '0')}.png`;

interface Manifest {
  attribution: string;
  gaussians: number;
  meshes: number;
  concepts: number;
}

export function TorsoRender() {
  const [frame, setFrame] = useState(0);
  const [loaded, setLoaded] = useState(0);
  const [playing, setPlaying] = useState(true);
  const [manifest, setManifest] = useState<Manifest | null>(null);
  const drag = useRef<{ x: number; f: number } | null>(null);
  const playRef = useRef(true);

  useEffect(() => { playRef.current = playing; }, [playing]);

  // preload all frames
  useEffect(() => {
    let n = 0;
    for (let i = 0; i < FRAME_COUNT; i++) {
      const im = new Image();
      im.onload = () => { n += 1; setLoaded(n); };
      im.src = frameSrc(i);
    }
    fetch('/torso.manifest.json')
      .then((r) => (r.ok ? r.json() : null))
      .then((m) => m && setManifest(m as Manifest))
      .catch(() => {});
  }, []);

  // auto-rotate
  useEffect(() => {
    const id = window.setInterval(() => {
      if (playRef.current) setFrame((f) => (f + 1) % FRAME_COUNT);
    }, 95);
    return () => window.clearInterval(id);
  }, []);

  const onDown = (e: React.PointerEvent) => {
    setPlaying(false);
    drag.current = { x: e.clientX, f: frame };
    (e.target as Element).setPointerCapture?.(e.pointerId);
  };
  const onMove = (e: React.PointerEvent) => {
    if (!drag.current) return;
    const df = Math.round((e.clientX - drag.current.x) / 16);
    setFrame((((drag.current.f - df) % FRAME_COUNT) + FRAME_COUNT) % FRAME_COUNT);
  };
  const onUp = () => { drag.current = null; };

  const ready = loaded >= FRAME_COUNT;

  return (
    <div style={{ position: 'fixed', inset: 0, background: '#0a0e17', overflow: 'hidden', userSelect: 'none' }}>
      <div
        onPointerDown={onDown}
        onPointerMove={onMove}
        onPointerUp={onUp}
        onPointerCancel={onUp}
        onClick={() => drag.current === null && setPlaying((p) => !p)}
        style={{ position: 'absolute', inset: 0, display: 'flex', alignItems: 'center', justifyContent: 'center', cursor: 'ew-resize', touchAction: 'none' }}
      >
        {/* eslint-disable-next-line jsx-a11y/alt-text */}
        <img
          src={frameSrc(frame)}
          draggable={false}
          style={{ maxHeight: '100%', maxWidth: '100%', opacity: ready ? 1 : 0.25, transition: 'opacity 0.3s', imageRendering: 'auto' }}
        />
      </div>

      <div style={{ position: 'absolute', top: 12, left: 16, color: '#cdd9e5', font: '13px ui-monospace, monospace', pointerEvents: 'none' }}>
        <div style={{ fontSize: 15, color: '#fff' }}>FMA torso · opaque surfel turntable</div>
        <div style={{ opacity: 0.7 }}>
          z-buffer surfels (no GPU, no fog)
          {manifest ? ` · ${manifest.gaussians.toLocaleString()} surfels · ${manifest.concepts} structures` : ''}
          {' · real BodyParts3D anatomy'}
        </div>
        <div style={{ opacity: 0.55, marginTop: 2 }}>
          {ready ? `${playing ? 'auto-rotating' : 'paused'} · drag to scrub · click to ${playing ? 'pause' : 'play'}` : `loading frames ${loaded}/${FRAME_COUNT}…`}
        </div>
      </div>

      <div style={{ position: 'absolute', top: 14, right: 16, font: '12px ui-monospace, monospace' }}>
        <a href="/torso-live" style={{ color: '#7fa6c4', textDecoration: 'none' }}>live orbit →</a>
      </div>

      <div style={{ position: 'absolute', bottom: 10, left: 16, color: '#5a6b7e', font: '10px ui-monospace, monospace', maxWidth: '70%', pointerEvents: 'none' }}>
        {manifest?.attribution ?? 'BodyParts3D, (c) The Database Center for Life Science, licensed under CC Attribution 4.0 International'}
      </div>
    </div>
  );
}
