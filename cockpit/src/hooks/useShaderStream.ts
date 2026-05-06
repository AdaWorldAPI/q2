import { useEffect, useRef, useState, useCallback } from 'react';
import { useDiagnostics } from '../diagnostics/store';
import { validateByType } from '../diagnostics/validate';

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

/**
 * Mirrors `ThoughtStruct` from `lance-graph/crates/thinking-engine/src/dto.rs`.
 *
 * The Rust shape is:
 *   pub struct ThoughtStruct {
 *       pub bus: BusDto,
 *       pub text: Option<String>,
 *       pub sensor_contributions: Vec<(SourceType, Vec<u16>)>,
 *       pub tension_history: Vec<Vec<f32>>,
 *       pub style_trajectory: Vec<ThinkingScale>,
 *   }
 *
 * `dto_bridge.rs` (the serde-serializing wire layer) is expected to:
 *   - emit field names verbatim (snake_case via `#[serde(rename_all = "snake_case")]`)
 *   - serialize `(SourceType, Vec<u16>)` tuples as JSON 2-tuples `[string, number[]]`
 *   - serialize `ThinkingScale` enum variants as their string names
 *   - replace `tension_history: Vec<Vec<f32>>` with `tension_history_len: u32`
 *     (the cockpit only needs the depth; full per-cycle energy snapshots are
 *     too large to ship over SSE per thought). The legacy `style: string`
 *     field is retained as the *last* `style_trajectory` entry for back-compat
 *     with `ThoughtLog`.
 */
export interface WireThoughtStruct {
  bus: WireBusDto;
  text: string | null;
  /** Last entry of `style_trajectory`, retained for ThoughtLog back-compat. */
  style: string;
  /** Vec<(SourceType, Vec<u16>)> serialized as JSON 2-tuples. */
  sensor_contributions: [string, number[]][];
  /** Length of `Vec<Vec<f32>>`; full data is intentionally omitted. */
  tension_history_len: number;
  /** ThinkingScale variants serialized as strings. */
  style_trajectory: string[];
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
  /** `tension_history_len` from the most recent thought (0 if none). */
  tensionDepth: number;
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
  tensionDepth: 0,
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
                tensionDepth: t.tension_history_len ?? 0,
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
