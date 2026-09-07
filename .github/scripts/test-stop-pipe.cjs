const { test } = require('node:test');
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const path = require('node:path');
test('private stop pipe invokes the registered SIGTERM handler exactly once', async () => {
  const child = spawn(process.execPath, ['--require', path.resolve(__dirname, '../../src/browser_entry.cjs'), '-e',
    "let count=0;setInterval(()=>{},1000);process.on('SIGTERM',()=>{console.log('stopped:'+ ++count);setTimeout(()=>process.exit(0),100)});console.log('ready')"],
    { windowsHide: true, env: { ...process.env, DSH_LAUNCHER_STOP_PIPE: '1' }, stdio: ['pipe', 'pipe', 'pipe'] });
  let output = '';
  const timeout = setTimeout(() => child.kill(), 5000);
  try {
    child.stdout.on('data', chunk => {
      output += chunk;
      if (output === 'ready\n') { child.stdin.write('DSH_LAUNCHER_STOP\n'); setTimeout(() => child.stdin.write('DSH_LAUNCHER_STOP\n'), 20); }
    });
    const code = await new Promise((resolve, reject) => { child.on('error', reject); child.on('exit', resolve); });
    assert.equal(code, 0);
    assert.equal(output.match(/stopped:/g)?.length, 1);
  } finally { clearTimeout(timeout); child.kill(); }
});
