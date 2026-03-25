import { useRef, useEffect } from 'react';
import * as THREE from 'three';
import type { StackDiagnosis, ModuleDiagnosis } from '../../../hooks/useNeuralDiagnosis';

interface ConnectomeModeProps {
  diagnosis: StackDiagnosis;
}

// Brain region positions (normalized -1 to 1)
const REGION_POSITIONS: Record<string, [number, number, number]> = {
  // Brainstem (bottom center)
  adjacency: [0, -0.8, 0], nars: [0.2, -0.7, 0.1], semiring: [-0.2, -0.7, -0.1],
  spo: [0.1, -0.9, 0], blasgraph: [-0.1, -0.85, 0.1],
  // Cerebellum (back bottom)
  physical: [0, -0.5, -0.6], scan: [0.2, -0.5, -0.5], accumulate: [-0.2, -0.5, -0.5],
  collapse: [0, -0.4, -0.7], cam_pq: [0.3, -0.6, -0.4],
  // Limbic (center)
  thinking: [0, 0, 0], qualia: [0.15, 0.05, 0], style: [-0.15, 0.05, 0],
  felt_parse: [0, -0.1, 0.1],
  // Cortex (outer surface)
  strategy: [0, 0.4, 0.3], selector: [0.3, 0.3, 0.3], compose: [-0.3, 0.3, 0.3],
  ir: [0.4, 0.2, 0.2], optimize: [-0.4, 0.2, 0.2],
  plan: [0.5, 0.3, 0], execute: [-0.5, 0.3, 0],
  // Prefrontal (front top)
  mul: [0, 0.7, 0.5], compass: [0.2, 0.65, 0.4], homeostasis: [-0.2, 0.65, 0.4],
  elevation: [0, 0.8, 0.3],
  // Default positions for unmapped
  src: [0, 0, 0.3], hpc: [0.4, -0.3, 0], tests: [-0.5, 0.5, -0.3],
  root: [0, 0.5, 0],
};

function healthColor(pct: number): THREE.Color {
  if (pct >= 70) return new THREE.Color(0x35d07f);
  if (pct >= 30) return new THREE.Color(0xffb547);
  return new THREE.Color(0xff637d);
}

function getPosition(moduleName: string): [number, number, number] {
  const lower = moduleName.toLowerCase();
  for (const [key, pos] of Object.entries(REGION_POSITIONS)) {
    if (lower.includes(key)) return pos;
  }
  // Random position in brain volume
  return [
    (Math.random() - 0.5) * 1.2,
    (Math.random() - 0.5) * 1.2,
    (Math.random() - 0.5) * 0.8,
  ];
}

