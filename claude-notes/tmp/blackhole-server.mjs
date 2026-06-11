// TCP server that accepts connections and never responds — simulates a
// wedged/suspended q2 preview server holding WS handshakes in CONNECTING.
import net from 'node:net';
const port = Number(process.argv[2] ?? 59001);
const server = net.createServer((socket) => {
  // accept, read, never write, never close
  socket.on('data', () => {});
  socket.on('error', () => {});
});
server.listen(port, '127.0.0.1', () => console.log(`blackhole listening on ${port}`));
