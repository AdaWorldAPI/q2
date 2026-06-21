import { useRef, useEffect } from 'react';
import * as THREE from 'three';
import type { StackDiagnosis } from '../../../hooks/useNeuralDiagnosis';

interface ConnectomeModeProps {
  diagnosis: StackDiagnosis;
}

// Brain region positions (normalized -1 to 1)
const REGION_POSITIONS: Record<string, [number, number, number]> = {
  adjacency: [0, -0.8, 0], nars: [0.2, -0.7, 0.1], semiring: [-0.2, -0.7, -0.1],
  spo: [0.1, -0.9, 0], blasgraph: [-0.1, -0.85, 0.1],
  physical: [0, -0.5, -0.6], scan: [0.2, -0.5, -0.5], accumulate: [-0.2, -0.5, -0.5],
  collapse: [0, -0.4, -0.7], cam_pq: [0.3, -0.6, -0.4],
  thinking: [0, 0, 0], qualia: [0.15, 0.05, 0], style: [-0.15, 0.05, 0],
  strategy: [0, 0.4, 0.3], selector: [0.3, 0.3, 0.3], compose: [-0.3, 0.3, 0.3],
  ir: [0.4, 0.2, 0.2], optimize: [-0.4, 0.2, 0.2],
  plan: [0.5, 0.3, 0], execute: [-0.5, 0.3, 0],
  mul: [0, 0.7, 0.5], compass: [0.2, 0.65, 0.4], homeostasis: [-0.2, 0.65, 0.4],
  elevation: [0, 0.8, 0.3],
  hpc: [0.4, -0.3, 0], src: [0, 0, 0.3], tests: [-0.5, 0.5, -0.3], root: [0, 0.5, 0],
};

// State colors matching the spec
const STATE_COLORS = {
  alive: 0x35d07f,    // green glow
  dead: 0xff637d,     // red
  nan: 0xffb547,      // orange
  stub: 0x666666,     // grey
  static: 0x93a9bf,   // dim blue-grey
  healthy: 0x35d07f,
  partial: 0xffb547,
  critical: 0xff637d,
};

function healthState(pct: number): 'healthy' | 'partial' | 'critical' {
  if (pct >= 70) return 'healthy';
  if (pct >= 30) return 'partial';
  return 'critical';
}

function getPosition(moduleName: string): [number, number, number] {
  const lower = moduleName.toLowerCase();
  for (const [key, pos] of Object.entries(REGION_POSITIONS)) {
    if (lower.includes(key)) return pos;
  }
  return [(Math.random() - 0.5) * 1.2, (Math.random() - 0.5) * 1.2, (Math.random() - 0.5) * 0.8];
}

