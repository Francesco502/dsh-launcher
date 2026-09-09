const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const repair = require('../../src/plugin_repair.cjs');
const digest = f => fs.existsSync(f) ? crypto.createHash('sha256').update(fs.readFileSync(f)).digest('hex') : null;

test('orphan preparation is removed only when no backup needs preservation', t => {
  const {p} = fixture(t);
  fs.unlinkSync(p.journal);
  repair.recover(p.profile);
  assert(!fs.existsSync(p.root));
  assert(fs.existsSync(path.join(p.profile, 'node_modules', 'old')));
  fs.mkdirSync(p.backup, {recursive:true});
  fs.writeFileSync(path.join(p.backup, 'preserve'), 'original');
  assert.throws(() => repair.recover(p.profile), /保留备份/);
  assert.equal(fs.readFileSync(path.join(p.backup, 'preserve'), 'utf8'), 'original');
});

function fixture(t, oldModules=true) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(),'dsh-plugin-repair-'));
  t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const profile=path.join(root,'web');fs.mkdirSync(profile);
  fs.writeFileSync(path.join(profile,'package.json'),JSON.stringify({dependencies:{'qa-plugin':'1.2.3'}}));
  fs.writeFileSync(path.join(profile,'package-lock.json'),'old-lock');
  fs.writeFileSync(path.join(profile,'cordis.patch.yml'),'user-setting');
  if(oldModules) { fs.mkdirSync(path.join(profile,'node_modules'));fs.writeFileSync(path.join(profile,'node_modules','old'),'old-module'); }
  const p=repair.locations(profile);fs.mkdirSync(p.candidate,{recursive:true});fs.mkdirSync(p.backup);
  fs.mkdirSync(path.join(p.candidate,'node_modules'));fs.writeFileSync(path.join(p.candidate,'node_modules','new'),'new-module');
  fs.writeFileSync(path.join(p.candidate,'package-lock.json'),'new-lock');
  const record={profile:p.profile,phase:'prepared',manifest:digest(path.join(profile,'package.json')),lock:digest(path.join(profile,'package-lock.json')),original:{node_modules:oldModules,'package-lock.json':true}};
  fs.writeFileSync(p.journal,JSON.stringify(record));return {p,record};
}
test('commit replaces dependency group but preserves profile configuration',t=>{
  const {p}=fixture(t);repair.commit(p.profile);
  assert.equal(fs.readFileSync(path.join(p.profile,'node_modules','new'),'utf8'),'new-module');
  assert.equal(fs.readFileSync(path.join(p.profile,'cordis.patch.yml'),'utf8'),'user-setting');
  repair.recover(p.profile);assert(!fs.existsSync(p.root));
  assert.equal(fs.readFileSync(path.join(p.profile,'package-lock.json'),'utf8'),'new-lock');
});
test('configuration changed after confirmation prevents committing',t=>{
  const {p}=fixture(t);fs.writeFileSync(path.join(p.profile,'package.json'),'changed');
  assert.throws(()=>repair.commit(p.profile),/已变化/);
  assert(fs.existsSync(path.join(p.profile,'node_modules','old')));
});
for(const step of ['old-moved','new-moved','lock-moved']) test(`interruption at ${step} restores original dependency group`,t=>{
  const {p,record}=fixture(t);record.phase='committing';fs.writeFileSync(p.journal,JSON.stringify(record));
  fs.renameSync(path.join(p.profile,'node_modules'),path.join(p.backup,'node_modules'));
  if(step!=='old-moved')fs.renameSync(path.join(p.candidate,'node_modules'),path.join(p.profile,'node_modules'));
  if(step==='lock-moved')fs.renameSync(path.join(p.profile,'package-lock.json'),path.join(p.backup,'package-lock.json'));
  repair.recover(p.profile);
  assert(fs.existsSync(path.join(p.profile,'node_modules','old')));
  assert.equal(fs.readFileSync(path.join(p.profile,'package-lock.json'),'utf8'),'old-lock');
});
test('interrupted first install removes only new dependency group',t=>{
  const {p,record}=fixture(t,false);record.phase='committing';fs.writeFileSync(p.journal,JSON.stringify(record));
  fs.renameSync(path.join(p.candidate,'node_modules'),path.join(p.profile,'node_modules'));
  repair.recover(p.profile);assert(!fs.existsSync(path.join(p.profile,'node_modules')));
  assert.equal(fs.readFileSync(path.join(p.profile,'package-lock.json'),'utf8'),'old-lock');
});
test('local dependency source is rejected before any installation',t=>{
  const {p}=fixture(t);repair.recover(p.profile);
  fs.writeFileSync(path.join(p.profile,'package.json'),JSON.stringify({dependencies:{p:'file:../missing'}}));
  assert.throws(()=>repair.prepare(p.profile,'unused','unused'),/本地依赖/);
  assert(!fs.existsSync(p.root));
});

test('missing local link reconnects original source without modifying manifest or lock',t=>{
  const {p}=fixture(t);repair.recover(p.profile);
  const source=path.join(path.dirname(p.profile),'local-plugin');fs.mkdirSync(source);
  fs.writeFileSync(path.join(source,'package.json'),JSON.stringify({name:'@qa/local',version:'0.2.4'}));
  fs.writeFileSync(path.join(p.profile,'package.json'),JSON.stringify({dependencies:{'@qa/local':'link:'+source}}));
  const before=digest(path.join(p.profile,'package.json')),lock=digest(path.join(p.profile,'package-lock.json'));
  const plan=repair.prepare(p.profile,'unused','unused');assert.equal(plan.kind,'links');
  repair.install(p.profile,'unused','unused');repair.commit(p.profile);repair.recover(p.profile);
  assert.equal(fs.realpathSync(path.join(p.profile,'node_modules','@qa/local')),fs.realpathSync(source));
  assert.equal(digest(path.join(p.profile,'package.json')),before);assert.equal(digest(path.join(p.profile,'package-lock.json')),lock);
  // Fixture cleanup removes the link, not its original source.
  fs.unlinkSync(path.join(p.profile,'node_modules','@qa/local'));
  assert(fs.existsSync(path.join(source,'package.json')));
});
