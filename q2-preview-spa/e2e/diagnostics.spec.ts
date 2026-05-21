/**
 * Diagnostics surface end-to-end (bd-b9kzg).
 *
 * Boots `q2 preview` against a fixture project whose `index.qmd`
 * contains a body link to a sibling that doesn't exist in the
 * project index — the same `[Q-13-4]` "Body link references
 * missing document" warning the user's `q2 render` example
 * produced. After the SPA boots and renders, the
 * `PreviewDiagnosticsOverlay`'s collapsed warning indicator
 * should be visible.
 *
 * What this spec pins:
 *   - Boot path through to a successful render with a known
 *     warning carried in `result.warnings`.
 *   - PreviewApp's success-with-warnings branch routing the
 *     payload through the overlay's collapsed indicator.
 *   - The user-visible affordance: a "Warning" button surfaces
 *     in the iframe overlay area.
 *
 * Out of scope for this spec (covered by the SPA integration
 * tests instead):
 *   - Server-side `/api/preview/diagnostics` feed shaping. The
 *     vitest integration tests stub that endpoint directly; an
 *     e2e trigger would require a deliberately-failing capture
 *     (engine-not-found etc.) which is brittle to wire up here.
 *   - Expanded-mode rendering of structured diagnostics. The
 *     overlay's own unit tests cover the expanded layout.
 */

import { test, expect, type Page } from '@playwright/test';
import { startPreviewServer, type PreviewServerHandle } from './helpers/previewServer';

// `[Q-13-4]` body-link-references-missing-document. The link target
// (`missing.qmd`) is deliberately not in the fixture, so
// `LinkResolutionStage` (in the q2-preview WASM pipeline) flags it
// as a warning during render.
const INDEX_WITH_BAD_LINK = `# Index

This page links to [a missing sibling](./missing.qmd).
`;

let server: PreviewServerHandle;

test.beforeEach(async () => {
  server = await startPreviewServer({
    fixtureFiles: [{ path: 'index.qmd', content: INDEX_WITH_BAD_LINK }],
  });
});

test.afterEach(async () => {
  await server?.stop();
});

async function waitForFirstRender(page: Page): Promise<void> {
  await page.waitForFunction(
    () => {
      const w = window as unknown as { __renderTicks?: number };
      return (w.__renderTicks ?? 0) >= 1;
    },
    null,
    { timeout: 30_000 },
  );
}

test('body-link warning surfaces a collapsed "Warning" indicator in the overlay', async ({
  page,
}) => {
  await page.goto(server.url);
  await waitForFirstRender(page);

  // The collapsed-mode indicator is a button with the text
  // "Warning" (warnings-mode label per the forked overlay's
  // severity prop). Wait up to 5 s for it to surface after the
  // first render — the render-effect populates `render.warnings`
  // synchronously off the WASM result, so it should be near-
  // instant once the render completes.
  await expect(page.getByRole('button', { name: /^warning$/i })).toBeVisible({
    timeout: 5_000,
  });
});
