// Real npm/network and Windows file-lock checks; run in a disposable repository fixture.
const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {spawn} = require('node:child_process');
const repair = require('../../src/plugin_repair.cjs');
const root = path.resolve(__dirname, '../../.tmp-release-050/plugin-integration');
const npmCli = process.env.DSH_TEST_NPM_CLI;

function fixture(name) {
  const profile = path.join(root, name, 'web');
  assert(!fs.existsSync(profile), 'Use a fresh fixture directory');
  fs.mkdirSync(path.join(profile, 'node_modules'), {recursive:true});
  fs.writeFileSync(path.join(profile, 'node_modules', 'original'), 'preserve');
  const pkg = {name:'repair-qa',version:'1.0.0',dependencies:{'@francescoli/dsh-quota':'0.2.4'}};
  fs.writeFileSync(path.join(profile, 'package.json'), JSON.stringify(pkg));
  fs.writeFileSync(path.join(profile, 'package-lock.json'), JSON.stringify({name:pkg.name,version:pkg.version,lockfileVersion:3,packages:{'':pkg,'node_modules/@francescoli/dsh-quota':{version:'0.2.4',resolved:'http://127.0.0.1:9/quota.tgz',integrity:'sha512-'+Buffer.alloc(64).toString('base64')}}}));
  return profile;
}

test('real npm connection failure preserves the original dependency group', {skip:!npmCli,timeout:30000}, () => {
  const profile = fixture('network-' + process.pid);
  const before = fs.readFileSync(path.join(profile, 'package-lock.json'));
  const temp = path.join(root, 'temp'); fs.mkdirSync(temp,{recursive:true});
  const plan = repair.prepare(profile,npmCli,temp);
  assert.equal(plan.packages[0].source,'http://127.0.0.1:9/quota.tgz');
  const saved = {...process.env};
  try {
    process.env.npm_config_fetch_retries='0';
    process.env.npm_config_fetch_timeout='1000';
    process.env.npm_config_registry='http://127.0.0.1:9/';
    assert.throws(()=>repair.install(profile,npmCli,temp), e=> /ECONNREFUSED/.test(String(e.stderr)));
  } finally { process.env = saved; }
  repair.recover(profile);
  assert.deepEqual(fs.readFileSync(path.join(profile,'package-lock.json')),before);
  assert.equal(fs.readFileSync(path.join(profile,'node_modules','original'),'utf8'),'preserve');
});

test('real Windows lock rolls back a partially swapped dependency group', {skip:process.platform!=='win32',timeout:20000}, async () => {
  const profile = fixture('locked-' + process.pid);
  const p = repair.locations(profile), temp = path.join(root,'temp');
  repair.prepare(profile,npmCli,temp);
  fs.mkdirSync(path.join(p.candidate,'node_modules'));
  fs.writeFileSync(path.join(p.candidate,'node_modules','replacement'),'candidate');
  const ready = path.join(p.root,'lock-ready');
  const quote = text => "'" + text.replaceAll("'","''") + "'";
  const script = `$f=[IO.File]::Open(${quote(path.join(profile,'package-lock.json'))},'Open','Read','Read'); try { [IO.File]::WriteAllText(${quote(ready)},'ready'); Start-Sleep -Seconds 15 } finally {$f.Dispose()}`;
  const holder = spawn('powershell.exe',['-NoProfile','-NonInteractive','-Command',script],{windowsHide:true,stdio:'ignore'});
  try {
    const deadline=Date.now()+5000;
    while(!fs.existsSync(ready)) { assert(Date.now()<deadline,'File lock helper timed out'); await new Promise(r=>setTimeout(r,20)); }
    assert.throws(()=>repair.commit(profile),/EPERM|EACCES|EBUSY/);
    assert.equal(fs.readFileSync(path.join(profile,'node_modules','original'),'utf8'),'preserve');
    assert(!fs.existsSync(path.join(profile,'node_modules','replacement')));
  } finally {
    holder.kill();
    await new Promise(resolve=>holder.once('exit',resolve));
  }
  repair.recover(profile);
});
