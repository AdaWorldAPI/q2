import { useRef, useEffect, useCallback } from 'react';
import * as THREE from 'three';
import type { StackDiagnosis } from '../../../hooks/useNeuralDiagnosis';

interface BrainMriModeProps {
  diagnosis: StackDiagnosis;
}

// ── Brain regions as 3D volumes ─────────────────────────────────────────────
// Each region maps to OSINT pipeline stages. Position in 3D space mirrors
// functional brain anatomy: perception at the back, action at the front,
// memory below, reasoning above.

interface BrainRegionDef {
  name: string;
  keys: string[];           // OSINT pipeline stage keys
  position: [number, number, number];
  shape: 'sphere' | 'torus' | 'cone' | 'icosahedron' | 'octahedron' | 'dodecahedron';
  baseSize: number;
  // Color temperature gradient: idle → active → peak
  colorIdle: number;
  colorActive: number;
  colorPeak: number;
}

const REGIONS: BrainRegionDef[] = [
  // ── Perception (posterior, cool blues) ──
  {
    name: 'Perception',
    keys: ['extraction', 'xai_api_call'],
    position: [0, 0, -1.2],
    shape: 'cone',
    baseSize: 0.3,
    colorIdle: 0x0a1628,     // deep navy
    colorActive: 0x00d4ff,   // cyan
    colorPeak: 0xffffff,     // white flash on API call
  },
  // ── Memory (inferior, warm greens) ──
  {
    name: 'Memory',
    keys: ['episodic_store', 'episodic_retrieve', 'graph_bfs', 'spatial_path'],
    position: [0, -0.9, 0],
    shape: 'torus',
    baseSize: 0.35,
    colorIdle: 0x0a2818,     // deep forest
    colorActive: 0x35d07f,   // emerald
    colorPeak: 0x88ffcc,     // bright mint
  },
  // ── Reasoning (superior, hot ambers) ──
  {
    name: 'Reasoning',
    keys: ['deduction', 'contradiction', 'revision'],
    position: [0, 0.9, 0],
    shape: 'icosahedron',
    baseSize: 0.35,
    colorIdle: 0x281a0a,     // deep amber
    colorActive: 0xffb547,   // gold
    colorPeak: 0xff4400,     // hot orange (peak inference)
  },
  // ── Action (anterior, vivid magentas) ──
  {
    name: 'Action',
    keys: ['refinement', 'planning', 'classification'],
    position: [0, 0, 1.2],
    shape: 'octahedron',
    baseSize: 0.3,
    colorIdle: 0x1a0a28,     // deep purple
    colorActive: 0xe040fb,   // magenta
    colorPeak: 0xff88ff,     // bright pink
  },
  // ── Meta (center-top, white/gold = MUL awareness) ──
  {
    name: 'Meta',
    keys: ['planning'],  // MUL doesn't have its own counter yet — proxy via planning
    position: [0, 0.3, 0],
    shape: 'dodecahedron',
    baseSize: 0.2,
    colorIdle: 0x111111,     // near-black
    colorActive: 0xaaaaaa,   // silver
    colorPeak: 0xffd700,     // gold (full meta-awareness)
  },
];

// ── Exponential decay parameters ──
const DECAY = 0.93;
const SPIKE_GAIN = 0.12;
const GLOW_RADIUS_MIN = 0.1;
const GLOW_RADIUS_MAX = 0.8;

// ── Color temperature mapping ──
// Maps orchestrator temperature (0-1) to a global color shift.
// 0.0 = cool (blues dominate), 0.5 = neutral, 1.0 = hot (reds dominate)
function temperatureShift(baseColor: THREE.Color, temp: number): THREE.Color {
  const warm = new THREE.Color(0xff4400);
  const cool = new THREE.Color(0x0044ff);
  const tint = temp > 0.5 ? warm : cool;
  const strength = Math.abs(temp - 0.5) * 0.3; // subtle shift
  return baseColor.clone().lerp(tint, strength);
}

