/**
 * Unit tests for shouldPlaceChromeBelow (bd-pvcnea83): floating edit chrome
 * flips below the surface when there isn't room above it at the viewport top.
 */

import { describe, it, expect } from 'vitest';
import { shouldPlaceChromeBelow } from './editChromeGeometry';

describe('shouldPlaceChromeBelow', () => {
  it('flips below when the chrome would clip above the viewport top (the measured first-block case)', () => {
    // editor top 15, toolbar height 26, gap 4 → 15 - 26 - 4 = -15 < 0.
    expect(shouldPlaceChromeBelow(15, 26, 4)).toBe(true);
  });

  it('stays above when there is ample room above', () => {
    expect(shouldPlaceChromeBelow(100, 26, 4)).toBe(false);
  });

  it('stays above with exactly enough room (chromeHeight + gap)', () => {
    // surfaceTop === chromeHeight + gap → 30 - 26 - 4 = 0, not < 0.
    expect(shouldPlaceChromeBelow(30, 26, 4)).toBe(false);
  });

  it('flips below one pixel short of enough room', () => {
    expect(shouldPlaceChromeBelow(29, 26, 4)).toBe(true);
  });

  it('flips below for a surface flush at the viewport top', () => {
    expect(shouldPlaceChromeBelow(0, 26, 4)).toBe(true);
  });
});
