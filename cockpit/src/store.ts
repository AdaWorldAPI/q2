import { create } from 'zustand';
import { SEED_NODES, SEED_EDGES } from './seed';

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
  connected: boolean;
  setConnected: (v: boolean) => void;

  nodes: GraphNode[];
  edges: GraphEdge[];
  setGraphData: (nodes: GraphNode[], edges: GraphEdge[]) => void;

  selectedNodeId: string | null;
  selectNode: (id: string | null) => void;

  cells: Cell[];
  setCells: (cells: Cell[]) => void;
  addCell: (cell: Cell) => void;
  updateCell: (cell: Cell) => void;
  removeCell: (id: string) => void;

  executing: boolean;
  setExecuting: (v: boolean) => void;

  // Filter state for left rail
  filter: string;
  setFilter: (f: string) => void;

  // Table search
  searchTerm: string;
  setSearchTerm: (t: string) => void;
}

export const useStore = create<CockpitState>((set) => ({
  connected: false,
  setConnected: (v: boolean) => set({ connected: v }),

  nodes: SEED_NODES,
  edges: SEED_EDGES,
  setGraphData: (nodes: GraphNode[], edges: GraphEdge[]) => set({ nodes, edges }),

  selectedNodeId: null,
  selectNode: (id: string | null) =>
    set((state: CockpitState) => ({
      selectedNodeId: state.selectedNodeId === id ? null : id,
    })),

  cells: [],
  setCells: (cells: Cell[]) => set({ cells }),
  addCell: (cell: Cell) => set((state: CockpitState) => ({ cells: [...state.cells, cell] })),
  updateCell: (cell: Cell) =>
    set((state: CockpitState) => ({
      cells: state.cells.map((c: Cell) => (c.id === cell.id ? cell : c)),
    })),
  removeCell: (id: string) =>
    set((state: CockpitState) => ({ cells: state.cells.filter((c: Cell) => c.id !== id) })),

  executing: false,
  setExecuting: (v: boolean) => set({ executing: v }),

  filter: 'all',
  setFilter: (f: string) => set({ filter: f }),

  searchTerm: '',
  setSearchTerm: (t: string) => set({ searchTerm: t }),
}));
