use super::*;
const BRIDGE: &[u8] = include_bytes!("plugin_bridge.cjs");
#[derive(Clone)]
pub(super) struct Plugin {
    pub(super) name: String,
    pub(super) version: String,
    pub(super) enabled: bool,
    pub(super) supported: bool,
    pub(super) aliases: Vec<String>,
    pub(super) conflict: bool,
    pub(super) reason: String,
}

#[derive(Clone)]
pub(super) struct Catalog {
    pub(super) key: String,
    pub(super) plugins: Vec<Plugin>,
    pub(super) complete: bool,
    pub(super) note: String,
}

fn settings_path(paths: &Paths) -> PathBuf {
    paths.state.join("plugin-settings.json")
}

pub(super) fn record_started(
    paths: &Paths,
    pid: u32,
    process: &ProcessHandle,
) -> Result<(), String> {
    let settings: serde_json::Value = fs::read(settings_path(paths))
        .ok()
        .and_then(|v| serde_json::from_slice(&v).ok())
        .unwrap_or(serde_json::Value::Null);
    let value = serde_json::json!({"pid":pid, "created":process.times()?.0, "image":lifecycle::image(process)?, "settings":settings});
    atomic_write(
        &paths.state.join("dsh-session.json"),
        &serde_json::to_vec(&value).unwrap(),
    )
}
pub(super) fn pending(paths: &Paths) -> bool {
    let record: Option<serde_json::Value> = fs::read(paths.state.join("dsh-session.json"))
        .ok()
        .and_then(|v| serde_json::from_slice(&v).ok());
    let current: serde_json::Value = fs::read(settings_path(paths))
        .ok()
        .and_then(|v| serde_json::from_slice(&v).ok())
        .unwrap_or(serde_json::Value::Null);
    record.is_some_and(|record| {
        record["pid"].as_u64() == read_tracked_pid(paths).map(u64::from)
            && record["settings"] != current
    })
}

fn inspect(paths: &Paths, installation: &Installation) -> Result<serde_json::Value, String> {
    inspect_settings(paths, installation, &settings_path(paths))
}

fn inspect_settings(
    paths: &Paths,
    installation: &Installation,
    settings: &Path,
) -> Result<serde_json::Value, String> {
    inspect_mode(paths, installation, settings, "")
}
pub(super) fn inspect_mode(
    paths: &Paths,
    installation: &Installation,
    settings: &Path,
    mode: &str,
) -> Result<serde_json::Value, String> {
    let bridge = paths.state.join("plugin-bridge.cjs");
    if fs::read(&bridge).ok().as_deref() != Some(BRIDGE) {
        atomic_write(&bridge, BRIDGE)?;
    }
    let mut command = hidden_command(&installation.node);
    command.arg(bridge).arg(&installation.entry).arg(settings);
    if installation.profile == ProfileMode::Portable {
        command.arg(&paths.profile);
        command.env("DSH_HOME", &paths.profile);
    } else {
        command.arg("");
    }
    command.arg(mode);
    command.env("TEMP", &paths.temp).env("TMP", &paths.temp);
    let label = match mode {
        "native" => "检查 DSH 原生依赖",
        "preflight" => "预检 DSH 配置和依赖",
        _ => "读取插件配置",
    };
    let output = run_capture(paths, &mut command, label, QUERY_TIMEOUT, true)?;
    serde_json::from_str(&output).map_err(|error| format!("插件列表格式无效：{error}"))
}

pub(super) fn preflight(paths: &Paths, installation: &Installation) -> Result<(), String> {
    inspect_mode(paths, installation, &settings_path(paths), "preflight").map(|_| ())
}

pub(super) fn native_dependencies(
    paths: &Paths,
    installation: &Installation,
) -> Result<Vec<serde_json::Value>, String> {
    let value = inspect_mode(paths, installation, &settings_path(paths), "native")?;
    value
        .as_array()
        .cloned()
        .ok_or_else(|| "原生依赖检查结果无效".to_owned())
}

pub(super) fn startup_patch(
    paths: &Paths,
    installation: &Installation,
) -> Result<Option<PathBuf>, String> {
    if !settings_path(paths).is_file() {
        return Ok(None);
    }
    let value = inspect(paths, installation)?;
    if let Some(error) = value["error"].as_str() {
        return Err(error.to_owned());
    }
    let patches = value["patches"].as_array().ok_or("插件启动配置格式无效")?;
    if patches.is_empty() {
        return Ok(None);
    }
    let file = paths.state.join("plugin-startup.patch.json");
    atomic_write(
        &file,
        serde_json::to_string_pretty(patches).unwrap().as_bytes(),
    )?;
    Ok(Some(file))
}

