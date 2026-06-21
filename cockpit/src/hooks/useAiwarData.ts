import { useState, useCallback } from 'react';
import type { GraphNode, GraphEdge } from '../store';
import { convertAiwarGraph } from '../data/aiwar-seed';

export interface AiwarWeapon {
  weapon: string;
  developed: string;
  usedBy: string;
  militaryPurpose: string;
  typeOfTech: string;
  repurpose: string;
  source: string;
  sourceType: string;
}

interface AiwarData {
  nodes: GraphNode[];
  edges: GraphEdge[];
  weapons: AiwarWeapon[];
  loading: boolean;
  error: string | null;
}

export function useAiwarData() {
  const [data, setData] = useState<AiwarData>({
    nodes: [],
    edges: [],
    weapons: [],
    loading: false,
    error: null,
  });

  const load = useCallback(async () => {
    setData((d) => ({ ...d, loading: true, error: null }));
    try {
      // Fetch both the graph JSON and the weapons CSV (both in /public/)
      const [graphRes, weaponsRes] = await Promise.all([
        fetch('/aiwar_graph.json'),
        fetch('/aiwar_weapons.json'),
      ]);

      if (!graphRes.ok) throw new Error(`Failed to load graph: HTTP ${graphRes.status}`);
      if (!weaponsRes.ok) throw new Error(`Failed to load weapons: HTTP ${weaponsRes.status}`);

      const raw = await graphRes.json();
      const weapons: AiwarWeapon[] = await weaponsRes.json();
      const { nodes, edges } = convertAiwarGraph(raw);

      setData({ nodes, edges, weapons, loading: false, error: null });
      return { nodes, edges, weapons };
    } catch (err) {
      // Fallback: try the server API endpoint
      try {
        const res = await fetch('/api/aiwar/graph');
        if (res.ok) {
          const raw = await res.json();
          const { nodes, edges } = convertAiwarGraph(raw);
          setData({ nodes, edges, weapons: [], loading: false, error: null });
          return { nodes, edges, weapons: [] };
        }
      } catch {
        // ignore server fallback error
      }
      const errMsg = err instanceof Error ? err.message : 'Failed to load aiwar data';
      setData((d) => ({ ...d, loading: false, error: errMsg }));
      return null;
    }
  }, []);

  return { ...data, load };
}
