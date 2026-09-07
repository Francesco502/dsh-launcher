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
function planPlugins(records, entries, overrides, compose) {
  const groups = new Map();
  for (const record of records) {
    const identity = record.identity || record.name;
    const existing = groups.get(identity);
    if (existing) {
      existing.aliases.push(record.name);
      existing.loaded ||= record.loaded;
    } else groups.set(identity, { ...record, aliases: [record.name] });
  }
  records = [...groups.values()];
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
    const selections = record.aliases.filter(name => Object.hasOwn(overrides, name)).map(name => overrides[name]);
    const specified = selections.length > 0;
    if (selections.some(value => typeof value !== 'boolean')) throw new Error('插件设置中的开关值无效');
    const conflict = selections.includes(true) && selections.includes(false);
    if (specified && !supported) error = `${record.name} 的加载结构已变化；请打开“选择插件”重新保存设置后重试`;
    const enabled = specified ? selections.every(Boolean) : record.loaded && record.roots.some(root => {
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
    const collisions = [...new Set(record.roots.flatMap(root => [...(owners.get(root.id) || [])]).filter(name => name !== record.name))];
    return { name: record.name, aliases: record.aliases, version: record.version, enabled, supported, conflict,
      reason: conflict ? '别名选择冲突，暂按停用；保存可统一' : !supported ? (collisions.length ? `加载 ID 冲突：${collisions.join('、')}` : '没有独立加载入口') : '' };
  });
  if (compose) {
    const effective = new Map(flatten(compose([[{ insert: entries }], patches])).map(entry => [entry.id, entry]));
    for (let i = 0; i < records.length; i++) {
      const active = records[i].roots.some(root => effective.has(root.id) && effective.get(root.id).disabled !== true);
      if (plugins[i].supported && plugins[i].enabled !== active) {
        plugins[i].supported = false;
        plugins[i].reason = '受上层配置限制，无法独立切换';
        if (records[i].aliases.some(name => Object.hasOwn(overrides, name))) error = `${records[i].name} 的选择无法生效：受上层配置限制`;
      }
      plugins[i].enabled = active;
    }
  }
  return { plugins, patches, error };
}

async function inspect(entry, settingsFile, portableHome, preflight = false) {
  const resolve = createRequire(entry);
  const core = await import(pathToFileURL(resolve.resolve('@deepseek-ai/dsh-app-boot')).href);
  for (const name of ['resolveProfileDir', 'resolveBundleDir', 'loadOverlayPatches', 'composeEntries', 'boot']) {
    if (typeof core[name] !== 'function') throw new Error(`DSH boot 接口缺失：${name}`);
  }
  if (preflight) {
    const metadata = JSON.parse(fs.readFileSync(path.resolve(path.dirname(entry), '..', 'package.json'), 'utf8'));
    for (const [name, file] of [[metadata.name, path.resolve(path.dirname(entry), '..', 'package.json')],
      ...Object.keys(metadata.dependencies || {}).map(name => [name, packageFile(resolve, name)])]) {
      const pkg = JSON.parse(fs.readFileSync(file, 'utf8'));
      if (pkg.engines?.node && !satisfiesNode(process.versions.node, pkg.engines.node)) throw new Error(`Node ${process.versions.node} 不满足 ${name} 要求 ${pkg.engines.node}`);
    }
  }
  const profileDir = portableHome ? path.join(portableHome, 'profiles', 'web') : core.resolveProfileDir('web');
  const key = portableHome ? 'portable:web' : `user:${path.resolve(profileDir).toLowerCase()}`;
  let settings = { profiles: {} };
  if (fs.existsSync(settingsFile)) settings = JSON.parse(fs.readFileSync(settingsFile, 'utf8'));
  if (!settings.profiles || typeof settings.profiles !== 'object' || Array.isArray(settings.profiles)) throw new Error('插件设置文件格式无效');
  const overrides = settings.profiles[key] || {};
  if (!overrides || typeof overrides !== 'object' || Array.isArray(overrides)) throw new Error('插件设置文件格式无效');
  const manifestFile = path.join(profileDir, 'package.json');
  const manifest = fs.existsSync(manifestFile) ? JSON.parse(fs.readFileSync(manifestFile, 'utf8'))
    : { dsh: { profile: { bundles: core.PROFILE_TEMPLATES?.web?.bundles || [] } } };
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
    if (Object.hasOwn(manifest.dependencies || {}, name)) records.push({ name, version: pkg.version, patches, loaded,
      directory: fs.realpathSync(dir), identity: `${pkg.name}@${pkg.version}:${JSON.stringify(patches)}` });
  }
  const userPatch = path.join(profileDir, 'cordis.patch.yml');
  if (fs.existsSync(userPatch)) layers.push(core.loadOverlayPatches('dsh', userPatch));
  const homePatch = path.join(portableHome || path.resolve(profileDir, '..', '..'), 'cordis.patch.yml');
  if (fs.existsSync(homePatch)) layers.push(core.loadOverlayPatches('dsh', homePatch));
  const result = planPlugins(records, core.composeEntries(layers), overrides, core.composeEntries);
  if (preflight) {
    if (result.error) throw new Error(result.error);
    const profileResolve = createRequire(manifestFile);
    for (const item of flatten(core.composeEntries([...layers, result.patches]))) {
      if (item.disabled || item.group || !item.name || item.name.startsWith('cordis:')) continue;
      try { profileResolve.resolve(item.name); }
      catch {
        try { resolve.resolve(item.name); }
        catch { throw new Error(`插件入口无法解析：${item.name}`); }
      }
    }
  }
  return { key, ...result };
}

