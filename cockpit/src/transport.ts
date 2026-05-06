import { useStore, type Cell, type GraphNode, type GraphEdge } from './store';

let eventSource: EventSource | null = null;
let requestId = 0;

// ---- SSE connection ----

export function connectSSE() {
  if (eventSource) return;

  eventSource = new EventSource('/mcp/sse');

  eventSource.onopen = () => {
    useStore.getState().setConnected(true);
  };

  eventSource.onmessage = (event) => {
    try {
      const msg = JSON.parse(event.data);
      if (msg.method === 'notifications/initialized') {
        useStore.getState().setConnected(true);
      }
    } catch {
      // ignore non-JSON keepalive comments
    }
  };

  eventSource.onerror = () => {
    useStore.getState().setConnected(false);
    eventSource?.close();
    eventSource = null;
    // Reconnect after 2 seconds
    setTimeout(connectSSE, 2000);
  };
}

// ---- MCP tool calls ----

async function callTool(name: string, args: Record<string, unknown>) {
  const id = ++requestId;
  const res = await fetch('/mcp/message', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      jsonrpc: '2.0',
      id,
      method: 'tools/call',
      params: { name, arguments: args },
    }),
  });
  const json = await res.json();
  if (json.error) {
    throw new Error(json.error.message);
  }
  return json.result;
}

// ---- High-level API ----

export async function executeQuery(code: string, lang?: string): Promise<Cell> {
  const store = useStore.getState();
  store.setExecuting(true);
  try {
    const args: Record<string, unknown> = { code };
    if (lang) args.lang = lang;
    const raw = await callTool('cell_execute', args);
    // Ensure the result has the Cell shape (outputs array)
    const cell: Cell = {
      id: raw?.id || `cell-${Date.now()}`,
      source: raw?.source || code,
      language: raw?.language || lang || 'unknown',
      execution_state: raw?.execution_state || 'success',
      outputs: Array.isArray(raw?.outputs) ? raw.outputs : [],
    };
    store.addCell(cell);

    // If the result contains graph output, parse and set graph data
    const graphOutput = cell.outputs.find((o) => o.type === 'graph');
    if (graphOutput) {
      try {
        const data = JSON.parse(graphOutput.content);
        if (data.nodes && data.edges) {
          store.setGraphData(
            data.nodes as GraphNode[],
            data.edges as GraphEdge[],
          );
        }
      } catch {
        // Not JSON graph data, ignore
      }
    }

    return cell;
  } finally {
    store.setExecuting(false);
  }
}

export async function listCells(): Promise<Cell[]> {
  const cells: Cell[] = await callTool('cells_list', {});
  useStore.getState().setCells(cells);
  return cells;
}

export async function createCell(source: string, language?: string): Promise<Cell> {
  const args: Record<string, unknown> = { source };
  if (language) args.language = language;
  const cell: Cell = await callTool('cell_create', args);
  useStore.getState().addCell(cell);
  return cell;
}

export async function deleteCell(id: string): Promise<void> {
  await callTool('cell_delete', { id });
  useStore.getState().removeCell(id);
}

export async function exportNotebook(format: 'html' | 'pdf'): Promise<string> {
  const result = await callTool('notebook_export', { format });
  return result.exported;
}

// ── Live Graph Engine API (neo4j-emulating, NARS-enabled) ──────────────────

export async function fetchLiveGraph(): Promise<{
  nodes: GraphNode[];
  edges: GraphEdge[];
  node_count: number;
  edge_count: number;
  scene_version: number;
  scene_name: string;
  health: { total_nodes: number; total_edges: number; total_inferences: number; contradiction_count: number; confidence_avg: number };
  nars_inferences: Array<{ source: string; target: string; relation: string; inference_type: string; truth_f: number; truth_c: number; via: string[] }>;
} | null> {
  try {
    const res = await fetch('/api/graph/snapshot');
    if (!res.ok) return null;
    const data = await res.json();
    // Map server field names to store field names
    if (data.nodes) {
      data.nodes = data.nodes.map((n: Record<string, unknown>) => ({
        id: n.id,
        label: n.label,
        type: n.node_type || n.type || 'Node',
        properties: n.properties || {},
      }));
    }
    return data;
  } catch {
    return null;
  }
}

export async function runNarsInference(minConfidence = 0.4, maxHops = 2) {
  try {
    const res = await fetch('/api/graph/infer', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ min_confidence: minConfidence, max_hops: maxHops }),
    });
    if (!res.ok) return null;
    return await res.json();
  } catch {
    return null;
  }
}

export interface LiveNarsInference {
  source: string;
  target: string;
  relation: string;
  inference_type: string;
  truth_f: number;
  truth_c: number;
  via: string[];
}

/// Run NARS inference scoped to a specific node and map to ReasoningResult shape.
/// Falls back to null so caller can use stub.
export async function runNarsForNode(
  nodeId: string,
  minConfidence = 0.3,
  maxHops = 3,
): Promise<{
  inferences: LiveNarsInference[];
  inferred_edges: number;
} | null> {
  try {
    const res = await fetch('/api/graph/infer', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ min_confidence: minConfidence, max_hops: maxHops, node_id: nodeId }),
    });
    if (!res.ok) return null;
    const data = await res.json();
    return data;
  } catch {
    return null;
  }
}

export async function fetchGraphHealth() {
  try {
    const res = await fetch('/api/graph/health');
    if (!res.ok) return null;
    return await res.json();
  } catch {
    return null;
  }
}

/// Try live graph first, fall back to seed data if unavailable.
export async function hydrateCockpit(): Promise<boolean> {
  const live = await fetchLiveGraph();
  if (live && live.nodes.length > 0) {
    useStore.getState().setGraphData(live.nodes as GraphNode[], live.edges as GraphEdge[]);
    return true; // live data
  }
  return false; // use seed fallback
}