export function ConnectomeMode({ diagnosis }: ConnectomeModeProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!containerRef.current) return;
    const container = containerRef.current;
    let w = container.clientWidth;
    let h = container.clientHeight;
    if (w === 0 || h === 0) return;

    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x0a0e17);
    scene.fog = new THREE.FogExp2(0x0a0e17, 0.25);

    const camera = new THREE.PerspectiveCamera(55, w / h, 0.1, 100);
    camera.position.set(0, 0, 3.5);

    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setSize(w, h);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    container.appendChild(renderer.domElement);

    // Collect neurons (one per module)
    interface NeuronData {
      pos: THREE.Vector3;
      name: string;
      repo: string;
      state: 'healthy' | 'partial' | 'critical';
      health: number;
      total: number;
      dead: number;
      nan: number;
      mesh: THREE.Mesh;
    }
    const neurons: NeuronData[] = [];
    const rings: THREE.Mesh[] = []; // dead-neuron rings that billboard toward camera

    // Create neuron meshes with glow
    const neuronGeom = new THREE.SphereGeometry(1, 16, 12);

    for (const repo of diagnosis.repos) {
      for (const mod of repo.modules) {
        if (mod.total === 0) continue;
        const [x, y, z] = getPosition(mod.name);
        const jitter = () => (Math.random() - 0.5) * 0.12;
        const pos = new THREE.Vector3(x + jitter(), y + jitter(), z + jitter());
        const state = healthState(mod.health_pct);
        const color = STATE_COLORS[state];
        const size = Math.max(0.025, Math.min(0.07, mod.total / 250));

        const mat = new THREE.MeshBasicMaterial({
          color,
          transparent: true,
          opacity: state === 'healthy' ? 0.9 : state === 'partial' ? 0.7 : 0.5,
        });
        const mesh = new THREE.Mesh(neuronGeom, mat);
        mesh.position.copy(pos);
        mesh.scale.setScalar(size);
        scene.add(mesh);

        // Glow halo
        const glowMat = new THREE.MeshBasicMaterial({
          color,
          transparent: true,
          opacity: 0.15,
        });
        const glow = new THREE.Mesh(neuronGeom, glowMat);
        glow.position.copy(pos);
        glow.scale.setScalar(size * 2.5);
        scene.add(glow);

        // Dead neurons get a red ring
        if (mod.dead > 0) {
          const ringGeom = new THREE.RingGeometry(size * 1.8, size * 2.2, 16);
          const ringMat = new THREE.MeshBasicMaterial({
            color: 0xff637d, transparent: true, opacity: 0.6, side: THREE.DoubleSide,
          });
          const ring = new THREE.Mesh(ringGeom, ringMat);
          ring.position.copy(pos);
          ring.lookAt(camera.position);
          scene.add(ring);
          rings.push(ring);
        }

        neurons.push({
          pos, name: mod.name, repo: repo.name, state, health: mod.health_pct,
          total: mod.total, dead: mod.dead, nan: mod.nan_risk, mesh,
        });
      }
    }

    // Axon connections between nearby modules
    const axonPositions: number[] = [];
    const axonColors: number[] = [];
    for (let i = 0; i < neurons.length; i++) {
      for (let j = i + 1; j < neurons.length; j++) {
        const dist = neurons[i].pos.distanceTo(neurons[j].pos);
        // Connect same-repo modules more readily
        const threshold = neurons[i].repo === neurons[j].repo ? 0.6 : 0.35;
        if (dist < threshold) {
          axonPositions.push(
            neurons[i].pos.x, neurons[i].pos.y, neurons[i].pos.z,
            neurons[j].pos.x, neurons[j].pos.y, neurons[j].pos.z,
          );
          const avgHealth = (neurons[i].health + neurons[j].health) / 200;
          const c = new THREE.Color().lerpColors(
            new THREE.Color(0x1a2a3a), new THREE.Color(0x4dd0e1), avgHealth,
          );
          axonColors.push(c.r, c.g, c.b, c.r, c.g, c.b);
        }
      }
    }

    const axonGeom = new THREE.BufferGeometry();
    axonGeom.setAttribute('position', new THREE.Float32BufferAttribute(axonPositions, 3));
    axonGeom.setAttribute('color', new THREE.Float32BufferAttribute(axonColors, 3));
    const axonMat = new THREE.LineBasicMaterial({ vertexColors: true, transparent: true, opacity: 0.2 });
    scene.add(new THREE.LineSegments(axonGeom, axonMat));

    // Brain outline
    const brainGeom = new THREE.SphereGeometry(1.4, 24, 18);
    const brainMat = new THREE.MeshBasicMaterial({
      color: 0x4dd0e1, transparent: true, opacity: 0.02, wireframe: true,
    });
    scene.add(new THREE.Mesh(brainGeom, brainMat));

    // HTML labels overlay (positioned via CSS transforms)
    const labelContainer = document.createElement('div');
    labelContainer.style.cssText = 'position:absolute;inset:0;pointer-events:none;overflow:hidden;';
    container.appendChild(labelContainer);

    const labelEls: { el: HTMLDivElement; neuron: NeuronData }[] = [];
    for (const n of neurons) {
      if (n.total < 5) continue; // skip tiny modules
      const el = document.createElement('div');
      el.style.cssText = `position:absolute;font-size:9px;color:#93a9bf;font-family:monospace;
        white-space:nowrap;transform:translate(-50%,-50%);text-shadow:0 0 4px #0a0e17;`;
      const stateColor = n.state === 'healthy' ? '#35d07f' : n.state === 'partial' ? '#ffb547' : '#ff637d';
      el.innerHTML = `<span style="color:${stateColor}">${n.name}</span>`;
      if (n.dead > 0) el.innerHTML += ` <span style="color:#ff637d;font-size:8px">${n.dead}d</span>`;
      if (n.nan > 0) el.innerHTML += ` <span style="color:#ffb547;font-size:8px">${n.nan}n</span>`;
      labelContainer.appendChild(el);
      labelEls.push({ el, neuron: n });
    }

    // Animation with pulse
    let animId: number;
    let frame = 0;
    function animate() {
      frame++;
      const t = frame * 0.004;

      // Slow orbit
      camera.position.x = Math.sin(t) * 3.5;
      camera.position.z = Math.cos(t) * 3.5;
      camera.position.y = Math.sin(t * 0.3) * 0.8;
      camera.lookAt(0, 0, 0);

      // Pulse neurons
      for (const n of neurons) {
        const baseMat = n.mesh.material as THREE.MeshBasicMaterial;
        const pulse = Math.sin(frame * 0.03 + n.pos.x * 5) * 0.15;
        baseMat.opacity = (n.state === 'healthy' ? 0.85 : 0.55) + pulse;
      }

      // Billboard dead-neuron rings toward camera
      for (const ring of rings) {
        ring.lookAt(camera.position);
      }

      // Update label positions (project 3D → 2D)
      for (const { el, neuron } of labelEls) {
        const vec = neuron.pos.clone().project(camera);
        const x = (vec.x * 0.5 + 0.5) * w;
        const y = (-vec.y * 0.5 + 0.5) * h;
        if (vec.z > 1) {
          el.style.display = 'none';
        } else {
          el.style.display = '';
          el.style.left = x + 'px';
          el.style.top = y + 'px';
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
      window.removeEventListener('resize', handleResize);
      renderer.dispose();
      if (container.contains(renderer.domElement)) container.removeChild(renderer.domElement);
      if (container.contains(labelContainer)) container.removeChild(labelContainer);
    };
  }, [diagnosis]);

  return <div ref={containerRef} className="viz-connectome" style={{ position: 'relative' }} />;
}
