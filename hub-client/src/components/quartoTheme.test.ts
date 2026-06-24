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
import { quartoThemeRules, qmdMonarch, qmdLanguageConfiguration } from './quartoTheme';

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

describe('qmd Monarch base — bracket symmetry', () => {
  // Regression: a base `content` rule coloured the opening `[` (as `string`)
  // but nothing coloured the closing `]`, so wherever the base shows through
  // (semantic leaves link brackets uncoloured) the two brackets mismatched.
  // The base must treat `[` and `]` identically.
  it('no content rule matches "[" without also matching "]" (or vice versa)', () => {
    const contentRules = qmdMonarch.tokenizer.content as Array<[RegExp, unknown]>;
    for (const rule of contentRules) {
      const re = rule[0];
      expect(re).toBeInstanceOf(RegExp);
      expect(
        re.test('['),
        `rule ${re} treats "[" and "]" asymmetrically — link brackets would mismatch`,
      ).toBe(re.test(']'));
    }
  });
});

describe('qmd language configuration — backtick auto-close (bd-w1s38lbe)', () => {
  // Regression: a backtick in `autoClosingPairs` made Monaco auto-close `` ` ``
  // whenever the next char was whitespace/EOL. Wrapping an existing word in
  // inline code (type `` ` `` before the word, then `` ` `` after it) inserted
  // TWO trailing backticks because the end-of-word keystroke triggered the
  // auto-close. Backticks must NOT auto-close.
  it('does not auto-close backticks', () => {
    const pairs = qmdLanguageConfiguration.autoClosingPairs ?? [];
    expect(
      pairs.some((p) => p.open === '`' || p.close === '`'),
      'backtick must not be in autoClosingPairs — it doubles when wrapping a word',
    ).toBe(false);
  });

  // The fix must not over-correct: typing `` ` `` over a *selection* should
  // still wrap it, which is the `surroundingPairs` list (independent of
  // `autoClosingPairs` in Monaco).
  it('still surrounds a selection with backticks', () => {
    const pairs = qmdLanguageConfiguration.surroundingPairs ?? [];
    expect(
      pairs.some((p) => p.open === '`' && p.close === '`'),
      'backtick must remain in surroundingPairs so wrap-selection still works',
    ).toBe(true);
  });
});