export function BrainMriMode({ diagnosis }: BrainMriModeProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const signalsRef = useRef<Float64Array>(new Float64Array(REGIONS.length));
  const prevCountersRef = useRef<Map<string, number>>(new Map());
  const temperatureRef = useRef(0.0);

  // Poll OSINT pipeline for real counter deltas.
  const pollCounters = useCallback(async () => {
    try {
      const resp = await fetch('/api/debug/osint');
      if (!resp.ok) return;
      const data = await resp.json();
      const stages: [string, { calls: number }][] = data?.pipeline?.stages || [];
      const stageMap = new Map<string, number>();
      for (const [name, snap] of stages) {
        const prev = prevCountersRef.current.get(name) || 0;
        const delta = Math.max(0, (snap?.calls || 0) - prev);
        stageMap.set(name, delta);
        prevCountersRef.current.set(name, snap?.calls || 0);
      }
      // Map deltas to regions.
      for (let r = 0; r < REGIONS.length; r++) {
        let totalDelta = 0;
        for (const key of REGIONS[r].keys) {
          totalDelta += stageMap.get(key) || 0;
        }
        if (totalDelta > 0) {
          signalsRef.current[r] = Math.min(1.0, signalsRef.current[r] + totalDelta * SPIKE_GAIN);
        }
      }
    } catch { /* silent */ }

    // Also poll orchestrator temperature.
    try {
      const resp = await fetch('/api/orchestrator/status');
      if (resp.ok) {
        const data = await resp.json();
        temperatureRef.current = data?.temperature ?? data?.mul?.free_will_modifier ?? 0.0;
      }
    } catch { /* silent */ }
  }, []);

  useEffect(() => {
    if (!containerRef.current) return;
    const container = containerRef.current;
    let w = container.clientWidth;
    let h = container.clientHeight;
    if (w === 0 || h === 0) return;

    // ── Three.js setup ──
    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x050810);

    const camera = new THREE.PerspectiveCamera(50, w / h, 0.1, 100);
    camera.position.set(0, 0, 4);

    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setSize(w, h);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    container.appendChild(renderer.domElement);

    // ── Brain outline (translucent shell) ──
    const brainGeom = new THREE.SphereGeometry(1.6, 32, 24);
    const brainMat = new THREE.MeshBasicMaterial({
      color: 0x4dd0e1, transparent: true, opacity: 0.015, wireframe: true,
    });
    scene.add(new THREE.Mesh(brainGeom, brainMat));

    // ── Create region meshes ──
    interface RegionMesh {
      core: THREE.Mesh;
      glow: THREE.Mesh;
      def: BrainRegionDef;
      coreColor: THREE.Color;
      glowColor: THREE.Color;
    }
    const regionMeshes: RegionMesh[] = [];

    for (const def of REGIONS) {
      let geom: THREE.BufferGeometry;
      switch (def.shape) {
        case 'torus':
          geom = new THREE.TorusGeometry(def.baseSize, def.baseSize * 0.35, 12, 24);
          break;
        case 'cone':
          geom = new THREE.ConeGeometry(def.baseSize, def.baseSize * 1.5, 16);
          break;
        case 'icosahedron':
          geom = new THREE.IcosahedronGeometry(def.baseSize, 1);
          break;
        case 'octahedron':
          geom = new THREE.OctahedronGeometry(def.baseSize, 0);
          break;
        case 'dodecahedron':
          geom = new THREE.DodecahedronGeometry(def.baseSize, 0);
          break;
        default:
          geom = new THREE.SphereGeometry(def.baseSize, 16, 12);
      }

      const coreMat = new THREE.MeshBasicMaterial({
        color: def.colorIdle, transparent: true, opacity: 0.6,
      });
      const core = new THREE.Mesh(geom, coreMat);
      core.position.set(...def.position);
      scene.add(core);

      // Glow volume — larger, more transparent.
      const glowGeom = new THREE.SphereGeometry(def.baseSize * 2, 16, 12);
      const glowMat = new THREE.MeshBasicMaterial({
        color: def.colorActive, transparent: true, opacity: 0.0,
      });
      const glow = new THREE.Mesh(glowGeom, glowMat);
      glow.position.set(...def.position);
      scene.add(glow);

      regionMeshes.push({
        core, glow, def,
        coreColor: new THREE.Color(def.colorIdle),
        glowColor: new THREE.Color(def.colorActive),
      });
    }

    // ── Axon connections between regions ──
    const axonPositions: number[] = [];
    for (let i = 0; i < REGIONS.length; i++) {
      for (let j = i + 1; j < REGIONS.length; j++) {
        const a = REGIONS[i].position;
        const b = REGIONS[j].position;
        axonPositions.push(a[0], a[1], a[2], b[0], b[1], b[2]);
      }
    }
    const axonGeom = new THREE.BufferGeometry();
    axonGeom.setAttribute('position', new THREE.Float32BufferAttribute(axonPositions, 3));
    const axonMat = new THREE.LineBasicMaterial({
      color: 0x1a2a3a, transparent: true, opacity: 0.15,
    });
    const axonLines = new THREE.LineSegments(axonGeom, axonMat);
    scene.add(axonLines);

    // ── HTML labels ──
    const labelContainer = document.createElement('div');
    labelContainer.style.cssText = 'position:absolute;inset:0;pointer-events:none;overflow:hidden;';
    container.appendChild(labelContainer);

    const labelEls: { el: HTMLDivElement; idx: number }[] = [];
    for (let i = 0; i < REGIONS.length; i++) {
      const el = document.createElement('div');
      el.style.cssText = `position:absolute;font:bold 10px monospace;color:#93a9bf;
        white-space:nowrap;transform:translate(-50%,-120%);text-shadow:0 0 6px #050810;
        transition:color 0.3s;`;
      el.textContent = REGIONS[i].name;
      labelContainer.appendChild(el);
      labelEls.push({ el, idx: i });
    }

    // ── Poll timer ──
    const pollTimer = setInterval(pollCounters, 500);

    // ── Animation loop ──
    let animId: number;
    let frame = 0;

    function animate() {
      frame++;
      const t = frame * 0.003;
      const signals = signalsRef.current;
      const temp = temperatureRef.current;

      // Slow orbit.
      camera.position.x = Math.sin(t) * 4;
      camera.position.z = Math.cos(t) * 4;
      camera.position.y = Math.sin(t * 0.4) * 1.0;
      camera.lookAt(0, 0, 0);

      // Update each region.
      for (let r = 0; r < regionMeshes.length; r++) {
        // Decay signal.
        signals[r] = signals[r] * DECAY;
        const s = signals[r];
        const rm = regionMeshes[r];
        const def = rm.def;

        // Color: lerp idle → active → peak based on signal.
        const idle = new THREE.Color(def.colorIdle);
        const active = new THREE.Color(def.colorActive);
        const peak = new THREE.Color(def.colorPeak);

        let color: THREE.Color;
        if (s < 0.5) {
          color = idle.clone().lerp(active, s * 2);
        } else {
          color = active.clone().lerp(peak, (s - 0.5) * 2);
        }
        // Apply orchestrator temperature shift.
        color = temperatureShift(color, temp);

        const coreMat = rm.core.material as THREE.MeshBasicMaterial;
        coreMat.color.copy(color);
        coreMat.opacity = 0.4 + s * 0.6;

        // Glow: radius and opacity scale with signal.
        const glowMat = rm.glow.material as THREE.MeshBasicMaterial;
        glowMat.color.copy(color);
        glowMat.opacity = s * 0.25;
        const glowScale = GLOW_RADIUS_MIN + s * (GLOW_RADIUS_MAX - GLOW_RADIUS_MIN);
        rm.glow.scale.setScalar(glowScale / (def.baseSize * 2));

        // Pulse: subtle breathing when active.
        const pulse = s > 0.1 ? Math.sin(frame * 0.05 + r) * 0.05 * s : 0;
        rm.core.scale.setScalar(1 + pulse);

        // Rotate shapes slowly when active (shows they're "working").
        if (s > 0.05) {
          rm.core.rotation.y += s * 0.01;
          rm.core.rotation.x += s * 0.005;
        }
      }

      // Axon brightness: brighter when connected regions co-activate.
      let maxCoActivation = 0;
      let axonIdx = 0;
      const axonColors = new Float32Array(axonPositions.length);
      for (let i = 0; i < REGIONS.length; i++) {
        for (let j = i + 1; j < REGIONS.length; j++) {
          const coAct = Math.min(signals[i], signals[j]);
          maxCoActivation = Math.max(maxCoActivation, coAct);
          const c = new THREE.Color().lerpColors(
            new THREE.Color(0x0a1628),
            new THREE.Color(0x4dd0e1),
            coAct * 2,
          );
          // Each line segment = 2 vertices × 3 components.
          axonColors[axonIdx * 6 + 0] = c.r;
          axonColors[axonIdx * 6 + 1] = c.g;
          axonColors[axonIdx * 6 + 2] = c.b;
          axonColors[axonIdx * 6 + 3] = c.r;
          axonColors[axonIdx * 6 + 4] = c.g;
          axonColors[axonIdx * 6 + 5] = c.b;
          axonIdx++;
        }
      }
      axonGeom.setAttribute('color', new THREE.Float32BufferAttribute(axonColors, 3));
      (axonLines.material as THREE.LineBasicMaterial).vertexColors = true;
      (axonLines.material as THREE.LineBasicMaterial).opacity = 0.1 + maxCoActivation * 0.5;

      // Update label positions.
      for (const { el, idx } of labelEls) {
        const pos = new THREE.Vector3(...REGIONS[idx].position);
        pos.project(camera);
        const x = (pos.x * 0.5 + 0.5) * w;
        const y = (-pos.y * 0.5 + 0.5) * h;
        if (pos.z > 1) {
          el.style.display = 'none';
        } else {
          el.style.display = '';
          el.style.left = x + 'px';
          el.style.top = y + 'px';
          // Label color matches region activation.
          const s = signals[idx];
          const def = REGIONS[idx];
          if (s > 0.3) {
            const c = new THREE.Color(def.colorActive);
            el.style.color = `#${c.getHexString()}`;
          } else {
            el.style.color = '#93a9bf';
          }
        }
      }

      renderer.render(scene, camera);
      animId = requestAnimationFrame(animate);
    }
    animate();

    const handleResize = () => {
      w = container.clientWidth;
      h = container.clientHeight;
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      renderer.setSize(w, h);
    };
    window.addEventListener('resize', handleResize);

    return () => {
      cancelAnimationFrame(animId);
      clearInterval(pollTimer);
      window.removeEventListener('resize', handleResize);
      renderer.dispose();
      if (container.contains(renderer.domElement)) container.removeChild(renderer.domElement);
      if (container.contains(labelContainer)) container.removeChild(labelContainer);
    };
  }, [diagnosis, pollCounters]);

  return (
    <div
      ref={containerRef}
      className="viz-brain-mri"
      style={{ width: '100%', height: '100%', position: 'relative' }}
    />
  );
}
