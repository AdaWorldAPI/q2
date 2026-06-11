// Does a hung WebSocket handshake to 127.0.0.1:<portA> delay a fresh
// WebSocket handshake to 127.0.0.1:<portB> in the same browser?
// Usage: node firefox-serialization-test.mjs <browser:firefox|chromium> <previewUrl> <blackholePort>
import { firefox, chromium } from 'playwright';

const browserName = process.argv[2] ?? 'firefox';
const previewUrl = process.argv[3] ?? 'http://127.0.0.1:59899/';
const blackholePort = Number(process.argv[4] ?? 59001);

const browser = await (browserName === 'firefox' ? firefox : chromium).launch();
const context = await browser.newContext();

// Tab A: stale preview tab analog — WS to the blackhole port, retrying.
const tabA = await context.newPage();
await tabA.goto(previewUrl); // same-origin page so we can open ws from it
await tabA.evaluate((port) => {
  const tryConnect = () => {
    const ws = new WebSocket(`ws://127.0.0.1:${port}/ws`);
    ws.onclose = () => setTimeout(tryConnect, 500);
    ws.onerror = () => {};
  };
  tryConnect();
}, blackholePort);
console.log('tab A: hung-handshake websocket started');

// Give tab A's handshake a moment to enter CONNECTING.
await new Promise((r) => setTimeout(r, 1500));

// Tab B: fresh preview load. Measure time to peer.
const tabB = await context.newPage();
const t0 = Date.now();
const events = [];
tabB.on('console', (msg) => {
  const text = msg.text();
  if (/peer|offline|Timeout/i.test(text)) {
    events.push(`[${Date.now() - t0}ms] ${msg.type()}: ${text}`);
  }
});
tabB.on('websocket', (ws) => events.push(`[${Date.now() - t0}ms] websocket opened: ${ws.url()}`));
await tabB.goto(previewUrl, { waitUntil: 'domcontentloaded' });
await tabB.waitForTimeout(12000);

console.log(`\n=== ${browserName} tab B events ===`);
for (const e of events) console.log('  ', e);
await browser.close();
process.exit(0);
