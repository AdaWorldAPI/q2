// Probe: time the samod/automerge-repo peer handshake against a running
// q2 preview server. Usage: node peer-handshake-probe.mjs [url] [iterations]
import { Repo } from '@automerge/automerge-repo';
import { WebSocketClientAdapter } from '@automerge/automerge-repo-network-websocket';

const url = process.argv[2] ?? 'ws://127.0.0.1:59776/ws';
const iterations = Number(process.argv[3] ?? 10);

async function once(i) {
  const t0 = performance.now();
  const adapter = new WebSocketClientAdapter(url);
  const repo = new Repo({ network: [adapter] });
  const result = await new Promise((resolve) => {
    const timeout = setTimeout(() => resolve({ ok: false, ms: performance.now() - t0 }), 10000);
    repo.networkSubsystem.on('peer', (p) => {
      clearTimeout(timeout);
      resolve({ ok: true, ms: performance.now() - t0, peerId: p.peerId });
    });
  });
  adapter.disconnect();
  console.log(`run ${i}: ${result.ok ? 'peer in ' + result.ms.toFixed(1) + ' ms (' + result.peerId + ')' : 'TIMEOUT after ' + result.ms.toFixed(0) + ' ms'}`);
  return result;
}

let failures = 0;
for (let i = 0; i < iterations; i++) {
  const r = await once(i);
  if (!r.ok) failures++;
}
console.log(`\n${failures}/${iterations} timed out`);
process.exit(0);
