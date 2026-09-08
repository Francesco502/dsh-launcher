//! User-triggered native checks and candidate-only builds; no idle subprocesses.
use super::*;

pub(super) fn needs_repair(paths: &Paths, installation: &Installation) -> Result<bool, String> {
    match plugins::native_dependencies(paths, installation) {
        Ok(modules) => Ok(modules.iter().any(|module| module["loaded"] != true)),
        Err(error) if CANCEL.load(Ordering::Acquire) => Err(error),
        // A missing backend/package also needs a fresh staged installation.
        Err(_) => Ok(true),
    }
}

pub(super) fn check(paths: &Paths, installation: &Installation) -> Result<(), String> {
    let modules = plugins::native_dependencies(paths, installation).map_err(|error| {
        format!("DSH 内置依赖检查失败；请点击“更新 DSH”修复安装依赖。\n{error}")
    })?;
    if let Some(module) = modules.iter().find(|module| module["loaded"] != true) {
        let reason = if module["kind"] == "node" {
            "Node ABI 不兼容"
        } else {
            "缺失或无法加载"
        };
        let name = module["name"].as_str().unwrap_or("未知模块");
        return Err(format!("DSH 内置原生依赖 {name} {reason}；关闭可选插件无效。请点击“更新 DSH”修复依赖；即使版本相同也会修复。"));
    }
    Ok(())
}

pub(super) fn prepare(
    paths: &Paths,
    npm: &Path,
    stage: &Path,
    installation: &Installation,
) -> Result<(), String> {
    let modules = plugins::native_dependencies(paths, installation)?;
    let failed: Vec<_> = modules
        .iter()
        .filter(|module| module["loaded"] != true)
        .collect();
    if failed.is_empty() {
        return Ok(());
    }
    let file = stage.join("package.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&file).map_err(|e| e.to_string())?)
            .map_err(|e| format!("候选安装清单无效：{e}"))?;
    let mut approvals = serde_json::Map::new();
    let mut names = Vec::new();
    for module in failed {
        let name = module["name"].as_str().ok_or("原生依赖名称缺失")?;
        let version = module["version"].as_str().ok_or("原生依赖版本缺失")?;
        if !matches!(name, "fs-ext" | "koffi") || parse_version(version).is_none() {
            return Err("候选请求了不支持的原生依赖构建".to_owned());
        }
        approvals.insert(format!("{name}@{version}"), serde_json::Value::Bool(true));
        names.push(name);
    }
    // npm 12 requires project-local approval even when ignore-scripts is false.
    manifest["allowScripts"] = serde_json::Value::Object(approvals);
    atomic_write(
        &file,
        &serde_json::to_vec(&manifest).map_err(|e| e.to_string())?,
    )?;
    let headers = paths.data.join("node-gyp");
    fs::create_dir_all(&headers).map_err(|e| e.to_string())?;
    let mut command = hidden_command(npm);
    command
        .arg("rebuild")
        .args(&names)
        .arg("--prefix")
        .arg(stage)
        .args([
            "--ignore-scripts=false",
            "--foreground-scripts",
            "--no-audit",
            "--no-fund",
        ])
        .env("npm_config_cache", &paths.cache)
        .env("npm_config_devdir", &headers)
        .env("npm_package_config_node_gyp_devdir", &headers)
        .env("TEMP", &paths.temp)
        .env("TMP", &paths.temp);
    let result = run_capture(paths, &mut command, "构建 DSH 原生依赖", NPM_TIMEOUT, true);
    cleanup_npm_cache(paths);
    if let Err(error) = result {
        if CANCEL.load(Ordering::Acquire) {
            return Err("操作已取消".to_owned());
        }
        return Err(format!("DSH 原生依赖构建失败（{}）；请检查现有 Python、Visual C++ 构建工具及 Node 头文件下载。未提交候选，原安装保持不变。\n{error}", names.join("、")));
    }
    check(paths, installation)
}
