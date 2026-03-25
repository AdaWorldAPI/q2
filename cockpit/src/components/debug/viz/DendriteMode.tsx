import { useRef, useEffect, useState } from 'react';
import * as THREE from 'three';
import type { StackDiagnosis } from '../../../hooks/useNeuralDiagnosis';
import { THINKING_STYLES, CLUSTER_COLORS, computeActivationPattern } from '../../../data/thinking-styles';
import type { GraphNode } from '../../../store';
import { useStore } from '../../../store';

interface DendriteModeProps {
  diagnosis: StackDiagnosis;
}

const STYLE_COLORS = THINKING_STYLES.map((s) => new THREE.Color(s.color));

export function DendriteMode({ diagnosis }: DendriteModeProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const nodes = useStore((s) => s.nodes);
  const [activeStyleIdx, setActiveStyleIdx] = useState<number | null>(null);
  const [showSuperposition, setShowSuperposition] = useState(false);

  useEffect(() => {
    if (!containerRef.current || nodes.length === 0) return;

    const container = containerRef.current;
    const w = container.clientWidth;
    const h = container.clientHeight;

    const scene = new THREE.Scene();
    scene.background = new THREE.Color(0x0a0e17);

    const camera = new THREE.PerspectiveCamera(50, w / h, 0.1, 100);
    camera.position.set(0, 0, 5);

    const renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setSize(w, h);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    container.appendChild(renderer.domElement);

    // Position nodes in 3D space (use hash of id for deterministic position)
    const nodePositions = new Map<string, THREE.Vector3>();
    nodes.forEach((node, i) => {
      const angle = (i / nodes.length) * Math.PI * 2;
      const radius = 1.5 + Math.random() * 0.8;
      const y = (Math.random() - 0.5) * 2;
      nodePositions.set(node.id, new THREE.Vector3(
        Math.cos(angle) * radius,
        y,
        Math.sin(angle) * radius,
      ));
    });

    // Compute all 36 activation patterns
    const patterns = THINKING_STYLES.map((style) =>
      computeActivationPattern(nodes, style),
    );

    // For each style, build a dendritic tree (lines from center to activated nodes)
    const treeMeshes: THREE.LineSegments[] = [];

    for (let s = 0; s < THINKING_STYLES.length; s++) {
      const pattern = patterns[s];
      const color = STYLE_COLORS[s];
      const linePositions: number[] = [];
      const lineColors: number[] = [];

      for (const node of nodes) {
        const activation = pattern.activations.get(node.id);
        if (!activation || !activation.fired) continue;

        const pos = nodePositions.get(node.id);
        if (!pos) continue;

        // Draw line from center to this node
        const strength = Math.max(0.1, activation.score);
        linePositions.push(0, 0, 0, pos.x, pos.y, pos.z);
        lineColors.push(
          color.r * strength, color.g * strength, color.b * strength,
          color.r, color.g, color.b,
        );
      }

      if (linePositions.length === 0) continue;

      const geom = new THREE.BufferGeometry();
      geom.setAttribute('position', new THREE.Float32BufferAttribute(linePositions, 3));
      geom.setAttribute('color', new THREE.Float32BufferAttribute(lineColors, 3));
      const mat = new THREE.LineBasicMaterial({
        vertexColors: true,
        transparent: true,
        opacity: showSuperposition ? 0.08 : 0.6,
        linewidth: 1,
      });
      const mesh = new THREE.LineSegments(geom, mat);
      mesh.visible = showSuperposition || activeStyleIdx === s || activeStyleIdx === null;
      treeMeshes.push(mesh);
      scene.add(mesh);
    }

    // Node spheres (colored by superposition consensus)
    const nodeGeom = new THREE.SphereGeometry(0.03, 8, 6);
    nodes.forEach((node) => {
      const pos = nodePositions.get(node.id);
      if (!pos) return;

      // Count how many styles fire this node
      let fireCount = 0;
      patterns.forEach((p) => {
        if (p.activations.get(node.id)?.fired) fireCount++;
      });

      const consensusPct = fireCount / 36;
      const color = new THREE.Color().lerpColors(
        new THREE.Color(0x1a2a3a), // dark = blind spot
        new THREE.Color(0xffffff), // white = consensus
        consensusPct,
      );
      const mat = new THREE.MeshBasicMaterial({ color, transparent: true, opacity: 0.5 + consensusPct * 0.5 });
      const sphere = new THREE.Mesh(nodeGeom, mat);
      sphere.position.copy(pos);
      scene.add(sphere);
    });

    // Center point
    const centerGeom = new THREE.SphereGeometry(0.05, 16, 12);
    const centerMat = new THREE.MeshBasicMaterial({ color: 0xffffff });
    scene.add(new THREE.Mesh(centerGeom, centerMat));

    let animId: number;
    let frame = 0;
    function animate() {
      frame++;
      const t = frame * 0.003;
      camera.position.x = Math.sin(t) * 5;
      camera.position.z = Math.cos(t) * 5;
      camera.position.y = Math.sin(t * 0.5) * 1.5;
      camera.lookAt(0, 0, 0);

      renderer.render(scene, camera);
      animId = requestAnimationFrame(animate);
    }
    animate();

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
  }, [diagnosis, nodes, activeStyleIdx, showSuperposition]);

  return (
    <div className="viz-dendrite">
      <div ref={containerRef} className="viz-dendrite-canvas" />
      <div className="viz-dendrite-controls">
        <button
          className={`viz-ctrl-btn ${showSuperposition ? 'active' : ''}`}
          onClick={() => { setShowSuperposition(true); setActiveStyleIdx(null); }}
        >
          Superposition
        </button>
        <button
          className={`viz-ctrl-btn ${!showSuperposition && activeStyleIdx === null ? 'active' : ''}`}
          onClick={() => { setShowSuperposition(false); setActiveStyleIdx(null); }}
        >
          All Trees
        </button>
        <select
          className="viz-style-select"
          value={activeStyleIdx ?? ''}
          onChange={(e) => {
            const v = e.target.value;
            setActiveStyleIdx(v === '' ? null : Number(v));
            setShowSuperposition(false);
          }}
        >
          <option value="">Select style...</option>
          {THINKING_STYLES.map((s, i) => (
            <option key={s.id} value={i}>{s.name} ({s.cluster})</option>
          ))}
        </select>
      </div>
    </div>
  );
}
