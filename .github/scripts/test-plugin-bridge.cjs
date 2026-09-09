const { test } = require('node:test');
const assert = require('node:assert/strict');
const { planPlugins } = require('../../src/plugin_bridge.cjs');
const { satisfiesNode } = require('../../src/plugin_bridge.cjs');
const { nativeDependencies, assertNativeDependencies } = require('../../src/plugin_bridge.cjs');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

test('catalog keeps missing configured bundle visible while startup inspection stays strict', async t => {
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'dsh-catalog-missing-'));
  t.after(()=>fs.rmSync(root,{recursive:true,force:true}));
  const entry=path.join(root,'node_modules/@deepseek-ai/dsh/lib/bin.js');
  const boot=path.join(root,'node_modules/@deepseek-ai/dsh-app-boot');
  fs.mkdirSync(path.dirname(entry),{recursive:true});fs.writeFileSync(entry,'');fs.mkdirSync(boot,{recursive:true});
  fs.writeFileSync(path.join(boot,'package.json'),JSON.stringify({name:'@deepseek-ai/dsh-app-boot',main:'index.js'}));
  fs.writeFileSync(path.join(boot,'index.js'),`exports.resolveProfileDir=()=>'';exports.resolveBundleDir=()=>{throw new Error('cannot resolve profile bundle qa-missing')};exports.loadOverlayPatches=()=>[];exports.composeEntries=()=>[];exports.boot=()=>{};`);
  const home=path.join(root,'home'),profile=path.join(home,'profiles/web');fs.mkdirSync(profile,{recursive:true});
  fs.writeFileSync(path.join(profile,'package.json'),JSON.stringify({dependencies:{'qa-missing':'1.0.0'},dsh:{profile:{bundles:['qa-missing']}}}));
  const {inspect}=require('../../src/plugin_bridge.cjs');
  const result=await inspect(entry,path.join(root,'settings.json'),home,false,true);
  assert.equal(result.complete,false);assert.equal(result.plugins[0].name,'qa-missing');assert.equal(result.plugins[0].supported,false);
  assert.equal(result.issues[0].specification,'1.0.0');
  await assert.rejects(()=>inspect(entry,path.join(root,'settings.json'),home),/cannot resolve profile bundle/);
  const repairRoot=path.join(home,'profiles/.dsh-launcher-repair-web');
  fs.mkdirSync(repairRoot);
  fs.writeFileSync(path.join(repairRoot,'transaction.json'),JSON.stringify({phase:'committing'}));
  const interrupted=await inspect(entry,path.join(root,'settings.json'),home,false,true);
  assert.equal(interrupted.complete,false);
  assert.match(interrupted.error,/修复尚未完成/);
  await assert.rejects(()=>inspect(entry,path.join(root,'settings.json'),home),/修复尚未完成/);
  assert.equal((await inspect(entry,path.join(root,'settings.json'),home,false,false,undefined,true)).profileDir,profile);
});

test('native checks load the addon, reject missing binaries and ABI drift, without initializing plugins', t => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dsh-native-check-'));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  for (const [kind, source] of Object.entries({
    native: "throw new Error(\"Cannot find module './build/Release/fs_ext.node'\");",
    node: "throw new Error('NODE_MODULE_VERSION 127; current version 137');",
    ready: "exports.flock = () => {};",
  })) {
    const modules = path.join(root, kind, 'node_modules');
    const write = (name, metadata, code) => {
      const directory = path.join(modules, name);
      fs.mkdirSync(directory, { recursive: true });
      fs.writeFileSync(path.join(directory, 'package.json'), JSON.stringify({name, version: '2.1.1', main: 'index.js', ...metadata}));
      fs.writeFileSync(path.join(directory, 'index.js'), code);
      return directory;
    };
    // DSH ships this transitively through its base bundle; its own manifest
    // lists it in devDependencies, not necessarily dependencies.
    const dsh = write('@deepseek-ai/dsh', {devDependencies: {'@deepseek-ai/dsh-session-persistence-jsonl': '*'}}, '');
    fs.mkdirSync(path.join(dsh, 'lib'));
    write('@deepseek-ai/dsh-session-persistence-jsonl', {dependencies: {'fs-ext': '*'}}, "throw new Error('Plugin must not initialize during native probe');");
    write('fs-ext', {}, source);
    const result = nativeDependencies(path.join(dsh, 'lib/bin.js'));
    assert.equal(result.length, 1);
    assert.equal(result[0].loaded, kind === 'ready');
    if (kind === 'ready') assert.doesNotThrow(() => assertNativeDependencies(result));
    else {
      assert.equal(result[0].kind, kind);
      assert.throws(() => assertNativeDependencies(result), /内置原生依赖无法加载：fs-ext/);
    }
  }
});

