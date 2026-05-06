/**
 * Wire-DTO shape validators. Called when SSE events arrive.
 * Logs precise mismatch info so the DiagnosticsOverlay can surface
 * "expected X.y to be number[], got string" without fishing through Network tab.
 *
 * Mirrors the canonical R1 wire types from `dto_bridge.rs`
 * (`lance_graph_contract::cognitive_shader`).
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

/**
 * Validate that `v` is a `WireShaderHit[]`:
 *   { row, distance, predicates, resonance, cycle_index } — all finite numbers.
 */
function isHitArray(v: unknown): boolean {
  if (!Array.isArray(v)) return false;
  return v.every((h) =>
    isObj(h) &&
    isFiniteNum(h.row) &&
    isFiniteNum(h.distance) &&
    isFiniteNum(h.predicates) &&
    isFiniteNum(h.resonance) &&
    isFiniteNum(h.cycle_index),
  );
}

export interface ValidationResult {
  valid: boolean;
  issues: string[];
}

export function validateDispatch(p: unknown): ValidationResult {
  const issues: string[] = [];
  if (!isObj(p)) {
    add('error', 'wrong_shape', 'dispatch', 'object', p, 'ShaderDispatch payload not an object');
    return { valid: false, issues: ['dispatch: not an object'] };
  }
  if (!isFiniteNum(p.layer_mask)) {
    add('warn', 'NaN', 'dispatch.layer_mask', 'finite number', p.layer_mask, 'ShaderDispatch.layer_mask NaN');
    issues.push('dispatch.layer_mask NaN');
  }
  if (!isFiniteNum(p.radius)) {
    add('warn', 'NaN', 'dispatch.radius', 'finite number', p.radius, 'ShaderDispatch.radius NaN');
    issues.push('dispatch.radius NaN');
  }
  if (typeof p.style !== 'string') {
    add('warn', 'wrong_shape', 'dispatch.style', 'string', p.style, 'ShaderDispatch.style not string');
    issues.push('dispatch.style not string');
  }
  if (!isFiniteNum(p.max_cycles)) {
    add('warn', 'NaN', 'dispatch.max_cycles', 'finite number', p.max_cycles, 'ShaderDispatch.max_cycles NaN');
    issues.push('dispatch.max_cycles NaN');
  }
  if (!isFiniteNum(p.entropy_floor)) {
    add('warn', 'NaN', 'dispatch.entropy_floor', 'finite number', p.entropy_floor, 'ShaderDispatch.entropy_floor NaN');
    issues.push('dispatch.entropy_floor NaN');
  }
  if (typeof p.emit !== 'string') {
    issues.push('dispatch.emit not string');
  }
  // merge_override may be null per spec
  if (p.merge_override !== null && typeof p.merge_override !== 'string' && p.merge_override !== undefined) {
    issues.push('dispatch.merge_override: expected string|null');
  }
  return { valid: issues.length === 0, issues };
}

export function validateResonance(p: unknown): ValidationResult {
  const issues: string[] = [];
  if (!isObj(p)) {
    add('error', 'wrong_shape', 'resonance', 'object', p, 'ShaderResonance not object');
    return { valid: false, issues: ['resonance: not an object'] };
  }
  if (!isHitArray(p.top_k)) {
    add('warn', 'wrong_shape', 'resonance.top_k', 'WireShaderHit[]', p.top_k, 'ShaderResonance.top_k expected hit objects');
    issues.push('resonance.top_k wrong shape');
  }
  if (!isFiniteNum(p.hit_count)) issues.push('resonance.hit_count NaN');
  if (!isFiniteNum(p.cycles_used)) issues.push('resonance.cycles_used NaN');
  if (!isFiniteNum(p.entropy)) {
    add('warn', 'NaN', 'resonance.entropy', 'finite number', p.entropy, 'ShaderResonance.entropy NaN');
    issues.push('resonance.entropy NaN');
  }
  if (!isFiniteNum(p.std_dev)) issues.push('resonance.std_dev NaN');
  if (!isFiniteNum(p.style_ord)) issues.push('resonance.style_ord NaN');
  return { valid: issues.length === 0, issues };
}

function validateGate(p: unknown): ValidationResult {
  const issues: string[] = [];
  if (!isObj(p)) {
    add('error', 'wrong_shape', 'bus.gate', 'object', p, 'GateDecision not object');
    return { valid: false, issues: ['gate: not an object'] };
  }
  if (!isFiniteNum(p.gate)) {
    add('warn', 'NaN', 'bus.gate.gate', 'finite number (u8)', p.gate, 'GateDecision.gate NaN');
    issues.push('gate.gate NaN');
  }
  if (typeof p.merge !== 'string') {
    add('warn', 'wrong_shape', 'bus.gate.merge', 'string', p.merge, 'GateDecision.merge not string');
    issues.push('gate.merge not string');
  }
  return { valid: issues.length === 0, issues };
}

