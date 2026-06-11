// bd-jit6pdwq Phase 5 end-to-end verification (Phase 3 manual check,
// scripted): kill the server under an open preview tab → banner; restart
// on the same port → automatic recovery, no reload.
//
// Drives the REAL q2 binary and a REAL Firefox (Playwright build).
// Usage: node verify-kill-restart.mjs [path-to-q2] [port]
import { firefox } from 'playwright';
import { spawn } from 'node:child_process';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';

const q2 = process.argv[2] ?? 'target/debug/q2';
const port = Number(process.argv[3] ?? 59321);

const projectDir = await mkdtemp(path.join(tmpdir(), 'q2-kill-restart-'));
await writeFile(path.join(projectDir, 'index.qmd'), '# Recovery\n\nbody text\n');

function startServer() {
  const proc = spawn(q2, ['preview', '--no-browser', '--port', String(port), projectDir], {
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  return new Promise((resolve, reject) => {
    let out = '';
    const onData = (c) => {
      out += c.toString();
      if (/→\s+http/.test(out)) resolve(proc);
    };
    proc.stdout.on('data', onData);
    proc.stderr.on('data', onData);
    proc.on('exit', (code) => {
      log(`server process exited (code ${code})`);
      reject(new Error(`server exited early (${code}): ${out}`));
    });
    setTimeout(() => reject(new Error('server did not print URL: ' + out)), 20000);
  });
}

const log = (msg) => console.log(`[${new Date().toISOString().slice(11, 19)}] ${msg}`);

async function pollHealth() {
  for (let i = 0; i < 60; i++) {
    try {
      return await (await fetch(`http://127.0.0.1:${port}/health`)).json();
    } catch {
      await new Promise((r) => setTimeout(r, 500));
    }
  }
  throw new Error('server never answered /health');
}

let server = await startServer();
log(`server up on :${port}`);
const health1 = await pollHealth();
log(`docId before kill: ${health1.index_document_id}`);

const browser = await firefox.launch();
const page = await browser.newPage();
page.on('console', (msg) => {
  const t = msg.text();
  if (/peer|offline|unavailable|error|gone|health|connect/i.test(t)) log(`console.${msg.type()}: ${t.slice(0, 140)}`);
});
await page.goto(`http://127.0.0.1:${port}/`);
await page.waitForFunction(() => {
  const h1 = document.querySelector('iframe')?.contentDocument?.querySelector('h1');
  return h1?.textContent === 'Recovery';
}, { timeout: 30000 });
log('preview rendered');

// Kill the server under the open tab.
server.kill('SIGKILL');
log('server killed');

await page.waitForSelector('text=/server stopped|not responding|reconnecting/i', { timeout: 60000 });
const bannerText = await page.locator('[role="status"]').textContent();
log(`banner shown: "${bannerText}"`);

// While dead: confirm content is still up (not the terminal overlay).
const headingStillUp = await page.evaluate(() => {
  const h1 = document.querySelector('iframe')?.contentDocument?.querySelector('h1');
  return h1?.textContent === 'Recovery';
});
log(`content still rendered while server gone: ${headingStillUp}`);

// Restart on the same port; the tab must recover with NO reload.
await new Promise((r) => setTimeout(r, 3000));
server = await startServer();
log('server restarted on same port (URL printed; awaiting bind)');
// q2 preview prints its URL BEFORE binding; poll /health until it
// actually answers (mirrors e2e/helpers/previewServer.ts waitForBind).
const health2 = await pollHealth();
log(`docId after restart: ${health2.index_document_id}`);

await page.waitForFunction(
  () => document.querySelector('[role="status"]') === null,
  undefined,
  { timeout: 90000 },
);
log('banner cleared — tab reconnected automatically (no reload)');

const finalHeading = await page.evaluate(() => {
  const h1 = document.querySelector('iframe')?.contentDocument?.querySelector('h1');
  return h1?.textContent;
});
log(`final rendered heading: "${finalHeading}"`);

await browser.close();
server.kill('SIGTERM');
console.log('\nPASS: kill → banner (content retained) → restart → automatic recovery');
process.exit(0);
