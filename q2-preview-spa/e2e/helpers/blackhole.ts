/**
 * A TCP "blackhole": accepts connections, reads, never replies, never
 * closes. Simulates a wedged/suspended local server whose WebSocket
 * endpoint holds browser handshakes in CONNECTING — the trigger for
 * Firefox's per-IP handshake-queue starvation (bd-jit6pdwq; see
 * claude-notes/research/2026-06-11-firefox-ws-handshake-serialization.md).
 */

import { createServer, type Server } from 'node:net';

export interface BlackholeHandle {
  port: number;
  stop(): Promise<void>;
}

export function startBlackhole(): Promise<BlackholeHandle> {
  return new Promise((resolve, reject) => {
    const sockets = new Set<import('node:net').Socket>();
    const server: Server = createServer((socket) => {
      sockets.add(socket);
      socket.on('data', () => {});
      socket.on('error', () => {});
      socket.on('close', () => sockets.delete(socket));
    });
    server.on('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      if (address === null || typeof address === 'string') {
        reject(new Error('blackhole: no port assigned'));
        return;
      }
      resolve({
        port: address.port,
        stop: () =>
          new Promise<void>((res) => {
            for (const socket of sockets) socket.destroy();
            server.close(() => res());
          }),
      });
    });
  });
}