export function validateBus(p: unknown): ValidationResult {
  const issues: string[] = [];
  if (!isObj(p)) {
    add('error', 'wrong_shape', 'bus', 'object', p, 'ShaderBus not object');
    return { valid: false, issues: ['bus: not an object'] };
  }
  if (!isFiniteNum(p.cycle_fingerprint_hash)) {
    add('warn', 'NaN', 'bus.cycle_fingerprint_hash', 'finite number', p.cycle_fingerprint_hash, 'ShaderBus.cycle_fingerprint_hash NaN');
    issues.push('bus.cycle_fingerprint_hash NaN');
  }
  if (!isFiniteNum(p.emitted_edge_count)) {
    add('warn', 'NaN', 'bus.emitted_edge_count', 'finite number', p.emitted_edge_count, 'ShaderBus.emitted_edge_count NaN');
    issues.push('bus.emitted_edge_count NaN');
  }
  if (!isObj(p.gate)) {
    add('warn', 'missing_field', 'bus.gate', 'object', p.gate, 'ShaderBus.gate missing');
    issues.push('bus.gate missing');
  } else {
    issues.push(...validateGate(p.gate).issues.map((i) => `bus.${i}`));
  }
  if (!isObj(p.resonance)) {
    add('warn', 'missing_field', 'bus.resonance', 'object', p.resonance, 'ShaderBus.resonance missing');
    issues.push('bus.resonance missing');
  } else {
    issues.push(...validateResonance(p.resonance).issues.map((i) => `bus.${i}`));
  }
  return { valid: issues.length === 0, issues };
}

function validateMeta(p: unknown): ValidationResult {
  const issues: string[] = [];
  if (!isObj(p)) {
    add('error', 'wrong_shape', 'crystal.meta', 'object', p, 'MetaSummary not object');
    return { valid: false, issues: ['meta: not an object'] };
  }
  for (const k of ['confidence', 'meta_confidence', 'brier']) {
    if (!isFiniteNum(p[k])) {
      add('warn', 'NaN', `crystal.meta.${k}`, 'finite number', p[k], `MetaSummary.${k} NaN`);
      issues.push(`meta.${k} NaN`);
    }
  }
  if (typeof p.should_admit_ignorance !== 'boolean') {
    issues.push('meta.should_admit_ignorance not boolean');
  }
  return { valid: issues.length === 0, issues };
}

function validateAlphaComposite(p: unknown): ValidationResult {
  const issues: string[] = [];
  if (!isObj(p)) {
    add('error', 'wrong_shape', 'crystal.alpha_composite', 'object', p, 'AlphaComposite not object');
    return { valid: false, issues: ['alpha_composite: not an object'] };
  }
  if (!isFiniteNum(p.alpha_acc)) issues.push('alpha_composite.alpha_acc NaN');
  if (!isFiniteNum(p.hits_consumed)) issues.push('alpha_composite.hits_consumed NaN');
  if (typeof p.saturated !== 'boolean') issues.push('alpha_composite.saturated not boolean');
  if (!isFiniteNum(p.color_acc_active_dims)) issues.push('alpha_composite.color_acc_active_dims NaN');
  return { valid: issues.length === 0, issues };
}

export function validateCrystal(p: unknown): ValidationResult {
  const issues: string[] = [];
  if (!isObj(p)) {
    add('error', 'wrong_shape', 'crystal', 'object', p, 'ShaderCrystal not object');
    return { valid: false, issues: ['crystal: not an object'] };
  }
  if (!isObj(p.bus)) {
    add('warn', 'missing_field', 'crystal.bus', 'object', p.bus, 'ShaderCrystal.bus missing');
    issues.push('crystal.bus missing');
  } else {
    issues.push(...validateBus(p.bus).issues.map((i) => `crystal.${i}`));
  }
  if (p.persisted_row !== null && !isFiniteNum(p.persisted_row) && p.persisted_row !== undefined) {
    issues.push('crystal.persisted_row: expected number|null');
  }
  if (!isObj(p.meta)) {
    add('warn', 'missing_field', 'crystal.meta', 'object', p.meta, 'ShaderCrystal.meta missing');
    issues.push('crystal.meta missing');
  } else {
    issues.push(...validateMeta(p.meta).issues.map((i) => `crystal.${i}`));
  }
  // alpha_composite may be null
  if (p.alpha_composite !== null && p.alpha_composite !== undefined) {
    issues.push(...validateAlphaComposite(p.alpha_composite).issues.map((i) => `crystal.${i}`));
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
    case 'dispatch': return validateDispatch(payload);
    case 'resonance': return validateResonance(payload);
    case 'bus': return validateBus(payload);
    case 'crystal': return validateCrystal(payload);
    case 'scene': return validateScene(payload);
    case 'health': return validateFreeEnergy(payload);
    default:
      add('warn', 'wrong_shape', 'event.type', 'known type', type, `unknown event type: ${type}`);
      return { valid: false, issues: [`unknown event type: ${type}`] };
  }
}
