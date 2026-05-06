import { useEffect, useRef, useState, useCallback } from 'react';

// ── Wire DTO types (mirror of Rust shader_stream.rs) ─────────────────────────

export interface WireStreamDto {
  source: string;
  codebook_indices: number[];
  timestamp: number;
}

export interface WireResonanceDto {
  top_k: [number, number][];
  cycle_count: number;
  converged: boolean;
  entropy: number;
  active_count: number;
}

export interface WireBusDto {
  codebook_index: number;
  energy: number;
  top_k: [number, number][];
  cycle_count: number;
  converged: boolean;
}

export interface WireThoughtStruct {
  bus: WireBusDto;
  text: string | null;
  style: string;
}

export interface WireSceneAct {
  act: number;
  total: number;
  name: string;
  cypher_preview: string;
  confidence: number;
}

export interface WireFreeEnergy {
  likelihood: number;
  kl: number;
  free_energy: number;
  below_homeostasis: boolean;
}

export interface ShaderEvent {
  type: 'stream' | 'resonance' | 'bus' | 'thought' | 'scene' | 'health';
  ts: number;
  payload: unknown;
}

// ── State shape ──────────────────────────────────────────────────────────────

export interface ShaderStreamState {
  connected: boolean;
  lastStream: WireStreamDto | null;
  lastResonance: WireResonanceDto | null;
  lastBus: WireBusDto | null;
  lastThought: WireThoughtStruct | null;
  currentScene: WireSceneAct | null;
  freeEnergy: WireFreeEnergy | null;
  busHistory: WireBusDto[];
  thoughtHistory: WireThoughtStruct[];
  cycle: number;
  eventCount: number;
}

const INITIAL_STATE: ShaderStreamState = {
  connected: false,
  lastStream: null,
  lastResonance: null,
  lastBus: null,
  lastThought: null,
  currentScene: null,
  freeEnergy: null,
  busHistory: [],
  thoughtHistory: [],
  cycle: 0,
  eventCount: 0,
};

// ── Hook ─────────────────────────────────────────────────────────────────────

export function useShaderStream(url = '/v1/shader/stream'): ShaderStreamState & { reconnect: () => void } {
  const [state, setState] = useState<ShaderStreamState>(INITIAL_STATE);
  const esRef = useRef<EventSource | null>(null);
  const cycleRef = useRef(0);

  const connect = useCallback(() => {
    if (esRef.current) {
      esRef.current.close();
      esRef.current = null;
    }

    const cypher_dir = encodeURIComponent(
      // Use CYPHER_PATH from meta tag if present (injected by server)
      (document.querySelector<HTMLMetaElement>('meta[name="cypher-path"]')?.content) ?? '',
    );
    const fullUrl = cypher_dir ? `${url}?cypher_dir=${cypher_dir}` : url;

    const es = new EventSource(fullUrl);
    esRef.current = es;

    es.onopen = () => {
      setState(s => ({ ...s, connected: true }));
    };

    es.onerror = () => {
      setState(s => ({ ...s, connected: false }));
      es.close();
      esRef.current = null;
      // Reconnect after 3s
      setTimeout(connect, 3000);
    };

    const handle = (ev: MessageEvent, type: ShaderEvent['type']) => {
      try {
        const event: ShaderEvent = JSON.parse(ev.data);
        cycleRef.current += 1;
        const cycle = cycleRef.current;

        setState(s => {
          const next = { ...s, eventCount: s.eventCount + 1, cycle };
          switch (type) {
            case 'stream':
              return { ...next, lastStream: event.payload as WireStreamDto };
            case 'resonance':
              return { ...next, lastResonance: event.payload as WireResonanceDto };
            case 'bus': {
              const bus = event.payload as WireBusDto;
              return {
                ...next,
                lastBus: bus,
                busHistory: [...s.busHistory.slice(-99), bus],
              };
            }
            case 'thought': {
              const t = event.payload as WireThoughtStruct;
              return {
                ...next,
                lastThought: t,
                thoughtHistory: [...s.thoughtHistory.slice(-199), t],
              };
            }
            case 'scene':
              return { ...next, currentScene: event.payload as WireSceneAct };
            case 'health':
              return { ...next, freeEnergy: event.payload as WireFreeEnergy };
            default:
              return next;
          }
        });
      } catch {
        // ignore parse errors / keepalive comments
      }
    };

    // SSE named events
    for (const type of ['stream', 'resonance', 'bus', 'thought', 'scene', 'health'] as const) {
      es.addEventListener(type, (ev) => handle(ev as MessageEvent, type));
    }
    // Also catch unnamed messages
    es.onmessage = (ev) => {
      try {
        const event: ShaderEvent = JSON.parse(ev.data);
        handle(ev, event.type ?? 'health');
      } catch { /* keep-alive */ }
    };
  }, [url]);

  useEffect(() => {
    connect();
    return () => {
      esRef.current?.close();
      esRef.current = null;
    };
  }, [connect]);

  return { ...state, reconnect: connect };
}
