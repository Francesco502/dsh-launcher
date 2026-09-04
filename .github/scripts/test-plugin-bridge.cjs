const { test } = require('node:test');
const assert = require('node:assert/strict');
const { planPlugins } = require('../../src/plugin_bridge.cjs');

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
