const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {fork,execFileSync} = require('node:child_process');
const update = require('../../src/plugin_update.cjs');
const npmCli=process.env.DSH_TEST_NPM_CLI;

test('real npm stages selected prerelease, preserves unselected lock and local source, and detects concurrent edits', {timeout:90000}, async () => {
  assert(npmCli,'DSH_TEST_NPM_CLI is required');
  const root=fs.mkdtempSync(path.resolve(__dirname,'../../.tmp-plugin-update-'));
  const pack=path.join(root,'pack');fs.mkdirSync(pack);
  let server;
  const npm=(cwd,args)=>execFileSync(process.execPath,[npmCli,...args,'--ignore-scripts','--no-audit','--no-fund'],{cwd,windowsHide:true,timeout:20000,stdio:'pipe',env:{...process.env,npm_config_cache:path.join(root,'cache'),npm_config_fetch_retries:'0',npm_config_fetch_timeout:'2000'}});
  try {
    for(const name of ['qa-selected','qa-unselected']) for(const version of ['1.0.0','2.0.0-beta.2','2.0.0-beta.10']) {
      fs.writeFileSync(path.join(pack,'package.json'),JSON.stringify({name,version}));
      npm(pack,['pack','--pack-destination',root]);
    }
    server=fork(path.join(__dirname,'plugin-registry-fixture.cjs'),[root],{windowsHide:true,stdio:['ignore','ignore','pipe','ipc']});
    const port=await new Promise((resolve,reject)=>{server.once('message',resolve);server.once('error',reject);server.once('exit',()=>reject(new Error('registry exited')));});
    const profile=path.join(root,'web');fs.mkdirSync(profile);
    const manifest={name:'qa-profile',version:'1.0.0',dependencies:{'qa-selected':'1.0.0','qa-unselected':'1.0.0'}};
    fs.writeFileSync(path.join(profile,'package.json'),JSON.stringify(manifest));
    fs.writeFileSync(path.join(profile,'.npmrc'),`registry=http://127.0.0.1:${port}/\nfetch-retries=0\nfetch-timeout=2000\n`);
    npm(profile,['install']);
    const originalLock=JSON.parse(fs.readFileSync(path.join(profile,'package-lock.json')));
    const local=path.join(root,'local');fs.mkdirSync(local);fs.writeFileSync(path.join(local,'package.json'),JSON.stringify({name:'local',version:'1.0.0'}));
    fs.symlinkSync(local,path.join(profile,'node_modules','local'),'junction');
    const copied=path.join(profile,'node_modules','copied');fs.mkdirSync(copied);fs.writeFileSync(path.join(copied,'package.json'),JSON.stringify({name:'copied',version:'1.0.0'}));
    manifest.dependencies.copied='git+https://example.invalid/copied.git#abcdef';
    manifest.dependencies.local='link:../local';fs.writeFileSync(path.join(profile,'package.json'),JSON.stringify(manifest));
    const before=fs.readFileSync(path.join(profile,'package.json'));
    const request={npmCli,temp:root,settingsFile:path.join(root,'settings'),catalog:{complete:true,profileDir:profile,plugins:[
      {name:'qa-selected',enabled:true,supported:true},{name:'qa-unselected',enabled:false,supported:true},{name:'local',enabled:true,supported:true},
    ]}};
    const plan=update.prepare(request);
    assert.equal(plan.updates.length,1); assert.equal(plan.updates[0].target,'2.0.0-beta.10');
    update.install(request,plan);
    assert.deepEqual(fs.readFileSync(path.join(profile,'package.json')),before);
    const next=JSON.parse(fs.readFileSync(path.join(plan.candidate,'package.json')));
    assert.equal(next.dependencies['qa-selected'],'2.0.0-beta.10');assert.equal(next.dependencies['qa-unselected'],'1.0.0');assert.equal(next.dependencies.local,'link:../local');
    const nextLock=JSON.parse(fs.readFileSync(path.join(plan.candidate,'package-lock.json')));
    assert.deepEqual(nextLock.packages['node_modules/qa-unselected'],originalLock.packages['node_modules/qa-unselected']);
    assert.equal(fs.realpathSync(path.join(plan.candidate,'node_modules/local')),local);
    const candidateCopy=path.join(plan.candidate,'node_modules/copied');
    assert(!fs.lstatSync(candidateCopy).isSymbolicLink());
    assert.equal(JSON.parse(fs.readFileSync(path.join(candidateCopy,'package.json'))).version,'1.0.0');
    assert.equal(next.dependencies.copied,manifest.dependencies.copied);
    update.check(plan);fs.writeFileSync(request.settingsFile,'changed');assert.throws(()=>update.check(plan),/已变化/);
    update.removeTree(plan.root);assert(fs.existsSync(path.join(local,'package.json')));
    // A different current source must fail before npm can query it.
    fs.writeFileSync(path.join(profile,'.npmrc'),'registry=http://127.0.0.1:9/');
    assert.throws(()=>update.prepare(request),/锁文件来源/);
    fs.writeFileSync(path.join(profile,'.npmrc'),`registry=http://127.0.0.1:${port}/\nfetch-retries=0\nfetch-timeout=1000\n`);
    const exited=new Promise(resolve=>server.once('exit',resolve));server.kill();await exited;server=undefined;
    assert.throws(()=>update.prepare(request));
    assert.deepEqual(fs.readFileSync(path.join(profile,'package.json')),before);
    assert(!fs.existsSync(plan.root));
  } finally {
    if(server) {server.kill();await new Promise(resolve=>server.once('exit',resolve));}
    update.removeTree(root);
  }
});
