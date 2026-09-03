// Preloaded only into launcher-owned DSH. DSH still validates every request and
// issues its own signed cookie; this changes only a local browser's root 401.
const { Server } = require('node:http');

let entry;
let pending = '';
const stdoutWrite = process.stdout.write;
process.stdout.write = function (chunk, ...args) {
  pending += Buffer.isBuffer(chunk) ? chunk.toString('utf8') : String(chunk);
  let newline;
  while ((newline = pending.indexOf('\n')) !== -1) {
    const line = pending.slice(0, newline).trimEnd();
    pending = pending.slice(newline + 1);
    const match = /^dsh web: http:\/\/127\.0\.0\.1:(\d+)\/\?token=([A-Za-z0-9_-]{20,256})(?:\s|$)/.exec(line);
    if (match && Number(match[1]) > 0 && Number(match[1]) <= 65535) {
      entry = { port: Number(match[1]), token: match[2] };
    }
  }
  if (pending.length > 8192) pending = '';
  return Reflect.apply(stdoutWrite, this, [chunk, ...args]);
};

function localNavigation(req) {
  const h = req.headers;
  const loopback = (address) => address === '127.0.0.1' || address === '::1' || address === '::ffff:127.0.0.1';
  return entry !== undefined && req.method === 'GET' && req.url === '/' &&
    req.socket.localPort === entry.port && loopback(req.socket.localAddress) &&
    loopback(req.socket.remoteAddress) &&
    (h.host?.toLowerCase() === `localhost:${entry.port}` || h.host === `127.0.0.1:${entry.port}`) &&
    req.rawHeaders.filter((_, index) => index % 2 === 0 && req.rawHeaders[index].toLowerCase() === 'host').length === 1 &&
    h.origin === undefined && h.referer === undefined &&
    h['sec-fetch-site'] === 'none' && h['sec-fetch-mode'] === 'navigate' &&
    h['sec-fetch-dest'] === 'document';
}

const emit = Server.prototype.emit;
Server.prototype.emit = function (event, ...args) {
  const [req, res] = args;
  if (event === 'request' && localNavigation(req)) {
    const writeHead = res.writeHead;
    res.writeHead = function (status, ...headers) {
      // Let DSH accept an existing cookie first, preventing a redirect loop.
      // Invalid/expired cookies use the same token exchange as the Open button.
      if (status === 401 && !this.headersSent && localNavigation(req)) {
        return Reflect.apply(writeHead, this, [303, {
          location: `/?token=${entry.token}`,
          'cache-control': 'no-store',
          'referrer-policy': 'no-referrer',
        }]);
      }
      return Reflect.apply(writeHead, this, [status, ...headers]);
    };
  }
  return Reflect.apply(emit, this, [event, ...args]);
};
