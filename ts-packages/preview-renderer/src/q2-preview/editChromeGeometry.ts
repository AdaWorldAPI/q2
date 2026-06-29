/**
 * editChromeGeometry.ts — shared vertical-placement geometry for floating edit
 * chrome (the rich-text toolbar and the standalone breadcrumb chip). Pure (no
 * DOM, no React) so it is unit-testable; the components measure rects and call
 * in. bd-pvcnea83.
 */

/**
 * True when floating chrome of `chromeHeight` placed ABOVE a surface whose
 * viewport-relative top edge is `surfaceTop` would be clipped above the viewport
 * top — i.e. there isn't `chromeHeight + gap` of room above it. The caller then
 * flips the chrome BELOW the surface instead.
 *
 * `surfaceTop` is the surface's `getBoundingClientRect().top` (viewport
 * coordinates; in the preview iframe, 0 is the top of the visible scroll area).
 * Computing from the *surface's* top (which doesn't move when the chrome flips)
 * keeps the decision stable — no flip/re-measure loop.
 */
export function shouldPlaceChromeBelow(
  surfaceTop: number,
  chromeHeight: number,
  gap: number,
): boolean {
  return surfaceTop - chromeHeight - gap < 0;
}