test('aliases of one actual bundle share a toggle; conflicting saved aliases fail closed', () => {
  const records = ['quota', '@scope/quota'].map(name => ({ ...bundle(name, [{ id: 'quota' }]), identity: 'same-package' }));
  const result = planPlugins(records, [{ id: 'quota' }], { quota: true, '@scope/quota': false });
  assert.equal(result.plugins.length, 1);
  assert.equal(result.plugins[0].supported, true);
  assert.equal(result.plugins[0].enabled, false);
  assert.equal(result.plugins[0].conflict, true);
  assert.deepEqual(result.plugins[0].aliases, ['quota', '@scope/quota']);
  assert.deepEqual(result.patches, [{ id: 'quota', disabled: true }]);
});

test('Node engine ranges reject incompatible versions and unsupported syntax', () => {
  assert(satisfiesNode('24.19.0', '>=22.12.0'));
  assert(!satisfiesNode('20.1.0', '>=22.12.0'));
  assert(satisfiesNode('24.19.0', '^22.0.0 || ^24.0.0'));
  assert(!satisfiesNode('24.19.0', '~24.18.0'));
  assert(satisfiesNode('24.19.0', '24.x'));
  assert(!satisfiesNode('24.19.0', '<24'));
  assert.throws(() => satisfiesNode('24.19.0', 'banana'));
});

const bundle = (name, roots, loaded = true) => ({
  name, version: '1.0.0', loaded, patches: [{ insert: roots }],
});

test('existing profile choices remain unchanged until the user selects a plugin', () => {
  const records = [bundle('bad-plugin', [{ id: 'bad', name: 'bad-plugin' }]),
    bundle('good-plugin', [{ id: 'good', name: 'good-plugin' }])];
  const result = planPlugins(records, [{ id: 'bad', disabled: true }, { id: 'good' }], {});
  assert.deepEqual(result.plugins.map(p => p.enabled), [false, true]);
  assert.deepEqual(result.patches, []);
});

test('disabling a failing bundle suppresses every inserted root without disabling core services', () => {
  const records = [bundle('broken', [{ id: 'worker', name: 'broken' }, { id: 'web', name: 'broken/client' }])];
  const result = planPlugins(records, [{ id: 'core' }, { id: 'worker' }, { id: 'web' }], { broken: false });
  assert.deepEqual(result.patches, [
    { id: 'worker', name: 'broken', disabled: true },
    { id: 'web', name: 'broken/client', disabled: true },
  ]);
});

test('explicit enable adds an installed but unlisted bundle, then overrides its disabled state', () => {
  const records = [bundle('optional', [{ id: 'optional', name: 'optional', disabled: true }], false)];
  const before = JSON.stringify(records);
  const result = planPlugins(structuredClone(records), [], { optional: true });
  assert.equal(result.patches[0].insert[0].disabled, true);
  assert.deepEqual(result.patches[1], { id: 'optional', name: 'optional', disabled: false });
  assert.equal(JSON.stringify(records), before);
  assert.deepEqual(planPlugins(records, [], { optional: false }).patches, []);
});

test('group disable is inherited, while a selected group preserves child settings', () => {
  const records = [bundle('group-plugin', [{ id: 'child', name: 'child' }])];
  const entries = [{ id: 'parent', group: true, disabled: true, config: [{ id: 'child' }] }];
  assert.equal(planPlugins(records, entries, {}).plugins[0].enabled, false);
});

test('shared entry IDs cannot silently disable another bundle', () => {
  const records = [bundle('a', [{ id: 'shared' }]), bundle('b', [{ id: 'shared' }])];
  const result = planPlugins(records, [{ id: 'shared' }], { a: false });
  assert.ok(result.error);
  assert.ok(result.plugins.every(p => !p.supported));
  assert.deepEqual(result.patches, []);
});

test('removed packages are ignored; invalid override values fail instead of enabling a plugin', () => {
  assert.deepEqual(planPlugins([], [], { removed: false }).patches, []);
  assert.throws(() => planPlugins([bundle('a', [{ id: 'a' }])], [], { a: 'false' }), /开关值无效/);
});

test('patch-only bundles remain visible without unsafe toggles', () => {
  const result = planPlugins([{ name: 'config', version: '1', loaded: true, patches: [{ id: 'core', config: {} }] }], [], {});
  assert.equal(result.plugins[0].supported, false);
  assert.deepEqual(result.patches, []);
});