export function ConnectomeMode({ diagnosis }: ConnectomeModeProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const sceneRef = useRef<{
    scene: THREE.Scene;
    camera: THREE.PerspectiveCamera;
    renderer: THREE.WebGLRenderer;
    particles: THREE.Points;
    axons: THREE.LineSegments;
  } | null>(null);
  const frameRef = useRef(0);

  useEffect(() => {
    if (!containerRef.current) return;

    const container = containerRef.current;
    const w = container.clientWidth;
    const h = container.clientHeight;

    // Scene setup
    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x0a0e17);
    scene.fog = new THREE.FogExp2(0x0a0e17, 0.3);

    const camera = new THREE.PerspectiveCamera(60, w / h, 0.1, 100);
    camera.position.set(0, 0, 3);

    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setSize(w, h);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    container.appendChild(renderer.domElement);

    // Collect all modules as neurons
    const neurons: { pos: THREE.Vector3; color: THREE.Color; size: number; health: number }[] = [];
    const connections: [THREE.Vector3, THREE.Vector3][] = [];

    for (const repo of diagnosis.repos) {
      for (const mod of repo.modules) {
        if (mod.total === 0) continue;
        const [x, y, z] = getPosition(mod.name);
        const jitter = () => (Math.random() - 0.5) * 0.15;
        const pos = new THREE.Vector3(x + jitter(), y + jitter(), z + jitter());
        const color = healthColor(mod.health_pct);
        const size = Math.max(0.02, Math.min(0.08, mod.total / 200));
        neurons.push({ pos, color, size, health: mod.health_pct });
      }
    }

    // Create neuron point cloud
    const positions = new Float32Array(neurons.length * 3);
    const colors = new Float32Array(neurons.length * 3);
    const sizes = new Float32Array(neurons.length);

    neurons.forEach((n, i) => {
      positions[i * 3] = n.pos.x;
      positions[i * 3 + 1] = n.pos.y;
      positions[i * 3 + 2] = n.pos.z;
      colors[i * 3] = n.color.r;
      colors[i * 3 + 1] = n.color.g;
      colors[i * 3 + 2] = n.color.b;
      sizes[i] = n.size;
    });

    const pointGeom = new THREE.BufferGeometry();
    pointGeom.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    pointGeom.setAttribute('color', new THREE.BufferAttribute(colors, 3));
    pointGeom.setAttribute('size', new THREE.BufferAttribute(sizes, 1));

    const pointMat = new THREE.PointsMaterial({
      size: 0.06,
      vertexColors: true,
      transparent: true,
      opacity: 0.9,
      sizeAttenuation: true,
    });
    const particles = new THREE.Points(pointGeom, pointMat);
    scene.add(particles);

    // Create axon connections (connect nearby neurons)
    const axonPositions: number[] = [];
    const axonColors: number[] = [];
    for (let i = 0; i < neurons.length; i++) {
      for (let j = i + 1; j < neurons.length; j++) {
        const dist = neurons[i].pos.distanceTo(neurons[j].pos);
        if (dist < 0.5) {
          axonPositions.push(
            neurons[i].pos.x, neurons[i].pos.y, neurons[i].pos.z,
            neurons[j].pos.x, neurons[j].pos.y, neurons[j].pos.z,
          );
          const avgHealth = (neurons[i].health + neurons[j].health) / 200;
          const c = new THREE.Color().lerpColors(
            new THREE.Color(0x1a2a3a),
            new THREE.Color(0x4dd0e1),
            avgHealth,
          );
          axonColors.push(c.r, c.g, c.b, c.r, c.g, c.b);
        }
      }
    }

    const axonGeom = new THREE.BufferGeometry();
    axonGeom.setAttribute('position', new THREE.Float32BufferAttribute(axonPositions, 3));
    axonGeom.setAttribute('color', new THREE.Float32BufferAttribute(axonColors, 3));
    const axonMat = new THREE.LineBasicMaterial({
      vertexColors: true,
      transparent: true,
      opacity: 0.25,
    });
    const axons = new THREE.LineSegments(axonGeom, axonMat);
    scene.add(axons);

    // Brain outline (translucent sphere)
    const brainGeom = new THREE.SphereGeometry(1.3, 32, 24);
    const brainMat = new THREE.MeshBasicMaterial({
      color: 0x4dd0e1,
      transparent: true,
      opacity: 0.03,
      wireframe: true,
    });
    scene.add(new THREE.Mesh(brainGeom, brainMat));

    sceneRef.current = { scene, camera, renderer, particles, axons };

    // Animation
    let animId: number;
    function animate() {
      frameRef.current++;
      const t = frameRef.current * 0.005;

      // Rotate brain slowly
      camera.position.x = Math.sin(t) * 3;
      camera.position.z = Math.cos(t) * 3;
      camera.position.y = Math.sin(t * 0.3) * 0.5;
      camera.lookAt(0, 0, 0);

      // Pulse neuron sizes
      const sizeAttr = pointGeom.getAttribute('size') as THREE.BufferAttribute;
      for (let i = 0; i < neurons.length; i++) {
        const base = neurons[i].size;
        const pulse = Math.sin(t * 2 + i * 0.5) * 0.01;
        sizeAttr.array[i] = base + pulse;
      }
      sizeAttr.needsUpdate = true;

      renderer.render(scene, camera);
      animId = requestAnimationFrame(animate);
    }
    animate();

    // Resize handler
    const handleResize = () => {
      const w = container.clientWidth;
      const h = container.clientHeight;
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      renderer.setSize(w, h);
    };
    window.addEventListener('resize', handleResize);

    return () => {
      cancelAnimationFrame(animId);
      window.removeEventListener('resize', handleResize);
      renderer.dispose();
      container.removeChild(renderer.domElement);
    };
  }, [diagnosis]);

  return <div ref={containerRef} className="viz-connectome" />;
}
