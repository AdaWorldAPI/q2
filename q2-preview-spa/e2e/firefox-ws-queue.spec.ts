/**
 * Firefox WebSocket handshake-queue regression (bd-jit6pdwq Phase 4).
 *
 * Firefox serializes WebSocket *opening handshakes* per IP address
 * browser-wide (RFC 6455 §4.1, nsWSAdmissionManager): only one
 * WebSocket to 127.0.0.1 may be in CONNECTING at a time, across all
 * tabs. A tab holding a hung handshake (TCP accepted, HTTP 101 never
 * sent — e.g. a wedged or suspended local server) therefore starves
 * every other localhost WS handshake for up to
 * `network.websocket.timeout.open` per attempt.
 *
 * Pre-fix, the preview SPA budgeted 5 s for the samod peer event and
 * hard-failed into a *permanent* offline error when starved. The fix
 * (health-arbitrated boot, Phases 1-3) waits on the WebSocket as long
 * as HTTP `/health` answers. So the assertion here is STATE-based,
 * not latency-based: the preview must reach rendered content and must
 * never show the terminal error overlay. Pre-fix code fails this
 * deterministically; post-fix code passes once Firefox releases the
 * queue slot.
 *
 * Pinned pref (firefox-ws-queue project in playwright.config.ts):
 *   network.websocket.timeout.open = 8 (seconds)
 * Why 8: it must EXCEED the legacy 5 s peer budget — with a shorter
 * open-timeout the queue would free before the old deadline and the
 * pre-fix bug wouldn't reproduce — while keeping the spec fast
 * (default is 20 s). Do not lower it below ~6.
 *
 * Demotion policy (plan §Phase 4, agreed 2026-06-11): this spec is
 * never PR-blocking (runs under `cargo xtask verify --e2e` only). If
 * it flakes twice, demote it to a documented manual harness; the
 * bootController vitest specs are the durable regression net. Known
 * benign decay: if Firefox ever drops per-IP handshake serialization,
 * the blackhole stops mattering and this becomes a plain boot test.
 *
 * The same spec runs under the chromium project as the control:
 * Chromium does not serialize handshakes this way, so it renders
 * quickly with the blackhole present.
 *
 * Research: claude-notes/research/2026-06-11-firefox-ws-handshake-serialization.md
 */

import { test, expect, type Page } from '@playwright/test';
import { startPreviewServer, type PreviewServerHandle } from './helpers/previewServer';
import { startBlackhole, type BlackholeHandle } from './helpers/blackhole';

const FIXTURE = {
  fixtureFiles: [
    {
      path: 'index.qmd',
      content: `# Hello

Rendered despite a hostile WebSocket queue.
`,
    },
  ],
};

/** Wait for the sandboxed renderer iframe to show the heading. */
async function waitForInnerHeading(page: Page, text: string, timeout: number) {
  await page.waitForFunction(
    (expected) => {
      const outer = document.querySelector('iframe');
      const innerDoc = outer?.contentDocument;
      const h1 = innerDoc?.querySelector('h1');
      return h1 != null && h1.textContent === expected;
    },
    text,
    { timeout },
  );
}

test('preview renders despite a hung WebSocket handshake to another localhost port', async ({
  browser,
  browserName,
}) => {
  // Firefox must outlast queue-release cycles (8 s pinned pref); the
  // 30 s Playwright default would cut the 40 s render budget short.
  test.setTimeout(120_000);
  let blackhole: BlackholeHandle | null = null;
  let server: PreviewServerHandle | null = null;
  const context = await browser.newContext();
  try {
    blackhole = await startBlackhole();
    server = await startPreviewServer(FIXTURE);

    // Tab A: the stale-tab analog. Holds a perpetually-hung WebSocket
    // handshake to the blackhole port, reconnecting on every close —
    // in Firefox this occupies the browser-wide per-IP CONNECTING
    // slot for 127.0.0.1 almost continuously. The tab must be a real
    // http://127.0.0.1 page (we use the server's /health JSON):
    // sockets opened from about:blank don't enter the same admission
    // queue, and the production scenario is a real stale tab anyway.
    const tabA = await context.newPage();
    await tabA.goto(`${server.url}health`);
    await tabA.evaluate((port) => {
      const w = window as unknown as { __wsQueueProbe?: WebSocket };
      const tryConnect = () => {
        const ws = new WebSocket(`ws://127.0.0.1:${port}/ws`);
        w.__wsQueueProbe = ws;
        ws.onclose = () => setTimeout(tryConnect, 500);
        ws.onerror = () => {};
      };
      tryConnect();
    }, blackhole.port);
    // Precondition: tab A's handshake really is hung in CONNECTING.
    // If this fails, the blackhole isn't holding the slot and the
    // test would pass vacuously.
    await tabA.waitForTimeout(750);
    const probeState = await tabA.evaluate(
      () => (window as unknown as { __wsQueueProbe?: WebSocket }).__wsQueueProbe?.readyState,
    );
    expect(probeState).toBe(0); // 0 === WebSocket.CONNECTING

    // Tab B: the victim preview.
    const tabB = await context.newPage();
    const consoleErrors: string[] = [];
    tabB.on('console', (msg) => {
      if (msg.type() === 'error') consoleErrors.push(msg.text());
    });
    await tabB.goto(server.url);

    // State-based assertion: rendered content appears. Firefox needs
    // to outlast at least one open-timeout cycle (8 s pinned pref);
    // 40 s gives several cycles of slack without being latency-flaky.
    // Chromium (control) renders in a couple of seconds.
    const renderBudget = browserName === 'firefox' ? 40_000 : 15_000;
    await waitForInnerHeading(tabB, 'Hello', renderBudget);

    // And the terminal failure UI never took over.
    await expect(tabB.getByText(/render error/i)).toHaveCount(0);
    await expect(tabB.getByText(/not responding/i)).toHaveCount(0);
  } finally {
    await context.close();
    await server?.stop();
    await blackhole?.stop();
  }
});
