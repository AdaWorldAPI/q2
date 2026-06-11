// Reproduce q2-preview peer-connection timeout in Firefox via Playwright.
// Usage: node firefox-repro.mjs [url] [iterations]
import { firefox } from 'playwright';

const url = process.argv[2] ?? 'http://127.0.0.1:59899/';
const iterations = Number(process.argv[3] ?? 15);

const browser = await firefox.launch();
let failures = 0;

for (let i = 0; i < iterations; i++) {
  const context = await browser.newContext(); // fresh storage each time
  const page = await context.newPage();
  const t0 = Date.now();
  const events = [];
  page.on('console', (msg) => {
    const text = msg.text();
    if (/peer|offline|Timeout|error|Error|unavailable/i.test(text)) {
      events.push(`[${Date.now() - t0}ms] console.${msg.type()}: ${text}`);
    }
  });
  page.on('websocket', (ws) => {
    events.push(`[${Date.now() - t0}ms] websocket opened: ${ws.url()}`);
    ws.on('close', () => events.push(`[${Date.now() - t0}ms] websocket closed`));
  });
  page.on('pageerror', (err) => events.push(`[${Date.now() - t0}ms] pageerror: ${err.message}`));

  try {
    await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 20000 });
    // give the SPA time to boot: wasm init + connect (5s peer budget) + render
    await page.waitForTimeout(9000);
  } catch (e) {
    events.push(`goto failed: ${e.message}`);
  }

  const failed = events.some((e) => /Timeout waiting for peer/.test(e));
  if (failed) failures++;
  console.log(`--- run ${i}${failed ? '  *** PEER TIMEOUT ***' : ''}`);
  for (const e of events) console.log('   ', e);
  await context.close();
}

console.log(`\n${failures}/${iterations} runs hit the peer timeout`);
await browser.close();
process.exit(0);
