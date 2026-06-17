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
import { quartoThemeRules, qmdMonarch } from './quartoTheme';

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

describe('qmd Monarch base — group reconstruction invariant', () => {
  // Regression (GH#10): Monaco's MonarchTokenizer throws "with groups, all
  // characters should be matched in consecutive groups" when an array-action
  // rule's regex consumes characters outside its capture groups (e.g. via a
  // non-capturing `(?:…)` group). It crashes the editor mid-tokenise, but only
  // on lines that actually match — so it slips past mount and fires when a
  // ```{r} executable cell appears. Encode the invariant statically.
  const samples: Array<{ line: string; desc: string }> = [
    { line: '```{r}', desc: 'executable cell, brace syntax' },
    { line: '```{python echo=false}', desc: 'executable cell with options' },
    { line: '```python', desc: 'plain fenced cell' },
    { line: '```', desc: 'bare fence' },
  ];
  const contentRules = qmdMonarch.tokenizer.content as Array<[RegExp, unknown]>;
  for (const { line, desc } of samples) {
    it(`array-action rules matching "${line}" (${desc}) capture every char`, () => {
      for (const [re, action] of contentRules) {
        if (!Array.isArray(action)) continue;
        const m = line.match(re);
        if (!m) continue;
        const groupLen = m.slice(1).reduce((n, g) => n + (g?.length ?? 0), 0);
        expect(
          groupLen,
          `rule ${re} leaves ${m[0].length - groupLen} char(s) uncaptured — Monaco will throw mid-tokenise`,
        ).toBe(m[0].length);
      }
    });
  }
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
