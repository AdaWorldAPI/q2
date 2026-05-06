/**
 * Wire-DTO shape validators. Called when SSE events arrive.
 * Logs precise mismatch info so the DiagnosticsOverlay can surface
 * "expected X.y to be number[], got string" without fishing through Network tab.
 */
import { useDiagnostics } from './store';

import type { DiagCategory } from './store';

function add(
  level: 'warn' | 'error',
  category: DiagCategory,
  field: string,
  expected: string,
  received: unknown,
  message: string,
) {
  useDiagnostics.getState().add({
    source: 'validate',
    level,
    category,
    field,
    expected,
    received: typeof received === 'string'
      ? received
      : `${typeof received}: ${JSON.stringify(received).slice(0, 80)}`,
    message,
  });
}

function isObj(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

function isFiniteNum(v: unknown): boolean {
  return typeof v === 'number' && Number.isFinite(v);
}

function isNumArr(v: unknown): v is number[] {
  return Array.isArray(v) && v.every((x) => typeof x === 'number');
}

function isTopK(v: unknown): boolean {
  if (!Array.isArray(v)) return false;
  return v.every(
    (e) =>
      Array.isArray(e) &&
      e.length === 2 &&
      typeof e[0] === 'number' &&
      typeof e[1] === 'number',
  );
}

export interface ValidationResult {
  valid: boolean;
  issues: string[];
}

export function validateStream(p: unknown): ValidationResult {
  const issues: string[] = [];
  if (!isObj(p)) {
    add('error', 'wrong_shape', 'stream', 'object', p, 'StreamDto payload not an object');
    return { valid: false, issues: ['stream: not an object'] };
  }
  if (typeof p.source !== 'string') {
    add('warn', 'missing_field', 'stream.source', 'string', p.source, 'StreamDto.source missing');
    issues.push('stream.source missing');
  }
  if (!isNumArr(p.codebook_indices)) {
    add('warn', 'wrong_shape', 'stream.codebook_indices', 'number[]', p.codebook_indices, 'StreamDto.codebook_indices wrong shape');
    issues.push('stream.codebook_indices wrong shape');
  }
  if (!isFiniteNum(p.timestamp)) {
    add('warn', 'NaN', 'stream.timestamp', 'finite number', p.timestamp, 'StreamDto.timestamp NaN/missing');
    issues.push('stream.timestamp NaN');
  }
  return { valid: issues.length === 0, issues };
}

export function validateResonance(p: unknown): ValidationResult {
  const issues: string[] = [];
  if (!isObj(p)) {
    add('error', 'wrong_shape', 'resonance', 'object', p, 'ResonanceDto not object');
    return { valid: false, issues: ['resonance: not an object'] };
  }
  if (!isTopK(p.top_k)) {
    add('warn', 'wrong_shape', 'resonance.top_k', '[number, number][]', p.top_k, 'ResonanceDto.top_k expected tuples');
    issues.push('resonance.top_k wrong shape');
  }
  if (!isFiniteNum(p.cycle_count)) issues.push('resonance.cycle_count NaN');
  if (!isFiniteNum(p.entropy)) {
    add('warn', 'NaN', 'resonance.entropy', 'finite number', p.entropy, 'ResonanceDto.entropy NaN');
    issues.push('resonance.entropy NaN');
  }
  if (!isFiniteNum(p.active_count)) issues.push('resonance.active_count NaN');
  return { valid: issues.length === 0, issues };
}

export function validateBus(p: unknown): ValidationResult {
  const issues: string[] = [];
  if (!isObj(p)) {
    add('error', 'wrong_shape', 'bus', 'object', p, 'BusDto not object');
    return { valid: false, issues: ['bus: not an object'] };
  }
  if (!isFiniteNum(p.codebook_index)) {
    add('warn', 'NaN', 'bus.codebook_index', 'finite number', p.codebook_index, 'BusDto.codebook_index NaN');
    issues.push('bus.codebook_index NaN');
  }
  if (!isFiniteNum(p.energy)) {
    add('warn', 'NaN', 'bus.energy', 'finite number', p.energy, 'BusDto.energy NaN');
    issues.push('bus.energy NaN');
  }
  if (!isTopK(p.top_k)) {
    add('warn', 'wrong_shape', 'bus.top_k', '[number, number][]', p.top_k, 'BusDto.top_k expected tuples');
    issues.push('bus.top_k wrong shape');
  }
  return { valid: issues.length === 0, issues };
}

export function validateThought(p: unknown): ValidationResult {
  const issues: string[] = [];
  if (!isObj(p)) {
    add('error', 'wrong_shape', 'thought', 'object', p, 'ThoughtStruct not object');
    return { valid: false, issues: ['thought: not an object'] };
  }
  if (!isObj(p.bus)) {
    add('warn', 'missing_field', 'thought.bus', 'object', p.bus, 'ThoughtStruct.bus missing');
    issues.push('thought.bus missing');
  } else {
    issues.push(...validateBus(p.bus).issues.map((i) => `thought.${i}`));
  }
  // text may be null per spec — that's fine
  if (p.text !== null && typeof p.text !== 'string' && p.text !== undefined) {
    issues.push('thought.text: expected string|null');
  }
  return { valid: issues.length === 0, issues };
}

export function validateScene(p: unknown): ValidationResult {
  const issues: string[] = [];
  if (!isObj(p)) {
    add('error', 'wrong_shape', 'scene', 'object', p, 'SceneAct not object');
    return { valid: false, issues: ['scene: not an object'] };
  }
  if (!isFiniteNum(p.act)) issues.push('scene.act NaN');
  if (!isFiniteNum(p.total)) issues.push('scene.total NaN');
  if (typeof p.name !== 'string') issues.push('scene.name not string');
  if (!isFiniteNum(p.confidence)) {
    add('warn', 'NaN', 'scene.confidence', 'finite number', p.confidence, 'SceneAct.confidence NaN');
    issues.push('scene.confidence NaN');
  }
  return { valid: issues.length === 0, issues };
}

export function validateFreeEnergy(p: unknown): ValidationResult {
  const issues: string[] = [];
  if (!isObj(p)) {
    add('error', 'wrong_shape', 'health', 'object', p, 'FreeEnergy not object');
    return { valid: false, issues: ['health: not an object'] };
  }
  for (const k of ['likelihood', 'kl', 'free_energy']) {
    if (!isFiniteNum(p[k])) {
      add('warn', 'NaN', `health.${k}`, 'finite number', p[k], `FreeEnergy.${k} NaN`);
      issues.push(`health.${k} NaN`);
    }
  }
  return { valid: issues.length === 0, issues };
}

export function validateByType(type: string, payload: unknown): ValidationResult {
  switch (type) {
    case 'stream': return validateStream(payload);
    case 'resonance': return validateResonance(payload);
    case 'bus': return validateBus(payload);
    case 'thought': return validateThought(payload);
    case 'scene': return validateScene(payload);
    case 'health': return validateFreeEnergy(payload);
    default:
      add('warn', 'wrong_shape', 'event.type', 'known type', type, `unknown event type: ${type}`);
      return { valid: false, issues: [`unknown event type: ${type}`] };
  }
}