function packageFile(resolve, name) {
  try { return resolve.resolve(`${name}/package.json`); } catch {}
  let dir = path.dirname(resolve.resolve(name));
  while (true) {
    const file = path.join(dir, 'package.json');
    if (fs.existsSync(file) && JSON.parse(fs.readFileSync(file, 'utf8')).name === name) return file;
    const parent = path.dirname(dir);
    if (parent === dir) throw new Error(`找不到运行依赖元数据：${name}`);
    dir = parent;
  }
}

// Accept common npm engine ranges; reject unsupported syntax rather than guessing.
function satisfiesNode(version, range) {
  const actual = version.split('.').map(Number);
  const cmp = expected => actual.reduce((value, part, i) => value || Math.sign(part - (expected[i] || 0)), 0);
  return range.split('||').some(branch => {
    branch = branch.trim().replace(/(\d+\.\d+\.\d+)\s+-\s+(\d+\.\d+\.\d+)/g, '>=$1 <=$2');
    return branch.split(/\s+/).filter(Boolean).every(term => {
      if (term === '*' || term === 'x') return true;
      const match = /^(>=|<=|>|<|=|\^|~)?v?(\d+)(?:\.(\d+|x|\*))?(?:\.(\d+|x|\*))?$/.exec(term);
      if (!match) throw new Error(`无法验证 Node 版本范围：${range}`);
      const [, op, major, minor, patch] = match;
      const parts = [major, minor, patch].map(value => /^\d+$/.test(value || '') ? Number(value) : 0);
      const order = cmp(parts);
      if (op === '>=') return order >= 0;
      if (op === '<=') return order <= 0;
      if (op === '>') return order > 0;
      if (op === '<') return order < 0;
      if (op === '^') return order >= 0 && (parts[0] ? actual[0] === parts[0] : parts[1] ? actual[0] === 0 && actual[1] === parts[1] : order === 0);
      if (op === '~') return order >= 0 && actual[0] === parts[0] && (minor === undefined || actual[1] === parts[1]);
      return actual[0] === parts[0] && (!/^\d+$/.test(minor || '') || actual[1] === parts[1]) && (!/^\d+$/.test(patch || '') || actual[2] === parts[2]);
    });
  });
}

module.exports = { planPlugins, inspect, satisfiesNode };
if (require.main === module) {
  inspect(process.argv[2], process.argv[3], process.argv[4] || undefined, process.argv[5] === 'preflight').then(result => {
    process.stdout.write(JSON.stringify(result));
  }).catch(error => {
    process.stderr.write(error.message);
    process.exitCode = 1;
  });
}
