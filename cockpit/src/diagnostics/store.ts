import { create } from 'zustand';

export type DiagLevel = 'info' | 'warn' | 'error';
export type DiagSource = 'sse' | 'fetch' | 'render' | 'parse' | 'validate' | 'boundary';
export type DiagCategory =
  | 'NaN'
  | 'Infinity'
  | 'missing_field'
  | 'wrong_shape'
  | 'http_4xx'
  | 'http_5xx'
  | 'network'
  | 'parse_fail'
  | 'crash'
  | 'sse_disconnect'
  | 'sse_reconnect'
  | 'empty_response'
  | 'env_missing';

export interface DiagEntry {
  id: number;
  ts: number;
  source: DiagSource;
  level: DiagLevel;
  category: DiagCategory;
  field?: string;
  endpoint?: string;
  expected?: string;
  received?: string;
  message: string;
  detail?: unknown;
}

export interface EndpointHealth {
  url: string;
  label: string;
  status: 'healthy' | 'degraded' | 'offline' | 'unknown';
  lastChecked: number;
  lastDurationMs: number;
  lastStatus?: number;
  lastError?: string;
  responseShape?: string;
}

export interface SseHealth {
  connected: boolean;
  reconnectCount: number;
  lastEventTs: number;
  lastEventType: string;
  bytesReceived: number;
  url: string;
}

interface DiagnosticsState {
  entries: DiagEntry[];
  endpoints: Record<string, EndpointHealth>;
  sse: SseHealth;
  fieldNaNCount: Record<string, number>;
  overlayOpen: boolean;
  paused: boolean;

  add: (e: Omit<DiagEntry, 'id' | 'ts'>) => void;
  setEndpoint: (key: string, h: Partial<EndpointHealth>) => void;
  setSse: (h: Partial<SseHealth>) => void;
  noteNaN: (field: string) => void;
  clear: () => void;
  toggleOverlay: () => void;
  setOverlayOpen: (v: boolean) => void;
  setPaused: (v: boolean) => void;
}

let nextId = 1;
const MAX_ENTRIES = 500;

export const useDiagnostics = create<DiagnosticsState>((set) => ({
  entries: [],
  endpoints: {},
  sse: {
    connected: false,
    reconnectCount: 0,
    lastEventTs: 0,
    lastEventType: '—',
    bytesReceived: 0,
    url: '/v1/shader/stream',
  },
  fieldNaNCount: {},
  overlayOpen: false,
  paused: false,

  add: (e) =>
    set((s) => {
      if (s.paused) return s;
      const entry: DiagEntry = { ...e, id: nextId++, ts: Date.now() };
      const next = [...s.entries, entry];
      if (next.length > MAX_ENTRIES) next.splice(0, next.length - MAX_ENTRIES);
      return { entries: next };
    }),

  setEndpoint: (key, h) =>
    set((s) => ({
      endpoints: {
        ...s.endpoints,
        [key]: { ...(s.endpoints[key] ?? { url: key, label: key, status: 'unknown', lastChecked: 0, lastDurationMs: 0 }), ...h },
      },
    })),

  setSse: (h) => set((s) => ({ sse: { ...s.sse, ...h } })),

  noteNaN: (field) =>
    set((s) => ({
      fieldNaNCount: { ...s.fieldNaNCount, [field]: (s.fieldNaNCount[field] ?? 0) + 1 },
    })),

  clear: () => set({ entries: [], fieldNaNCount: {} }),
  toggleOverlay: () => set((s) => ({ overlayOpen: !s.overlayOpen })),
  setOverlayOpen: (v) => set({ overlayOpen: v }),
  setPaused: (v) => set({ paused: v }),
}));

// Convenience: derive a status summary
export function diagSummary(state: DiagnosticsState): {
  level: 'good' | 'warn' | 'error';
  errorCount: number;
  warnCount: number;
} {
  let warnCount = 0;
  let errorCount = 0;
  for (const e of state.entries) {
    if (e.level === 'warn') warnCount++;
    else if (e.level === 'error') errorCount++;
  }
  return {
    level: errorCount > 0 ? 'error' : warnCount > 0 ? 'warn' : 'good',
    errorCount,
    warnCount,
  };
}
