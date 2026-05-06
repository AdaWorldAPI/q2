/**
 * Safe coercion utilities. Every numeric value crossing the wire goes through
 * `safeNum()` so NaN/Infinity/null/undefined become a fallback AND get logged
 * to the diagnostics store with field provenance.
 */
import { useDiagnostics } from './store';

const reportThrottle = new Map<string, number>();
const THROTTLE_MS = 2000;

function shouldReport(key: string): boolean {
  const now = Date.now();
  const last = reportThrottle.get(key) ?? 0;
  if (now - last < THROTTLE_MS) return false;
  reportThrottle.set(key, now);
  return true;
}

export function safeNum(v: unknown, fallback = 0, field = 'unknown'): number {
  if (typeof v === 'number' && Number.isFinite(v)) return v;
  // Detect specific failure mode
  let category: 'NaN' | 'Infinity' | 'missing_field' | 'wrong_shape';
  let received: string;
  if (typeof v === 'number') {
    if (Number.isNaN(v)) {
      category = 'NaN';
      received = 'NaN';
    } else {
      category = 'Infinity';
      received = String(v);
    }
  } else if (v === null || v === undefined) {
    category = 'missing_field';
    received = String(v);
  } else {
    category = 'wrong_shape';
    received = `${typeof v}: ${JSON.stringify(v).slice(0, 40)}`;
  }
  if (shouldReport(`safeNum:${field}:${category}`)) {
    useDiagnostics.getState().add({
      source: 'validate',
      level: 'warn',
      category,
      field,
      expected: 'finite number',
      received,
      message: `safeNum(${field}) coerced to ${fallback} from ${received}`,
    });
    useDiagnostics.getState().noteNaN(field);
  }
  return fallback;
}

export function safeStr(v: unknown, fallback = '', field = 'unknown'): string {
  if (typeof v === 'string') return v;
  if (v === null || v === undefined) {
    if (shouldReport(`safeStr:${field}`)) {
      useDiagnostics.getState().add({
        source: 'validate',
        level: 'info',
        category: 'missing_field',
        field,
        expected: 'string',
        received: String(v),
        message: `safeStr(${field}) used fallback "${fallback}"`,
      });
    }
    return fallback;
  }
  return String(v);
}

export function safeArr<T>(v: unknown, field = 'unknown'): T[] {
  if (Array.isArray(v)) return v as T[];
  if (shouldReport(`safeArr:${field}`)) {
    useDiagnostics.getState().add({
      source: 'validate',
      level: 'warn',
      category: 'wrong_shape',
      field,
      expected: 'array',
      received: typeof v,
      message: `safeArr(${field}) used [] from ${typeof v}`,
    });
  }
  return [];
}

/** Format a number with NaN-resilient toFixed. Returns '—' for invalid values. */
export function fmt(v: unknown, digits = 2, field = ''): string {
  if (typeof v === 'number' && Number.isFinite(v)) return v.toFixed(digits);
  if (field && shouldReport(`fmt:${field}`)) {
    useDiagnostics.getState().add({
      source: 'render',
      level: 'info',
      category: typeof v === 'number' ? (Number.isNaN(v) ? 'NaN' : 'Infinity') : 'missing_field',
      field,
      message: `fmt(${field}) rendered "—" for ${String(v)}`,
    });
  }
  return '—';
}

/** Clamp v to [lo, hi]. NaN-safe; returns lo if v invalid. */
export function clamp(v: unknown, lo: number, hi: number, field = ''): number {
  const n = safeNum(v, lo, field || 'clamp');
  return n < lo ? lo : n > hi ? hi : n;
}