pub(super) fn catalog(paths: &Paths, installation: &Installation) -> Result<Catalog, String> {
    let value = inspect_mode(paths, installation, &settings_path(paths), "catalog")?;
    let key = value["key"]
        .as_str()
        .ok_or("插件配置缺少 profile 标识")?
        .to_owned();
    let mut plugins = Vec::new();
    for row in value["plugins"].as_array().ok_or("插件列表无效")? {
        plugins.push(Plugin {
            name: row["name"].as_str().ok_or("插件名称无效")?.to_owned(),
            version: row["version"].as_str().unwrap_or("未知版本").to_owned(),
            enabled: row["enabled"].as_bool().ok_or("插件开关无效")?,
            supported: row["supported"].as_bool().unwrap_or(false),
            aliases: row["aliases"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
            conflict: row["conflict"].as_bool().unwrap_or(false),
            reason: row["reason"].as_str().unwrap_or("").to_owned(),
        });
    }
    Ok(Catalog {
        key,
        plugins,
        complete: value["complete"].as_bool().unwrap_or(false) && value["error"].is_null(),
        note: if let Some(error) = value["error"].as_str() {
            error.into()
        } else if value["complete"] == false {
            "部分插件无法解析；请修复依赖后再保存。原设置未修改。".into()
        } else {
            "保存后下次启动生效；运行中的 DSH 需手动重启。".into()
        },
    })
}

pub(super) fn save_choices(
    paths: &Paths,
    catalog: &Catalog,
    enabled: &[bool],
) -> Result<(), String> {
    if !catalog.complete || enabled.len() != catalog.plugins.len() {
        return Err("插件列表不完整，请重新读取或修复后保存".into());
    }
    let mut choices = serde_json::Map::new();
    for (plugin, &value) in catalog.plugins.iter().zip(enabled) {
        if plugin.supported && (plugin.enabled != value || plugin.conflict) {
            for name in &plugin.aliases {
                choices.insert(name.clone(), value.into());
            }
        }
    }
    if choices.is_empty() {
        return Ok(());
    }
    let file = settings_path(paths);
    let candidate = paths.state.join("plugin-settings.pending.json");
    if file.exists() {
        fs::copy(&file, &candidate).map_err(|e| e.to_string())?;
    } else {
        atomic_write(&candidate, b"{\"profiles\":{}}")?;
    }
    let result = (|| {
        write_choices(&candidate, &catalog.key, choices)?;
        let installation = discover_installation(paths)?.ok_or("请先安装 DSH")?;
        let value = inspect_settings(paths, &installation, &candidate)?;
        if value["key"] != catalog.key {
            return Err("DSH profile 已变化，请重新读取插件列表".into());
        }
        if let Some(error) = value["error"].as_str() {
            return Err(error.to_owned());
        }
        atomic_write(&file, &fs::read(&candidate).map_err(|e| e.to_string())?)
    })();
    let _ = fs::remove_file(candidate);
    result
}

pub(super) fn repair(
    paths: &Paths,
    confirm: &dyn Fn(&str) -> bool,
    progress: &dyn Fn(&str, bool),
) -> Result<String, String> {
    let _guard = acquire_action_mutex().ok_or("已有启动器操作正在执行")?;
    if tracked_dsh_pid(paths)?.is_some() {
        return Err("请先停止 DSH，再修复其正在使用的插件依赖。".into());
    }
    let installation = discover_installation(paths)?.ok_or("未找到 DSH")?;
    let info = inspect_mode(paths, &installation, &settings_path(paths), "profile")?;
    let profile = info["profileDir"]
        .as_str()
        .ok_or("无法确定原 profile 目录")?;
    let npm = find_command("npm.cmd").ok_or("未找到 npm")?;
    let npm_cli = npm
        .parent()
        .ok_or("npm 路径无效")?
        .join("node_modules/npm/bin/npm-cli.js");
    if !npm_cli.is_file() {
        return Err("无法定位 npm-cli.js；请修复本机 npm 安装。".into());
    }
    let script = paths.state.join("plugin-repair.cjs");
    atomic_write(&script, include_bytes!("plugin_repair.cjs"))?;
    let invoke = |mode: &str| -> Result<serde_json::Value, String> {
        let mut command = hidden_command(&installation.node);
        command
            .arg(&script)
            .arg(mode)
            .arg(profile)
            .arg(&npm_cli)
            .arg(&paths.temp);
        command.env("npm_config_cache", &paths.cache);
        command.env("TEMP", &paths.temp).env("TMP", &paths.temp);
        let text = run_capture(
            paths,
            &mut command,
            "修复插件依赖",
            NPM_TIMEOUT,
            mode != "commit" && mode != "recover",
        )?;
        serde_json::from_str(&text).map_err(|e| e.to_string())
    };
    let result = (|| {
        progress("正在按原 profile 解析依赖锁定版本…", true);
        let plan = invoke("prepare")?;
        let items = plan["packages"].as_array().ok_or("插件修复方案无效")?;
        let details = items
            .iter()
            .map(|item| {
                format!(
                    "{} @ {}\n{}",
                    item["name"].as_str().unwrap_or(""),
                    item["version"].as_str().unwrap_or(""),
                    item["source"].as_str().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        if !confirm(&format!("将修复以下 profile 的依赖：\n{profile}\n\n{details}\n\n保持原配置和版本约束，不运行包安装脚本。继续？")) { return Err("操作已取消".into()); }
        progress("正在暂存插件依赖…", true);
        invoke("install")?;
        let candidate = plan["candidate"].as_str().ok_or("修复暂存目录无效")?;
        let mut command = hidden_command(&installation.node);
        command
            .arg(paths.state.join("plugin-bridge.cjs"))
            .arg(&installation.entry)
            .arg(settings_path(paths))
            .arg(if installation.profile == ProfileMode::Portable {
                paths.profile.as_os_str()
            } else {
                OsStr::new("")
            })
            .arg("preflight")
            .arg(candidate);
        command.env("TEMP", &paths.temp).env("TMP", &paths.temp);
        run_capture(paths, &mut command, "验证候选插件依赖", QUERY_TIMEOUT, true)?;
        if tracked_dsh_pid(paths)?.is_some() {
            return Err("DSH 已启动，未提交插件修复；请停止后重试。".into());
        }
        progress("正在提交已验证的插件依赖…", false);
        invoke("commit")?;
        Ok("插件依赖修复完成，原配置与开关已保留。".into())
    })();
    if let Err(cleanup) = invoke("recover") {
        append_log(&paths.logs.join("launcher.log"), &cleanup);
    }
    cleanup_npm_cache(paths);
    result
}

fn write_choices(
    file: &Path,
    key: &str,
    choices: serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    let mut settings: serde_json::Value = if file.exists() {
        serde_json::from_slice(&fs::read(file).map_err(|error| error.to_string())?)
            .map_err(|error| format!("插件设置文件损坏：{error}"))?
    } else {
        serde_json::json!({ "profiles": {} })
    };
    let profiles = settings
        .get_mut("profiles")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("插件设置格式无效")?;
    let profile = profiles
        .entry(key.to_owned())
        .or_insert_with(|| serde_json::json!({}));
    let profile = profile.as_object_mut().ok_or("插件 profile 设置格式无效")?;
    profile.extend(choices);
    atomic_write(
        file,
        serde_json::to_string_pretty(&settings).unwrap().as_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_choices_round_trip_and_preserve_other_profiles() {
        let root = env::temp_dir().join(format!("dsh-plugin-settings-{}", transaction_nonce()));
        fs::create_dir_all(&root).unwrap();
        let file = root.join("settings.json");
        let choices = |enabled| {
            serde_json::Map::from_iter([("plugin".to_owned(), serde_json::Value::Bool(enabled))])
        };
        write_choices(&file, "portable:web", choices(false)).unwrap();
        write_choices(&file, "user:other", choices(true)).unwrap();
        write_choices(
            &file,
            "portable:web",
            serde_json::Map::from_iter([("untouched".to_owned(), serde_json::Value::Bool(true))]),
        )
        .unwrap();
        write_choices(&file, "portable:web", choices(false)).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        assert_eq!(value["profiles"]["portable:web"]["plugin"], false);
        assert_eq!(value["profiles"]["user:other"]["plugin"], true);
        assert_eq!(value["profiles"]["portable:web"]["untouched"], true);
        fs::write(&file, b"invalid").unwrap();
        assert!(write_choices(&file, "portable:web", choices(true)).is_err());
        assert_eq!(fs::read(&file).unwrap(), b"invalid");
        fs::remove_dir_all(root).unwrap();
    }
}
