'use strict';
// Repair stages only declared dependencies. No registry substitution or package scripts.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const {execFileSync} = require('node:child_process');
const hash = file => fs.existsSync(file) ? crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex') : null;
const exists = file => fs.existsSync(file);
function plain(directory) {
  const info = fs.lstatSync(directory);
  if (info.isSymbolicLink()) throw new Error(`修复目录不能是链接：${directory}`);
}
function write(file, value) {
  fs.writeFileSync(file + '.tmp', JSON.stringify(value, null, 2));
  fs.renameSync(file + '.tmp', file);
}
function locations(profile) {
  profile = fs.realpathSync(profile);
  const root = path.join(path.dirname(profile), '.dsh-launcher-repair-' + path.basename(profile));
  if (exists(root)) plain(root);
  return {profile, root, candidate: path.join(root, 'candidate'), backup: path.join(root, 'backup'), journal: path.join(root, 'transaction.json')};
}
const files = ['node_modules', 'package-lock.json'];
function rollback(p, record) {
  for (const name of [...files].reverse()) {
    const target = path.join(p.profile, name), backup = path.join(p.backup, name);
    // An absent candidate means it was renamed into the target before interruption.
    if (exists(backup)) {
      if (exists(target)) { if (fs.statSync(target).isDirectory()) checkTree(target); else plain(target); fs.rmSync(target, {recursive: true, force: true}); }
      fs.renameSync(backup, target);
    } else if (!record.original[name] && !exists(path.join(p.candidate, name)) && exists(target)) {
      if (fs.statSync(target).isDirectory()) checkTree(target); else plain(target);
      fs.rmSync(target, {recursive: true, force: true});
    }
  }
  record.phase = 'restored'; write(p.journal, record);
}
function recover(profile) {
  const p = locations(profile);
  if (!exists(p.journal)) {
    if (!exists(p.root)) return;
    if (exists(p.backup) && fs.readdirSync(p.backup).length) throw new Error('插件修复缺少事务记录，保留备份，请检查日志');
    checkTree(p.root);
    fs.rmSync(p.root, {recursive:true});
    return;
  }
  const record = JSON.parse(fs.readFileSync(p.journal));
  if (record.profile !== p.profile || !['prepared','committing','done','restored'].includes(record.phase)) throw new Error('插件修复事务无效，保留备份');
  if (record.kind === 'links') {
    if (record.phase === 'committing') restoreLinks(p, record);
    removeLinkStage(p.root);
    return;
  }
  if (record.phase === 'committing') rollback(p, record);
  // Validate the whole disposable tree before deletion, never follow a junction.
  checkTree(p.root);
  fs.rmSync(p.root, {recursive: true});
}
function packageName(name) {
  if (!/^(?:@[a-z0-9_.-]+\/)?[a-z0-9_.-]+$/i.test(name) || name.includes('..')) throw new Error('插件包名无效');
  return name;
}
function removeLinkStage(root) {
  plain(root);
  for (const item of fs.readdirSync(root,{withFileTypes:true})) {
    const file=path.join(root,item.name);
    if(item.isSymbolicLink()) fs.unlinkSync(file);
    else if(item.isDirectory()) removeLinkStage(file);
    else fs.unlinkSync(file);
  }
  fs.rmdirSync(root);
}
function restoreLinks(p,record) {
  for(const item of [...record.links].reverse()) {
    const target=path.join(p.profile,'node_modules',packageName(item.name));
    const backup=path.join(p.backup,item.name);
    if(fs.existsSync(target) && !fs.existsSync(path.join(p.candidate,'node_modules',item.name))) {
      if(!fs.lstatSync(target).isSymbolicLink() || fs.realpathSync(target)!==item.target) throw new Error('已提交插件链接发生变化，保留备份');
      fs.unlinkSync(target);
    }
    try { fs.lstatSync(backup); fs.mkdirSync(path.dirname(target),{recursive:true}); fs.renameSync(backup,target); } catch(e) { if(e.code!=='ENOENT') throw e; }
  }
  record.phase='restored';write(p.journal,record);
}
function prepareLinks(p,manifest) {
  const links=[];
  for(const [name,spec] of Object.entries(manifest.dependencies || {})) {
    if(!spec.startsWith('link:')) continue;
    packageName(name);
    const target=path.resolve(p.profile,spec.slice(5));
    if(!exists(path.join(target,'package.json'))) throw new Error(`${name} 原本地目录不存在：${target}；请恢复原路径`);
    const pkg=JSON.parse(fs.readFileSync(path.join(target,'package.json')));
    if(pkg.name!==name) throw new Error(`${name} 本地目录包名不匹配`);
    const current=path.join(p.profile,'node_modules',name);
    if(exists(current)) continue;
    links.push({name,spec,version:pkg.version,source:target,target:fs.realpathSync(target)});
  }
  if(!links.length) return null;
  fs.mkdirSync(p.candidate,{recursive:true});fs.mkdirSync(p.backup);
  for(const name of ['package.json','package-lock.json','cordis.patch.yml']) {
    const from=path.join(p.profile,name);if(exists(from)){plain(from);fs.copyFileSync(from,path.join(p.candidate,name));}
  }
  const record={kind:'links',profile:p.profile,phase:'prepared',manifest:hash(path.join(p.profile,'package.json')),lock:hash(path.join(p.profile,'package-lock.json')),links};
  write(p.journal,record);
  return {kind:'links',profile:p.profile,candidate:p.candidate,packages:links};
}
function installLinks(p,record) {
  const modules=path.join(p.profile,'node_modules');
  const connect=(name,target)=>{const dest=path.join(p.candidate,'node_modules',name);fs.mkdirSync(path.dirname(dest),{recursive:true});fs.symlinkSync(target,dest,'junction');};
  if(exists(modules)) {
    plain(modules);
    for(const item of fs.readdirSync(modules,{withFileTypes:true})) {
      if(item.name.startsWith('.')) continue;
      if(item.name.startsWith('@')) {
        const scope=path.join(modules,item.name);plain(scope);
        for(const name of fs.readdirSync(scope)) {
          const full=path.join(scope,name);if(exists(full)) connect(item.name+'/'+name,fs.realpathSync(full));
        }
      } else {const full=path.join(modules,item.name);if(exists(full)) connect(item.name,fs.realpathSync(full));}
    }
  }
  for(const item of record.links) connect(item.name,item.target);
}
function commitLinks(p,record) {
  record.phase='committing';write(p.journal,record);
  try {
    for(const item of record.links) {
      const target=path.join(p.profile,'node_modules',packageName(item.name));
      fs.mkdirSync(path.dirname(target),{recursive:true});plain(path.dirname(target));
      try {fs.lstatSync(target); const backup=path.join(p.backup,item.name);fs.mkdirSync(path.dirname(backup),{recursive:true});fs.renameSync(target,backup);} catch(e){if(e.code!=='ENOENT')throw e;}
      fs.renameSync(path.join(p.candidate,'node_modules',item.name),target);
    }
    record.phase='done';write(p.journal,record);
  } catch(error){restoreLinks(p,record);throw error;}
}
function checkTree(root) {
  plain(root);
  for (const entry of fs.readdirSync(root, {withFileTypes:true})) {
    const file = path.join(root, entry.name);
    if (entry.isSymbolicLink()) throw new Error(`修复暂存包含链接：${file}`);
    if (entry.isDirectory()) checkTree(file);
  }
}
function npm(npmCli, candidate, args, temp) {
  execFileSync(process.execPath, [npmCli, ...args, '--ignore-scripts', '--no-audit', '--no-fund'], {
    cwd: candidate, windowsHide: true, timeout: 15 * 60 * 1000,
    env: {...process.env, TEMP:temp, TMP:temp, npm_config_cache:process.env.npm_config_cache || path.join(temp,'npm-cache'), npm_config_update_notifier:'false'},
    stdio: ['ignore','pipe','pipe'], maxBuffer: 4 * 1024 * 1024,
  });
}
function prepare(profile, npmCli, temp) {
  recover(profile);
  const p = locations(profile);
  const manifest = JSON.parse(fs.readFileSync(path.join(p.profile, 'package.json')));
  const deps = manifest.dependencies || {};
  if (!Object.keys(deps).length) throw new Error('profile 没有声明依赖来源，请先修复 package.json，不能自动猜测安装来源');
  const local=prepareLinks(p,manifest);
  if(local) return local;
  for (const [name, spec] of Object.entries(deps)) {
    if (!spec || /^(file:|link:|workspace:|\.\.?[\\/])/.test(spec)) throw new Error(`${name} 使用本地依赖 ${spec}；请恢复原路径后重试`);
  }
  for (const name of manifest.dsh?.profile?.bundles || []) {
    if (!deps[name] && !name.startsWith('@deepseek-ai/')) throw new Error(`${name} 未声明依赖来源，不能自动安装`);
  }
  fs.mkdirSync(p.candidate, {recursive:true}); fs.mkdirSync(p.backup);
  for (const entry of fs.readdirSync(p.profile)) {
    if (entry === 'node_modules') continue;
    const from=path.join(p.profile,entry); plain(from);
    if (fs.statSync(from).isDirectory()) checkTree(from);
    fs.cpSync(from,path.join(p.candidate,entry),{recursive:true});
  }
  const record = {profile:p.profile, phase:'prepared', manifest:hash(path.join(p.profile,'package.json')),
    lock:hash(path.join(p.profile,'package-lock.json')), original:Object.fromEntries(files.map(name=>[name,exists(path.join(p.profile,name))]))};
  write(p.journal,record);
  const lockFile=path.join(p.candidate,'package-lock.json');
  if (!exists(lockFile)) npm(npmCli,p.candidate,['install','--package-lock-only'],temp);
  const lock=JSON.parse(fs.readFileSync(lockFile));
  const packages=Object.entries(deps).map(([name,spec])=>{
    const item=lock.packages?.['node_modules/'+name];
    if (!item?.version || !item.resolved || !item.integrity) throw new Error(`${name} 缺少完整锁定信息，请先检查依赖锁文件`);
    const url=new URL(item.resolved); url.username='';url.password='';url.search='';url.hash='';
    return {name,spec,version:item.version,source:url.toString()};
  });
  return {candidate:p.candidate,profile:p.profile,packages};
}
function install(profile,npmCli,temp) {
  const p=locations(profile);
  const record=JSON.parse(fs.readFileSync(p.journal));
  if(record.kind==='links') {installLinks(p,record);return;}
  npm(npmCli,p.candidate,['ci'],temp);
  checkTree(p.candidate);
}
function commit(profile) {
  const p=locations(profile), record=JSON.parse(fs.readFileSync(p.journal));
  if(record.phase !== 'prepared' || record.manifest !== hash(path.join(p.profile,'package.json')) || record.lock !== hash(path.join(p.profile,'package-lock.json'))) throw new Error('原 profile 已变化，请重新生成修复方案');
  if(record.kind==='links') {commitLinks(p,record);return;}
  for(const name of files) { if(exists(path.join(p.profile,name))) plain(path.join(p.profile,name)); }
  record.phase='committing';write(p.journal,record);
  try {
    for(const name of files) {
      const target=path.join(p.profile,name);
      if(exists(target)) fs.renameSync(target,path.join(p.backup,name));
      fs.renameSync(path.join(p.candidate,name),target);
    }
    record.phase='done';write(p.journal,record);
  } catch(error) { rollback(p,record);throw error; }
  // Keep backup until next recovery; committed modules are not reverted by cleanup failures.
}
module.exports={prepare,install,commit,recover,locations};
if(require.main === module) {
  try {
    const [mode,profile,npmCli,temp]=process.argv.slice(2);
    if(!['prepare','install','commit','recover'].includes(mode)) throw new Error('invalid repair mode');
    const result=module.exports[mode](profile,npmCli,temp);
    process.stdout.write(JSON.stringify(result || {}));
  } catch(error) { process.stderr.write(error.stderr?.toString() || error.message);process.exitCode=1; }
}
