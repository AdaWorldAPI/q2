import { useEffect, useRef, useState, useCallback } from 'react';
import { useDiagnostics } from '../diagnostics/store';
import { validateByType } from '../diagnostics/validate';

// ── Wire DTO types (mirror of Rust dto_bridge.rs — canonical R1 from
//    `lance_graph_contract::cognitive_shader`) ───────────────────────────────

/** Single hit in a resonance top_k vector. Mirrors `ShaderHit`. */
export interface WireShaderHit {
  row: number;
  distance: number;
  predicates: number;
  resonance: number;
  cycle_index: number;
}

/** Resonance summary. Mirrors `ShaderResonance`. */
export interface WireShaderResonance {
  top_k: WireShaderHit[];
  hit_count: number;
  cycles_used: number;
  entropy: number;
  std_dev: number;
  style_ord: number;
}

/** Gate decision. Mirrors `GateDecision` from `collapse_gate.rs`. */
export interface WireGateDecision {
  /** u8 — see GateDecision constants in collapse_gate.rs */
  gate: number;
  /** 'Xor' | 'Bundle' | 'Superposition' | 'AlphaFrontToBack' */
  merge: string;
}

/**
 * Bus snapshot. Mirrors `ShaderBus`.
 *
 * Note: `cycle_fingerprint_hash` is an XOR-fold of the full `[u64; 256]`
 * fingerprint history — the cockpit never sees the raw array (too large
 * for SSE). The fold preserves identity-equality for de-duping.
 */
export interface WireShaderBus {
  cycle_fingerprint_hash: number;
  emitted_edge_count: number;
  gate: WireGateDecision;
  resonance: WireShaderResonance;
}

/** Meta-cognition summary. Mirrors `MetaSummary`. */
export interface WireMetaSummary {
  confidence: number;
  meta_confidence: number;
  brier: number;
  should_admit_ignorance: boolean;
}

/** Alpha-front-to-back composite. Mirrors `AlphaComposite`. */
export interface WireAlphaComposite {
  alpha_acc: number;
  hits_consumed: number;
  saturated: boolean;
  color_acc_active_dims: number;
}

/**
 * A crystallized shader cycle. Mirrors `ShaderCrystal` — the canonical
 * "thought-equivalent" in R1. Replaces the old `ThoughtStruct`.
 */
export interface WireShaderCrystal {
  bus: WireShaderBus;
  persisted_row: number | null;
  meta: WireMetaSummary;
  alpha_composite: WireAlphaComposite | null;
}

/** Dispatch parameters. Mirrors `ShaderDispatch`. */
export interface WireShaderDispatch {
  layer_mask: number;
  radius: number;
  style: string;
  max_cycles: number;
  entropy_floor: number;
  emit: string;
  merge_override: string | null;
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
  type: 'dispatch' | 'resonance' | 'bus' | 'crystal' | 'scene' | 'health';
  ts: number;
  payload: unknown;
}

// ── State shape ──────────────────────────────────────────────────────────────

export interface ShaderStreamState {
  connected: boolean;
  lastDispatch: WireShaderDispatch | null;
  lastResonance: WireShaderResonance | null;
  lastBus: WireShaderBus | null;
  lastCrystal: WireShaderCrystal | null;
  currentScene: WireSceneAct | null;
  freeEnergy: WireFreeEnergy | null;
  busHistory: WireShaderBus[];
  crystalHistory: WireShaderCrystal[];
  cycle: number;
  eventCount: number;
}

const INITIAL_STATE: ShaderStreamState = {
  connected: false,
  lastDispatch: null,
  lastResonance: null,
  lastBus: null,
  lastCrystal: null,
  currentScene: null,
  freeEnergy: null,
  busHistory: [],
  crystalHistory: [],
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
    const diag = useDiagnostics.getState();
    diag.setSse({ url: fullUrl });

    es.onopen = () => {
      setState(s => ({ ...s, connected: true }));
      useDiagnostics.getState().setSse({ connected: true });
    };

    es.onerror = () => {
      setState(s => ({ ...s, connected: false }));
      const cur = useDiagnostics.getState();
      cur.setSse({ connected: false, reconnectCount: cur.sse.reconnectCount + 1 });
      cur.add({
        source: 'sse',
        level: 'warn',
        category: 'sse_disconnect',
        endpoint: fullUrl,
        message: `SSE connection lost (reconnect #${cur.sse.reconnectCount + 1} in 3s)`,
      });
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
        // Validate payload shape & log mismatches before consuming
        validateByType(type, event.payload);
        const diagState = useDiagnostics.getState();
        diagState.setSse({
          lastEventTs: Date.now(),
          lastEventType: type,
          bytesReceived: diagState.sse.bytesReceived + (ev.data?.length ?? 0),
        });

        setState(s => {
          const next = { ...s, eventCount: s.eventCount + 1, cycle };
          switch (type) {
            case 'dispatch':
              return { ...next, lastDispatch: event.payload as WireShaderDispatch };
            case 'resonance':
              return { ...next, lastResonance: event.payload as WireShaderResonance };
            case 'bus': {
              const bus = event.payload as WireShaderBus;
              return {
                ...next,
                lastBus: bus,
                busHistory: [...s.busHistory.slice(-99), bus],
              };
            }
            case 'crystal': {
              const c = event.payload as WireShaderCrystal;
              return {
                ...next,
                lastCrystal: c,
                crystalHistory: [...s.crystalHistory.slice(-199), c],
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
      } catch (err) {
        useDiagnostics.getState().add({
          source: 'sse',
          level: 'warn',
          category: 'parse_fail',
          endpoint: fullUrl,
          message: `SSE payload parse failed: ${err instanceof Error ? err.message : String(err)}`,
          detail: { raw: ev.data?.slice?.(0, 200) },
        });
      }
    };

    // SSE named events
    for (const type of ['dispatch', 'resonance', 'bus', 'crystal', 'scene', 'health'] as const) {
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
