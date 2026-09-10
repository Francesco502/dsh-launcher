const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const {createRequire} = require('node:module');
const update = require('../../src/plugin_update.cjs');

test('update selection uses enabled entries, deduplicates aliases and excludes builtins', () => {
  assert.deepEqual(update.selected({complete:true, plugins:[
    {name:'one',enabled:true,supported:true,aliases:['one','alias','one']},
    {name:'off',enabled:false,supported:true,aliases:['off']},
    {name:'dsh',enabled:true,supported:true,builtin:true,aliases:['dsh']},
  ]}), ['one','alias']);
  assert.throws(()=>update.selected({complete:false}), /不完整/);
  assert.throws(()=>update.selected({complete:true,plugins:[{name:'conflict',enabled:true,conflict:true}]}), /无法确认/);
});

test('npm SemVer ordering includes prereleases, ignores tags, and never downgrades', () => {
  const npmCli = process.env.DSH_TEST_NPM_CLI;
  assert(npmCli, 'DSH_TEST_NPM_CLI is required');
  const semver = createRequire(npmCli)('semver');
  assert.equal(update.highest(['1.0.0','2.0.0-beta.2','2.0.0-beta.10','latest'],'1.0.0',semver),'2.0.0-beta.10');
  assert.equal(update.highest(['2.0.0-beta.10','2.0.0'],'1.0.0',semver),'2.0.0');
  assert.equal(update.highest(['1.0.0'],'2.0.0',semver),'2.0.0');
  assert.throws(()=>update.highest([], '1.0.0',semver), /有效版本/);
});

test('configuration fingerprint detects changes and link cleanup never follows the target', () => {
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'plugin-update-'));
  try {
    const file=path.join(root,'config');
    const plan={guard:{[file]:null}}; update.check(plan);
    fs.writeFileSync(file,'changed'); assert.throws(()=>update.check(plan), /已变化/);
    const external=path.join(root,'external'); fs.mkdirSync(external); fs.writeFileSync(path.join(external,'keep'),'data');
    const candidate=path.join(root,'candidate');fs.mkdirSync(candidate);
    fs.symlinkSync(external,path.join(candidate,'link'),'junction');
    update.removeTree(candidate);
    assert.equal(fs.readFileSync(path.join(external,'keep'),'utf8'),'data');
  } finally {update.removeTree(root);}
});

test('selected local source remains unchanged without npm lookup', () => {
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'plugin-local-'));
  try {
    const profile=path.join(root,'web'); const pkg=path.join(profile,'node_modules','local');fs.mkdirSync(pkg,{recursive:true});
    fs.writeFileSync(path.join(pkg,'package.json'),JSON.stringify({name:'local',version:'1.0.0'}));
    fs.writeFileSync(path.join(profile,'package.json'),JSON.stringify({dependencies:{local:'link:../source'}}));
    const plan=update.prepare({catalog:{complete:true,profileDir:profile,plugins:[{name:'local',aliases:['local'],enabled:true,supported:true}]},npmCli:process.env.DSH_TEST_NPM_CLI,settingsFile:path.join(root,'settings')});
    assert.equal(plan.updates.length,0); assert.equal(plan.packages[0].reason,'需在原来源更新'); assert(!fs.existsSync(plan.root));
  } finally {update.removeTree(root);}
});
