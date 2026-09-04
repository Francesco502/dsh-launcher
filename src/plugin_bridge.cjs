'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { createRequire } = require('node:module');
const { pathToFileURL } = require('node:url');

function flatten(entries) {
  return entries.flatMap(entry => [entry, ...(entry.group && Array.isArray(entry.config)
    ? flatten(entry.config).map(child => ({ ...child, disabled: entry.disabled === true || child.disabled })) : [])]);
}

// Toggle the entries a bundle inserts, leaving shared services and user settings alone.
function planPlugins(records, entries, overrides) {
  const current = new Map(flatten(entries).map(entry => [entry.id, entry]));
  const owners = new Map();
  for (const record of records) {
    record.roots = record.patches.flatMap(patch => Array.isArray(patch.insert) ? patch.insert : []);
    for (const root of record.roots) {
      const names = owners.get(root.id) || new Set();
      names.add(record.name);
      owners.set(root.id, names);
    }
  }
  const patches = [];
  let error;
  const plugins = records.map(record => {
    const supported = record.roots.length > 0 && record.roots.every(root =>
      typeof root.id === 'string' && root.id.length > 0 && owners.get(root.id).size === 1);
    const specified = Object.hasOwn(overrides, record.name);
    if (specified && typeof overrides[record.name] !== 'boolean') throw new Error('插件设置中的开关值无效');
    if (specified && !supported) error = `${record.name} 的加载结构已变化；请打开“选择插件”重新保存设置后重试`;
    const enabled = specified ? overrides[record.name] : record.loaded && record.roots.some(root => {
      const entry = current.get(root.id);
      return entry && entry.disabled !== true;
    });
    if (specified && supported) {
      if (enabled && !record.loaded) patches.push(...record.patches);
      for (const root of record.roots) {
        if (record.loaded || enabled) {
          const patch = { id: root.id, disabled: !enabled };
          if (root.name) patch.name = root.name;
          patches.push(patch);
        }
      }
    }
    return { name: record.name, version: record.version, enabled, supported };
  });
  return { plugins, patches, error };
}

async function inspect(entry, settingsFile, portableHome) {
  const resolve = createRequire(entry);
  const core = await import(pathToFileURL(resolve.resolve('@deepseek-ai/dsh-app-boot')).href);
  const profileDir = portableHome ? path.join(portableHome, 'profiles', 'web') : core.resolveProfileDir('web');
  const key = portableHome ? 'portable:web' : `user:${path.resolve(profileDir).toLowerCase()}`;
  let settings = { profiles: {} };
  if (fs.existsSync(settingsFile)) settings = JSON.parse(fs.readFileSync(settingsFile, 'utf8'));
  if (!settings.profiles || typeof settings.profiles !== 'object' || Array.isArray(settings.profiles)) throw new Error('插件设置文件格式无效');
  const overrides = settings.profiles[key] || {};
  if (!overrides || typeof overrides !== 'object' || Array.isArray(overrides)) throw new Error('插件设置文件格式无效');
  const manifestFile = path.join(profileDir, 'package.json');
  if (!fs.existsSync(manifestFile)) return { key, plugins: [], patches: [] };
  const manifest = JSON.parse(fs.readFileSync(manifestFile, 'utf8'));
  const bundles = manifest.dsh?.profile?.bundles || [];
  const anchor = path.resolve(path.dirname(entry), '..', 'package.json');
  const records = [];
  const layers = [];
  for (const name of new Set([...bundles, ...Object.keys(manifest.dependencies || {})])) {
    const loaded = bundles.includes(name);
    let dir;
    try { dir = core.resolveBundleDir('dsh', name, anchor, profileDir); }
    catch (error) { if (loaded) throw error; else continue; }
    const pkg = JSON.parse(fs.readFileSync(path.join(dir, 'package.json'), 'utf8'));
    const patchFile = pkg.dsh?.bundle?.patch;
    if (typeof patchFile !== 'string') continue;
    const patches = core.loadOverlayPatches('dsh', path.join(dir, patchFile));
    if (loaded) layers.push(patches);
    if (Object.hasOwn(manifest.dependencies || {}, name)) records.push({ name, version: pkg.version, patches, loaded });
  }
  const userPatch = path.join(profileDir, 'cordis.patch.yml');
  if (fs.existsSync(userPatch)) layers.push(core.loadOverlayPatches('dsh', userPatch));
  return { key, ...planPlugins(records, core.composeEntries(layers), overrides) };
}

module.exports = { planPlugins, inspect };
if (require.main === module) {
  inspect(process.argv[2], process.argv[3], process.argv[4]).then(result => {
    process.stdout.write(JSON.stringify(result));
  }).catch(error => {
    process.stderr.write(error.message);
    process.exitCode = 1;
  });
}
