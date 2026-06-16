/**
 * Phase 7, Defence 2 — namespace-invariant guard.
 *
 * A Monaco theme is global: a rule colours any token whose scope *starts with*
 * the rule's token string. The `qmd.` sentinel super-prefix makes it impossible
 * for a quarto rule to prefix-match a scope another language emits. This test
 * encodes that invariant so a future bare `keyword`/`string` rule fails CI
 * rather than silently recolouring TS/JS/CSS/HTML editor-wide.
 */

import { describe, it, expect } from 'vitest';
import { quartoThemeRules } from './quartoTheme';

describe('quartoThemeRules namespace invariant', () => {
  it('every theme rule token carries the qmd. sentinel prefix', () => {
    expect(quartoThemeRules.length).toBeGreaterThan(0);
    for (const rule of quartoThemeRules) {
      expect(
        rule.token.startsWith('qmd.'),
        `theme rule token "${rule.token}" must start with "qmd." — a bare scope would recolour other languages editor-wide`,
      ).toBe(true);
    }
  });

  it('has no duplicate rule tokens', () => {
    const tokens = quartoThemeRules.map((r) => r.token);
    expect(new Set(tokens).size).toBe(tokens.length);
  });
});
