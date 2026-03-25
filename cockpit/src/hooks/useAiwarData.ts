import { useState, useCallback } from 'react';
import type { GraphNode, GraphEdge } from '../store';
import { convertAiwarGraph } from '../data/aiwar-seed';

interface AiwarData {
  nodes: GraphNode[];
  edges: GraphEdge[];
  loading: boolean;
  error: string | null;
}

// Try to fetch from server first, fall back to embedded data
export function useAiwarData() {
  const [data, setData] = useState<AiwarData>({
    nodes: [],
    edges: [],
    loading: false,
    error: null,
  });

  const load = useCallback(async () => {
    setData((d) => ({ ...d, loading: true, error: null }));
    try {
      const res = await fetch('/api/aiwar/graph');
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const raw = await res.json();
      const { nodes, edges } = convertAiwarGraph(raw);
      setData({ nodes, edges, loading: false, error: null });
      return { nodes, edges };
    } catch (err) {
      // Server not available — try embedded data via MCP
      try {
        const res = await fetch('/mcp/message', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            jsonrpc: '2.0',
            id: Date.now(),
            method: 'tools/call',
            params: { name: 'cell_execute', arguments: { code: 'MATCH (n) RETURN n LIMIT 500', lang: 'cypher' } },
          }),
        });
        if (res.ok) {
          const json = await res.json();
          const graphOutput = json.result?.outputs?.find((o: { type: string }) => o.type === 'graph');
          if (graphOutput) {
            const parsed = JSON.parse(graphOutput.content);
            setData({ nodes: parsed.nodes || [], edges: parsed.edges || [], loading: false, error: null });
            return { nodes: parsed.nodes || [], edges: parsed.edges || [] };
          }
        }
      } catch {
        // ignore MCP fallback error
      }
      const errMsg = err instanceof Error ? err.message : 'Failed to load aiwar data';
      setData((d) => ({ ...d, loading: false, error: errMsg }));
      return null;
    }
  }, []);

  return { ...data, load };
}
