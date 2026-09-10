'use strict';
// npm's own parsers are used from the npm installation already required by DSH.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const {createRequire} = require('node:module');
const {execFileSync} = require('node:child_process');
const read = file => JSON.parse(fs.readFileSync(file, 'utf8'));
const write = (file, value) => fs.writeFileSync(file, JSON.stringify(value, null, 2));
const hash = file => fs.existsSync(file) ? crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex') : null;
function plain(directory) {
  if (fs.lstatSync(directory).isSymbolicLink()) throw new Error(`更新路径不能是链接：${directory}`);
}
function removeTree(root) {
  const stat = fs.lstatSync(root, {throwIfNoEntry: false});
  if (!stat) return;
  if (stat.isSymbolicLink() || !stat.isDirectory()) { fs.unlinkSync(root); return; }
  for (const name of fs.readdirSync(root)) removeTree(path.join(root, name));
  fs.rmdirSync(root);
}
function npm(request, cwd, args) {
  return execFileSync(process.execPath, [request.npmCli, ...args, '--ignore-scripts', '--no-audit', '--no-fund'], {
    cwd, windowsHide: true, timeout: 15 * 60 * 1000, encoding: 'utf8', maxBuffer: 8 * 1024 * 1024,
    env: {...process.env, TEMP: request.temp, TMP: request.temp, npm_config_update_notifier: 'false'},
    stdio: ['ignore', 'pipe', 'pipe'],
  }).trim();
}
function parsers(request) {
  const resolve = createRequire(request.npmCli);
  return {semver: resolve('semver'), spec: resolve('npm-package-arg')};
}
function versionsFromRegistry(request, profile, name, registry) {
  // npm may serve stale metadata after a network failure. A fresh query cache
  // makes that failure visible while keeping the separate tarball cache useful.
  const cache = fs.mkdtempSync(path.join(request.temp, 'plugin-query-'));
  try { return JSON.parse(npm(request, profile, ['view', name, 'versions', '--json', '--registry', registry, '--cache', cache, '--prefer-online'])); }
  finally { removeTree(cache); }
}
function parseSpec(spec, name, declared, profile) {
  return /^(link:|workspace:)/.test(declared) ? {registry: false} : spec.resolve(name, declared, profile);
}
function selected(catalog) {
  if (!catalog.complete || catalog.error) throw new Error('插件列表不完整，请先修复依赖或配置后更新');
  return [...new Set(catalog.plugins.filter(p => p.enabled && !p.builtin).flatMap(p => {
    if (p.conflict || !p.supported) throw new Error(`${p.name} 的启用状态无法确认，请先处理插件配置`);
    return p.aliases || [p.name];
  }))];
}
function highest(versions, current, semver) {
  if (!semver.valid(current)) throw new Error(`当前插件版本无效：${current}`);
  if (!Array.isArray(versions)) versions = [versions];
  const valid = versions.filter(v => typeof v === 'string' && semver.valid(v));
  if (!valid.length) throw new Error('npm 未返回有效版本');
  return valid.reduce((best, value) => semver.gt(value, best) ? value : best, current);
}
function prepare(request) {
  const catalog = request.catalog;
  const names = selected(catalog);
  const profile = path.resolve(catalog.profileDir);
  const root = path.join(path.dirname(profile), '.dsh-launcher-update-' + path.basename(profile));
  if (fs.existsSync(root)) throw new Error('插件更新暂存尚未清理，请先恢复上次事务');
  const candidate = path.join(root, 'candidate');
  const packages = [], updates = [], preserved = [];
  for (const plugin of catalog.plugins.filter(p => p.enabled && p.builtin)) packages.push({name: plugin.name, current: plugin.version, target: plugin.version, source: 'DSH 内置', reason: '随 DSH 更新'});
  const guard = Object.fromEntries(['package.json', 'package-lock.json', 'cordis.yml', 'cordis.patch.yml', '.npmrc'].map(name => [path.join(profile, name), hash(path.join(profile, name))]));
  guard[request.settingsFile] = hash(request.settingsFile);
  const plan = {profile, root, candidate, packages, updates, preserved, guard};
  if (!names.length) return plan;
  plain(profile);
  plain(path.join(profile, 'package.json'));
  const manifest = read(path.join(profile, 'package.json'));
  const lockFile = path.join(profile, 'package-lock.json');
  if (fs.existsSync(lockFile)) plain(lockFile);
  const lock = fs.existsSync(lockFile) ? read(lockFile) : {packages: {}};
  const {semver, spec} = parsers(request);
  const versions = new Map();
  for (const name of names) {
    const declared = manifest.dependencies?.[name];
    if (!declared) throw new Error(`${name} 缺少原依赖来源，不能自动猜测`);
    const parsed = parseSpec(spec, name, declared, profile);
    const resolved = parsed.type === 'alias' ? parsed.subSpec : parsed;
    const installed = read(path.join(profile, 'node_modules', name, 'package.json'));
    if (resolved.name && resolved.name !== installed.name) throw new Error(`${name} 的依赖包归属不匹配`);
    const item = {name, package: installed.name, current: installed.version, target: installed.version, source: declared};
    if (!resolved.registry) {
      item.reason = '需在原来源更新'; packages.push(item); continue;
    }
    let registry = resolved.scope ? npm(request, profile, ['config', 'get', resolved.scope + ':registry']) : '';
    if (!registry || ['undefined', 'null'].includes(registry)) registry = npm(request, profile, ['config', 'get', 'registry']);
    const source = new URL(registry);
    if (!['https:', 'http:'].includes(source.protocol) || source.username || source.password || source.search || source.hash) throw new Error(`${name} 的 npm 来源无效`);
    const original = lock.packages?.['node_modules/' + name];
    // A different origin must be fixed explicitly; do not silently move a private dependency.
    if (original?.resolved && /^https?:/.test(original.resolved) && new URL(original.resolved).origin !== source.origin) {
      throw new Error(`${name} 的锁文件来源与当前 npm 源不同，请先核对原来源`);
    }
    source.search = ''; source.hash = ''; item.source = source.toString();
    const key = registry + ':' + installed.name;
    if (!versions.has(key)) versions.set(key, versionsFromRegistry(request, profile, installed.name, registry));
    item.target = highest(versions.get(key), installed.version, semver);
    item.spec = parsed.type === 'alias' ? `npm:${installed.name}@${item.target}` : item.target;
    packages.push(item);
    if (semver.gt(item.target, item.current)) updates.push(item);
  }
  if (!updates.length) return plan;
  if (!fs.existsSync(lockFile)) throw new Error('缺少插件锁文件，请先修复依赖后更新');
  // Pin every other direct npm dependency while the selected packages are resolved.
  for (const [name, declared] of Object.entries(manifest.dependencies || {})) {
    if (updates.some(item => item.name === name)) continue;
    const parsed = parseSpec(spec, name, declared, profile);
    const resolved = parsed.type === 'alias' ? parsed.subSpec : parsed;
    if (!resolved.registry) {
      const target = fs.realpathSync(path.join(profile, 'node_modules', name));
      preserved.push({name, target}); continue;
    }
    const original = lock.packages?.['node_modules/' + name];
    if (!original?.version || !original.resolved || !original.integrity) throw new Error(`${name} 缺少完整锁定信息，请先修复依赖`);
    if (read(path.join(profile,'node_modules',name,'package.json')).version !== original.version) throw new Error(`${name} 的安装版本与锁文件不一致，请先修复依赖`);
  }
  fs.mkdirSync(candidate, {recursive: true});
  try {
    for (const name of fs.readdirSync(profile)) {
      if (name === 'node_modules') continue;
      fs.cpSync(path.join(profile, name), path.join(candidate, name), {recursive: true, dereference: false});
    }
    // Relative home patches must keep their original meaning during preflight.
    const parentPatch = path.resolve(profile, '../..', 'cordis.patch.yml');
    guard[parentPatch] = hash(parentPatch);
    write(path.join(root, 'plan.json'), plan);
    return plan;
  } catch (error) { removeTree(root); throw error; }
}
function install(request, plan) {
  if (!plan.updates.length) return;
  check(plan);
  const {spec, semver} = parsers(request);
  const original = read(path.join(plan.profile, 'package.json'));
  const manifest = structuredClone(original);
  const lock = read(path.join(plan.profile, 'package-lock.json'));
  for (const [name, declared] of Object.entries(manifest.dependencies || {})) {
    const update = plan.updates.find(item => item.name === name);
    if (update) { manifest.dependencies[name] = update.spec; continue; }
    if (plan.preserved.some(item => item.name === name)) { delete manifest.dependencies[name]; continue; }
    const parsed = parseSpec(spec, name, declared, plan.profile);
    manifest.dependencies[name] = parsed.type === 'alias' ? `npm:${parsed.subSpec.name}@${lock.packages['node_modules/' + name].version}` : lock.packages['node_modules/' + name].version;
  }
  // Do not install unrelated developer/optional dependencies as part of plugin updates.
  for (const group of ['devDependencies', 'optionalDependencies', 'peerDependencies']) {
    if (Object.keys(manifest[group] || {}).length) throw new Error(`profile 包含 ${group}，请先使用原包管理流程整理依赖`);
  }
  write(path.join(plan.candidate, 'package.json'), manifest);
  npm(request, plan.candidate, ['install', '--package-lock-only']);
  npm(request, plan.candidate, ['ci']);
  const nextLock = read(path.join(plan.candidate, 'package-lock.json'));
  for (const [name, declared] of Object.entries(original.dependencies || {})) {
    const item = nextLock.packages?.['node_modules/' + name];
    const update = plan.updates.find(item => item.name === name);
    if (update) {
      if (item?.version !== update.target || !item.integrity || !item.resolved || new URL(item.resolved).origin !== new URL(update.source).origin) throw new Error(`${name} 候选与确认版本或来源不一致`);
      const installed = read(path.join(plan.candidate, 'node_modules', name, 'package.json'));
      if (installed.name !== update.package || installed.version !== update.target) throw new Error(`${name} 候选包身份不一致`);
      if (installed.engines?.node && !semver.satisfies(process.versions.node, installed.engines.node, {includePrerelease:true})) throw new Error(`${name} 要求 Node ${installed.engines.node}，当前为 ${process.versions.node}`);
      original.dependencies[name] = update.spec;
    } else if (!plan.preserved.some(item => item.name === name)) {
      const previous = lock.packages['node_modules/' + name];
      if (!item || item.version !== previous.version || item.integrity !== previous.integrity || item.resolved !== previous.resolved) throw new Error(`${name} 未选依赖发生变化，取消更新`);
    }
  }
  for (const item of plan.preserved) {
    const target = path.join(plan.candidate, 'node_modules', item.name);
    removeTree(target); fs.mkdirSync(path.dirname(target), {recursive: true});
    const relative = path.relative(path.join(plan.profile, 'node_modules'), item.target);
    if (relative && !relative.startsWith('..' + path.sep) && relative !== '..' && !path.isAbsolute(relative)) {
      // Copied file/Git packages must survive replacement of the old dependency tree.
      fs.cpSync(item.target, target, {recursive: true, dereference: false});
    } else {
      fs.symlinkSync(item.target, target, 'junction');
    }
    if (lock.packages['node_modules/' + item.name]) nextLock.packages['node_modules/' + item.name] = lock.packages['node_modules/' + item.name];
  }
  nextLock.packages[''].dependencies = original.dependencies;
  write(path.join(plan.candidate, 'package.json'), original);
  write(path.join(plan.candidate, 'package-lock.json'), nextLock);
  check(plan);
}
function check(plan) {
  for (const [file, digest] of Object.entries(plan.guard)) if (hash(file) !== digest) throw new Error('原 profile 或插件选择已变化，请重新检查更新');
}
module.exports = {prepare, install, selected, highest, removeTree, check};
if (require.main === module) {
  try {
    const [mode, requestFile, planFile] = process.argv.slice(2);
    const request = read(requestFile);
    const plan = planFile ? read(planFile) : undefined;
    const result = mode === 'prepare' ? prepare(request) : mode === 'install' ? install(request, plan) : mode === 'check' ? check(plan) : (() => { throw new Error('invalid update mode'); })();
    process.stdout.write(JSON.stringify(result || {}));
  } catch (error) { process.stderr.write(error.message); process.exitCode = 1; }
}
