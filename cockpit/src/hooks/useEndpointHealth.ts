import { useEffect, useRef } from 'react';
import { useDiagnostics, type EndpointHealth } from '../diagnostics/store';

interface EndpointSpec {
  key: string;
  url: string;
  label: string;
  /** Optional shape check: throw if response isn't usable. */
  shapeHint?: (data: unknown) => string | null;
  /** HTTP method, default GET */
  method?: 'GET' | 'HEAD';
}

const ENDPOINTS: EndpointSpec[] = [
  { key: 'health', url: '/health', label: 'health' },
  {
    key: 'graph_snapshot',
    url: '/api/graph/snapshot',
    label: 'graph snapshot',
    shapeHint: (d) => {
      if (!d || typeof d !== 'object') return 'expected object';
      const obj = d as Record<string, unknown>;
      if (!Array.isArray(obj.nodes)) return 'nodes not array';
      if (!Array.isArray(obj.edges)) return 'edges not array';
      return null;
    },
  },
  { key: 'graph_health', url: '/api/graph/health', label: 'graph health' },
  { key: 'shader_status', url: '/v1/shader/status', label: 'shader status' },
  { key: 'data_status', url: '/api/data/status', label: 'data status' },
];

async function probe(spec: EndpointSpec): Promise<Partial<EndpointHealth>> {
  const start = performance.now();
  try {
    const res = await fetch(spec.url, { method: spec.method ?? 'GET' });
    const dur = Math.round(performance.now() - start);
    if (!res.ok) {
      const body = await res.text().catch(() => '');
      return {
        status: res.status >= 500 ? 'offline' : 'degraded',
        lastChecked: Date.now(),
        lastDurationMs: dur,
        lastStatus: res.status,
        lastError: body.slice(0, 80) || `HTTP ${res.status}`,
      };
    }
    if (spec.method === 'HEAD' || !spec.shapeHint) {
      return { status: 'healthy', lastChecked: Date.now(), lastDurationMs: dur, lastStatus: res.status, lastError: undefined };
    }
    let parsed: unknown;
    try {
      parsed = await res.json();
    } catch (e) {
      return {
        status: 'degraded',
        lastChecked: Date.now(),
        lastDurationMs: dur,
        lastStatus: res.status,
        lastError: 'invalid JSON',
      };
    }
    const shapeIssue = spec.shapeHint(parsed);
    return {
      status: shapeIssue ? 'degraded' : 'healthy',
      lastChecked: Date.now(),
      lastDurationMs: dur,
      lastStatus: res.status,
      lastError: shapeIssue ?? undefined,
      responseShape: shapeIssue ?? 'ok',
    };
  } catch (e) {
    const dur = Math.round(performance.now() - start);
    return {
      status: 'offline',
      lastChecked: Date.now(),
      lastDurationMs: dur,
      lastError: e instanceof Error ? e.message : 'network error',
    };
  }
}

/**
 * Polls the standard backend endpoints every `intervalMs` and updates the
 * diagnostics store. The first poll runs immediately on mount.
 */
export function useEndpointHealth(intervalMs = 8000) {
  const setEndpoint = useDiagnostics((s) => s.setEndpoint);
  const add = useDiagnostics((s) => s.add);
  const lastSeenStatusRef = useRef<Record<string, string>>({});

  useEffect(() => {
    let cancelled = false;
    // Seed all endpoints in 'unknown' state so the overlay shows them
    for (const spec of ENDPOINTS) {
      setEndpoint(spec.key, { url: spec.url, label: spec.label, status: 'unknown', lastChecked: 0, lastDurationMs: 0 });
    }
    const tick = async () => {
      for (const spec of ENDPOINTS) {
        const result = await probe(spec);
        if (cancelled) return;
        setEndpoint(spec.key, { url: spec.url, label: spec.label, ...result });
        // Log only on status changes
        const prev = lastSeenStatusRef.current[spec.key];
        if (prev !== result.status) {
          lastSeenStatusRef.current[spec.key] = result.status ?? 'unknown';
          if (result.status === 'offline') {
            add({
              source: 'fetch',
              level: 'error',
              category: 'network',
              endpoint: spec.url,
              message: `${spec.label} OFFLINE: ${result.lastError ?? 'unknown'}`,
            });
          } else if (result.status === 'degraded') {
            add({
              source: 'fetch',
              level: 'warn',
              category: result.lastStatus && result.lastStatus >= 400 ? (result.lastStatus >= 500 ? 'http_5xx' : 'http_4xx') : 'wrong_shape',
              endpoint: spec.url,
              message: `${spec.label} degraded: ${result.lastError ?? 'shape mismatch'}`,
            });
          } else if (result.status === 'healthy' && prev && prev !== 'unknown') {
            add({
              source: 'fetch',
              level: 'info',
              category: 'network',
              endpoint: spec.url,
              message: `${spec.label} recovered`,
            });
          }
        }
      }
    };
    tick();
    const id = setInterval(tick, intervalMs);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [intervalMs, setEndpoint, add]);
}
