// Instrumented variant: trace storage id() resolution and adapter.connect timing.
import { Repo } from '@automerge/automerge-repo';
import { WebSocketClientAdapter } from '@automerge/automerge-repo-network-websocket';

const url = process.argv[2] ?? 'ws://127.0.0.1:59776/ws';
const storageDelayMs = Number(process.argv[3] ?? 0);
const t0 = performance.now();
const ts = () => (performance.now() - t0).toFixed(1) + ' ms';

class SlowMemoryStorage {
  data = new Map();
  delay() { return new Promise(r => setTimeout(r, storageDelayMs)); }
  async load(key) {
    await this.delay();
    console.log(`[${ts()}] storage.load(${key.join('.')})`);
    return this.data.get(key.join('.'));
  }
  async save(key, value) {
    await this.delay();
    console.log(`[${ts()}] storage.save(${key.join('.')})`);
    this.data.set(key.join('.'), value);
  }
  async remove() {}
  async loadRange(prefix) {
    await this.delay();
    console.log(`[${ts()}] storage.loadRange(${prefix.join('.')})`);
    return [];
  }
  async removeRange() {}
}

const adapter = new WebSocketClientAdapter(url);
const origConnect = adapter.connect.bind(adapter);
adapter.connect = (...args) => {
  console.log(`[${ts()}] adapter.connect() called`);
  return origConnect(...args);
};

const repo = new Repo({ network: [adapter], storage: new SlowMemoryStorage() });
const result = await new Promise((resolve) => {
  const t = setTimeout(() => resolve('timeout'), 5000);
  repo.networkSubsystem.on('peer', (p) => { clearTimeout(t); resolve('peer:' + p.peerId); });
});
console.log(`[${ts()}] result: ${result}`);
process.exit(0);
