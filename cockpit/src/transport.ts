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
    const cell: Cell = await callTool('cell_execute', args);
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
