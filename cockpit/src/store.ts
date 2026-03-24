import { create } from 'zustand';

// ---- Types ----

export interface GraphNode {
  id: string;
  label: string;
  type: string;
  properties: Record<string, string | number>;
}

export interface GraphEdge {
  source: string;
  target: string;
  label: string;
}

export interface CellOutput {
  type: 'html' | 'text' | 'error' | 'table' | 'graph';
  content: string;
}

export interface Cell {
  id: string;
  source: string;
  language: string;
  execution_state: string;
  outputs: CellOutput[];
}

// ---- Store ----

interface CockpitState {
  // Connection
  connected: boolean;
  setConnected: (v: boolean) => void;

  // Graph data (populated from cell_execute results with type=graph)
  nodes: GraphNode[];
  edges: GraphEdge[];
  setGraphData: (nodes: GraphNode[], edges: GraphEdge[]) => void;

  // Global selection (linked: graph <-> table <-> inspector)
  selectedNodeId: string | null;
  selectNode: (id: string | null) => void;

  // Cells
  cells: Cell[];
  setCells: (cells: Cell[]) => void;
  addCell: (cell: Cell) => void;
  updateCell: (cell: Cell) => void;
  removeCell: (id: string) => void;

  // Execution
  executing: boolean;
  setExecuting: (v: boolean) => void;
}

export const useStore = create<CockpitState>((set) => ({
  connected: false,
  setConnected: (v) => set({ connected: v }),

  nodes: [],
  edges: [],
  setGraphData: (nodes, edges) => set({ nodes, edges }),

  selectedNodeId: null,
  selectNode: (id) =>
    set((state) => ({
      selectedNodeId: state.selectedNodeId === id ? null : id,
    })),

  cells: [],
  setCells: (cells) => set({ cells }),
  addCell: (cell) => set((state) => ({ cells: [...state.cells, cell] })),
  updateCell: (cell) =>
    set((state) => ({
      cells: state.cells.map((c) => (c.id === cell.id ? cell : c)),
    })),
  removeCell: (id) =>
    set((state) => ({ cells: state.cells.filter((c) => c.id !== id) })),

  executing: false,
  setExecuting: (v) => set({ executing: v }),
}));
