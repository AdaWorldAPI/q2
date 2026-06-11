// bd-jit6pdwq: while the server is gone, the SPA must make ZERO WebSocket
// attempts (HTTP /health polling only). Counts page websocket events
// during a 12s dead window.
import { firefox } from 'playwright';
import { spawn } from 'node:child_process';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';

const port = 59322;
const projectDir = await mkdtemp(path.join(tmpdir(), 'q2-ws-churn-'));
await writeFile(path.join(projectDir, 'index.qmd'), '# Hello\n');
const proc = spawn('target/debug/q2', ['preview', '--no-browser', '--port', String(port), projectDir], { stdio: ['ignore', 'pipe', 'pipe'] });
await new Promise((res, rej) => {
  let out = '';
  proc.stdout.on('data', (c) => { out += c; if (/→\s+http/.test(out)) res(); });
  setTimeout(() => rej(new Error('no URL')), 20000);
});

const browser = await firefox.launch();
const page = await browser.newPage();
let wsAttempts = 0;
page.on('websocket', () => wsAttempts++);
await page.goto(`http://127.0.0.1:${port}/`);
await page.waitForFunction(() => document.querySelector('iframe')?.contentDocument?.querySelector('h1') != null, { timeout: 30000 });
const bootAttempts = wsAttempts;
console.log(`boot WS attempts: ${bootAttempts}`);

proc.kill('SIGKILL');
// Measure from the moment the SPA declares the server gone — attempts
// before that belong to the (legitimate) reconnect probing phase.
await page.waitForSelector('text=/server stopped/i', { timeout: 60000 });
await new Promise((r) => setTimeout(r, 2000));
const baseline = wsAttempts;
await new Promise((r) => setTimeout(r, 12000));
console.log(`WS attempts during 12s dead window: ${wsAttempts - baseline}`);
console.log(wsAttempts - baseline === 0 ? 'PASS: zero WS churn while server gone' : 'FAIL: WS churn detected');
await browser.close();
process.exit(0);
