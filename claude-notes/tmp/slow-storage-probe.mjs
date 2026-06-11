// Mechanism proof: does a slow storage adapter delay the websocket
// join/peer handshake in automerge-repo? Mirrors quarto-sync-client's
// connect(): new Repo({network, storage}) then waitForPeer(5000).
import { Repo } from '@automerge/automerge-repo';
import { WebSocketClientAdapter } from '@automerge/automerge-repo-network-websocket';

const url = process.argv[2] ?? 'ws://127.0.0.1:59776/ws';
const storageDelayMs = Number(process.argv[3] ?? 6000);

class SlowMemoryStorage {
  data = new Map();
  delay() { return new Promise(r => setTimeout(r, storageDelayMs)); }
  async load(key) { await this.delay(); return this.data.get(key.join('.')); }
  async save(key, value) { await this.delay(); this.data.set(key.join('.'), value); }
  async remove(key) { this.data.delete(key.join('.')); }
  async loadRange(prefix) {
    await this.delay();
    const p = prefix.join('.');
    return [...this.data.entries()]
      .filter(([k]) => k.startsWith(p))
      .map(([k, data]) => ({ key: k.split('.'), data }));
  }
  async removeRange() {}
}

function waitForPeer(repo, timeoutMs) {
  return new Promise((resolve, reject) => {
    const t = setTimeout(() => reject(new Error('Timeout waiting for peer connection')), timeoutMs);
    repo.networkSubsystem.on('peer', () => { clearTimeout(t); resolve(); });
  });
}

const t0 = performance.now();
const adapter = new WebSocketClientAdapter(url);
const repo = new Repo({ network: [adapter], storage: new SlowMemoryStorage() });
try {
  await waitForPeer(repo, 5000);
  console.log(`peer in ${(performance.now() - t0).toFixed(1)} ms — storage delay ${storageDelayMs} ms did NOT block handshake`);
} catch (e) {
  console.log(`TIMEOUT at ${(performance.now() - t0).toFixed(1)} ms with storage delay ${storageDelayMs} ms — storage gates the handshake`);
}
adapter.disconnect();
process.exit(0);
