const http = require('node:http');
const path = require('node:path');
const { spawn } = require('node:child_process');

if (process.argv.includes('--fixture')) {
  const token = 'test-launch-token-with-sufficient-length';
  const server = http.createServer((req, res) => {
    if (req.headers.host === 'blocked.invalid') { res.writeHead(403); res.end('blocked'); return; }
    if (req.url === `/?token=${token}`) {
      res.writeHead(303, { location: '/', 'set-cookie': 'dsh-test=valid; HttpOnly; Path=/' });
      res.end(); return;
    }
    const ready = req.headers.cookie === 'dsh-test=valid';
    res.writeHead(ready ? 200 : 401);
    res.end(ready ? '__DSH_BOOT__' : 'dsh web authentication required');
  });
  server.listen(0, '127.0.0.1', () => {
    const port = server.address().port;
    process.stdout.write('中'.repeat(9000) + '\n');
    process.stdout.write(`dsh web: http://127.0.0.1:${port}/?tok`);
    setImmediate(() => {
      process.stdout.write(`en=${token}\n`);
      process.stdout.write(`fixture-ready:${port}\n`);
    });
  });
} else {
  const { test } = require('node:test');
  const assert = require('node:assert/strict');
  const navigation = { 'sec-fetch-site': 'none', 'sec-fetch-mode': 'navigate', 'sec-fetch-dest': 'document' };
  test('local browser entry preserves DSH authentication boundaries', async () => {
    const preload = path.resolve(__dirname, '../../src/browser_entry.cjs');
    const child = spawn(process.execPath, ['--require', preload, __filename, '--fixture'], { stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
    try {
      const port = await new Promise((resolve, reject) => {
        let stdout = '';
        const timeout = setTimeout(() => reject(new Error('Fixture timeout')), 10000);
        child.once('error', reject);
        child.stdout.on('data', (chunk) => {
          stdout += chunk;
          const match = /fixture-ready:(\d+)/.exec(stdout);
          if (match) { clearTimeout(timeout); resolve(Number(match[1])); }
        });
        child.stderr.on('data', (chunk) => reject(new Error(String(chunk))));
      });
      const request = (headers = {}, url = '/', method = 'GET') => new Promise((resolve, reject) => {
        const req = http.request({ hostname: '127.0.0.1', port, path: url, method, headers, agent: false }, (res) => {
          let body = '';
          res.on('data', (chunk) => body += chunk);
          res.on('end', () => resolve({ status: res.statusCode, headers: res.headers, body }));
        });
        req.on('error', reject); req.end();
      });
      for (const host of [`127.0.0.1:${port}`, `localhost:${port}`]) {
        const first = await request({ ...navigation, host });
        assert.equal(first.status, 303);
        assert.equal(first.headers['cache-control'], 'no-store');
        assert.equal(first.headers['referrer-policy'], 'no-referrer');
        const exchange = await request({ host }, first.headers.location);
        assert.equal(exchange.status, 303);
        const cookie = exchange.headers['set-cookie'][0].split(';')[0];
        assert.equal((await request({ ...navigation, host, cookie })).status, 200);
        assert.equal((await request({ ...navigation, host, cookie: 'dsh-test=expired' })).status, 303);
      }
      for (const headers of [
        {}, { ...navigation, 'sec-fetch-site': 'cross-site' },
        { ...navigation, 'sec-fetch-site': 'same-site' },
        { ...navigation, 'sec-fetch-site': 'same-origin' },
        { ...navigation, 'sec-fetch-dest': 'iframe' },
        { ...navigation, 'sec-fetch-mode': 'cors' },
        { ...navigation, origin: `http://localhost:${port}` },
        { ...navigation, referer: 'http://external.invalid/' },
        { ...navigation, host: `rebind.invalid:${port}` },
        { ...navigation, host: `localhost:${port + 1}` },
      ]) assert.equal((await request(headers)).status, 401);
      assert.equal((await request(navigation, '/api/private')).status, 401);
      assert.equal((await request(navigation, '/', 'POST')).status, 401);
      assert.equal((await request(navigation, '/', 'HEAD')).status, 401);
      assert.equal((await request({ ...navigation, host: 'blocked.invalid' })).status, 403);
    } finally { child.kill(); }
  });
}
