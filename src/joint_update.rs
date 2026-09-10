//! One journal covers the DSH prefix and the selected profile dependency group.
use super::*;
use serde_json::{json, Value};

fn journal(paths: &Paths) -> PathBuf {
    paths.state.join("joint-update.json")
}
fn json_file(file: &Path) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(file).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
fn save(paths: &Paths, record: &Value) -> Result<(), String> {
    atomic_write(
        &journal(paths),
        &serde_json::to_vec(record).map_err(|e| e.to_string())?,
    )
}
fn field(value: &Value, name: &str) -> Result<PathBuf, String> {
    value[name]
        .as_str()
        .map(PathBuf::from)
        .ok_or_else(|| format!("更新记录缺少 {name}"))
}
fn remove(path: &Path) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt;
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
        Ok(info) if info.file_attributes() & 0x400 != 0 => if info.file_attributes() & 0x10 != 0 {
            fs::remove_dir(path)
        } else {
            fs::remove_file(path)
        }
        .map_err(|e| e.to_string()),
        Ok(info) if info.is_dir() => fs::remove_dir_all(path).map_err(|e| e.to_string()),
        Ok(_) => fs::remove_file(path).map_err(|e| e.to_string()),
    }
}
fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}
fn rename(from: &Path, to: &Path) -> Result<(), String> {
    fs::rename(from, to).map_err(|e| format!("无法交换 {} → {}：{e}", from.display(), to.display()))
}
fn swap(target: PathBuf, stage: PathBuf, backup: PathBuf) -> Value {
    json!({"original": exists(&target), "target": target, "stage": stage, "backup": backup})
}
fn validate(paths: &Paths, record: &Value) -> Result<(), String> {
    let root = field(record, "root")?;
    ensure_under(&paths.updates, &root)?;
    if root.parent() != Some(paths.updates.as_path())
        || !root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .starts_with("joint-")
    {
        return Err("联合更新目录无效".into());
    }
    let profile = record["profile"].as_str().map(PathBuf::from);
    if let Some(profile) = &profile {
        let portable = paths.profile.join("profiles/web");
        let user = env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .map(|p| p.join(".dsh/profiles/web"));
        if profile != &portable && user.as_ref() != Some(profile) {
            return Err("联合更新 profile 路径无效".into());
        }
    }
    let swaps = record["swaps"].as_array().ok_or("联合更新记录无效")?;
    let mut seen = std::collections::HashSet::new();
    for entry in swaps {
        let target = field(entry, "target")?;
        let stage = field(entry, "stage")?;
        let backup = field(entry, "backup")?;
        if !seen.insert(target.clone()) {
            return Err("重复更新目标".into());
        }
        if target == paths.npm_prefix {
            ensure_under(&paths.updates, &stage)?;
            if stage.parent() != Some(paths.updates.as_path())
                || !stage
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .starts_with("dsh-stage-")
                || backup != root.join("old-dsh")
            {
                return Err("DSH 暂存路径无效".into());
            }
        } else if target == paths.install_state_file() {
            if stage != root.join("install-mode.json") || backup != root.join("old-mode.json") {
                return Err("安装状态暂存路径无效".into());
            }
        } else {
            let profile = profile.as_ref().ok_or("缺少插件 profile")?;
            let name = target.file_name().ok_or("插件目标无效")?;
            if target.parent() != Some(profile.as_path())
                || ![
                    OsStr::new("node_modules"),
                    OsStr::new("package.json"),
                    OsStr::new("package-lock.json"),
                ]
                .contains(&name)
            {
                return Err("插件交换目标无效".into());
            }
            let plugin_root = profile.parent().unwrap().join(".dsh-launcher-update-web");
            if stage != plugin_root.join("candidate").join(name)
                || backup != plugin_root.join("backup").join(name)
            {
                return Err("插件交换暂存路径无效".into());
            }
        }
    }
    Ok(())
}
fn exchange(record: &Value) -> Result<(), String> {
    for entry in record["swaps"].as_array().ok_or("交换记录无效")? {
        if exists(&field(entry, "target")?) != (entry["original"] == true)
            || exists(&field(entry, "backup")?)
            || !exists(&field(entry, "stage")?)
        {
            return Err("更新目录已变化，未开始交换".into());
        }
    }
    for entry in record["swaps"].as_array().ok_or("交换记录无效")? {
        let target = field(entry, "target")?;
        let stage = field(entry, "stage")?;
        let backup = field(entry, "backup")?;
        if entry["original"] == true {
            rename(&target, &backup)?;
        }
        rename(&stage, &target)?;
    }
    Ok(())
}
fn rollback(record: &Value) -> Result<(), String> {
    for entry in record["swaps"]
        .as_array()
        .ok_or("交换记录无效")?
        .iter()
        .rev()
    {
        let target = field(entry, "target")?;
        let stage = field(entry, "stage")?;
        let backup = field(entry, "backup")?;
        if exists(&backup) {
            if exists(&target) && exists(&stage) {
                return Err("恢复目标已被外部修改，已保留备份".into());
            }
            if exists(&target) && !exists(&stage) {
                rename(&target, &stage)?;
            }
            rename(&backup, &target)?;
        } else if entry["original"] == false && !exists(&stage) && exists(&target) {
            rename(&target, &stage)?;
        }
    }
    Ok(())
}
fn cleanup(paths: &Paths, record: &Value) -> Result<(), String> {
    for entry in record["swaps"].as_array().ok_or("交换记录无效")? {
        remove(&field(entry, "backup")?)?;
        remove(&field(entry, "stage")?)?;
    }
    if let Some(profile) = record["profile"]
        .as_str()
        .filter(|_| record["owns_plugin_stage"] == true)
    {
        remove(
            &Path::new(profile)
                .parent()
                .ok_or("profile 无效")?
                .join(".dsh-launcher-update-web"),
        )?;
    }
    remove(&field(record, "root")?)?;
    remove(&journal(paths))
}
pub(super) fn recover(paths: &Paths) -> Result<(), String> {
    if !journal(paths).exists() {
        return Ok(());
    }
    let mut record = json_file(&journal(paths))?;
    validate(paths, &record)?;
    match record["phase"].as_str() {
        Some("done") | Some("prepared") => return cleanup(paths, &record),
        Some("committing") | Some("restored") => {}
        _ => return Err("联合更新状态无效，已保留备份".into()),
    }
    if record["phase"] == "committing" {
        if tracked_process_running(paths)? || probe_dsh(paths).identified {
            stop_dsh()?;
        }
        rollback(&record)?;
        record["phase"] = "restored".into();
        save(paths, &record)?;
    }
    if record["was_running"] == true {
        start_dsh().map_err(|e| format!("原版本已恢复，但恢复运行失败：{e}"))?;
    }
    cleanup(paths, &record)
}

struct PluginPlan {
    node: PathBuf,
    script: PathBuf,
    request: PathBuf,
    file: PathBuf,
    value: Value,
}
impl PluginPlan {
    fn invoke(&self, paths: &Paths, mode: &str) -> Result<Value, String> {
        let mut command = hidden_command(&self.node);
        command.arg(&self.script).arg(mode).arg(&self.request);
        if mode != "prepare" {
            command.arg(&self.file);
        }
        command
            .env("npm_config_cache", &paths.cache)
            .env("TEMP", &paths.temp)
            .env("TMP", &paths.temp);
        let output = run_capture(paths, &mut command, "更新所选插件", NPM_TIMEOUT, true)?;
        serde_json::from_str(&output).map_err(|e| e.to_string())
    }
    fn changed(&self) -> bool {
        self.value["updates"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
    }
    fn summary(&self) -> String {
        self.value["packages"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .map(|p| {
                        format!(
                            "{}：{} → {}\n{}{}",
                            p["name"].as_str().unwrap_or(""),
                            p["current"].as_str().unwrap_or(""),
                            p["target"].as_str().unwrap_or(""),
                            p["source"].as_str().unwrap_or(""),
                            p["reason"]
                                .as_str()
                                .map(|s| format!("\n{s}"))
                                .unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n")
            })
            .unwrap_or_default()
    }
}

pub(super) fn update(
    plugins_only: bool,
    progress: &dyn Fn(&str, bool),
    confirm: Option<&dyn Fn(&str) -> bool>,
) -> Result<String, String> {
    let paths = app_paths()?;
    recover(&paths)?;
    let current = discover_installation(&paths)?.ok_or("未找到 DSH，请先安装")?;
    let npm = find_command("npm.cmd").ok_or("未找到 npm")?;
    let npm_cli = npm
        .parent()
        .ok_or("npm 路径无效")?
        .join("node_modules/npm/bin/npm-cli.js");
    if !npm_cli.is_file() {
        return Err("无法定位 npm-cli.js".into());
    }
    let root = paths.updates.join(format!("joint-{}", transaction_nonce()));
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let mut record =
        json!({"phase":"prepared", "root":root, "profile":null, "was_running":false, "swaps":[]});
    save(&paths, &record)?;
    let result = (|| {
        progress("正在检查已保存的启用插件及版本…", true);
        let catalog = plugins::inspect_mode(
            &paths,
            &current,
            &paths.state.join("plugin-settings.json"),
            "catalog",
        )?;
        let profile = catalog["profileDir"]
            .as_str()
            .ok_or("无法确定目标 profile")?;
        record["profile"] = profile.into();
        validate(&paths, &record)?;
        save(&paths, &record)?;
        let plugin_root = Path::new(profile)
            .parent()
            .ok_or("profile 路径无效")?
            .join(".dsh-launcher-update-web");
        if exists(&plugin_root) {
            return Err("发现未完成的插件暂存，已保留；请检查更新记录".into());
        }
        record["owns_plugin_stage"] = true.into();
        save(&paths, &record)?;
        let mut plan = PluginPlan {
            node: current.node.clone(),
            script: paths.state.join("plugin-update.cjs"),
            request: root.join("request.json"),
            file: root.join("plan.json"),
            value: Value::Null,
        };
        atomic_write(&plan.script, include_bytes!("plugin_update.cjs"))?;
        atomic_write(&plan.request, &serde_json::to_vec(&json!({"catalog":catalog, "npmCli":npm_cli, "temp":paths.temp, "settingsFile":paths.state.join("plugin-settings.json")})).unwrap())?;
        plan.value = plan.invoke(&paths, "prepare")?;
        atomic_write(&plan.file, &serde_json::to_vec(&plan.value).unwrap())?;
        let mut target = current.version.clone();
        let mut notice = None;
        let repair = !plugins_only
            && (paths.repair_file().exists() || native_deps::needs_repair(&paths, &current)?);
        if !plugins_only {
            progress("正在查询 DSH 官方版本…", true);
            let latest = latest_dsh_version(&paths, &npm)?;
            if parse_version(&latest) > parse_version(&target) {
                target = latest.clone();
            }
            let source = dsh_update::source_version(&paths);
            notice = dsh_update::source_notice(&latest, source.as_deref().map_err(String::as_str));
        }
        let dsh_changed = target != current.version || repair;
        let summary = plan.summary();
        if !dsh_changed && !plan.changed() {
            return Ok(format!(
                "没有可自动更新的版本。\n{summary}\n{}",
                notice.unwrap_or_default()
            ));
        }
        let was_running = tracked_process_running(&paths)? || probe_dsh(&paths).identified;
        if !was_running && tcp_open(DSH_PORT) {
            return Err("3080 端口已被其他程序占用，无法安全更新".into());
        }
        record["was_running"] = was_running.into();
        save(&paths, &record)?;
        let message = format!(
            "目标 profile：{profile}\n\nDSH：{} → {target}\n\n{summary}\n\n{}\n{}",
            current.version,
            if was_running {
                "提交前将短暂停止 DSH，完成后恢复运行。"
            } else {
                "更新后 DSH 保持停止。"
            },
            notice.unwrap_or_default()
        );
        if CANCEL.load(Ordering::Acquire) || confirm.is_some_and(|f| !f(&message)) {
            return Err("操作已取消".into());
        }
        let profile_mode = current.profile;
        let mut candidate = current.clone();
        if dsh_changed {
            progress("正在暂存 DSH…", true);
            let stage = stage_dsh(&paths, &npm, &target)?;
            record["swaps"].as_array_mut().unwrap().push(swap(
                paths.npm_prefix.clone(),
                stage.clone(),
                root.join("old-dsh"),
            ));
            let mode_stage = root.join("install-mode.json");
            atomic_write(&mode_stage, &serde_json::to_vec(&json!({"profile": if profile_mode == ProfileMode::Portable {"portable"} else {"user"}})).unwrap())?;
            record["swaps"].as_array_mut().unwrap().push(swap(
                paths.install_state_file(),
                mode_stage,
                root.join("old-mode.json"),
            ));
            save(&paths, &record)?;
            candidate = Installation {
                source: Source::Managed,
                node: current.node.clone(),
                entry: stage.join("node_modules/@deepseek-ai/dsh/lib/bin.js"),
                version: target.clone(),
                profile: profile_mode,
            };
            native_deps::prepare(&paths, &npm, &stage, &candidate)?;
        }
        if plan.changed() {
            progress("正在暂存所选插件…", true);
            plan.invoke(&paths, "install")?;
            let plugin_root = field(&plan.value, "root")?;
            fs::create_dir_all(plugin_root.join("backup")).map_err(|e| e.to_string())?;
            for name in ["node_modules", "package.json", "package-lock.json"] {
                record["swaps"].as_array_mut().unwrap().push(swap(
                    Path::new(profile).join(name),
                    plugin_root.join("candidate").join(name),
                    plugin_root.join("backup").join(name),
                ));
            }
            save(&paths, &record)?;
        }
        progress("正在验证 DSH 与插件候选组合…", true);
        let mut command = hidden_command(&candidate.node);
        command
            .arg(paths.state.join("plugin-bridge.cjs"))
            .arg(&candidate.entry)
            .arg(paths.state.join("plugin-settings.json"))
            .arg(if profile_mode == ProfileMode::Portable {
                paths.profile.as_os_str()
            } else {
                OsStr::new("")
            })
            .arg("preflight");
        if plan.changed() {
            command.arg(field(&plan.value, "candidate")?);
        }
        run_capture(
            &paths,
            &mut command,
            "验证联合更新候选",
            QUERY_TIMEOUT,
            true,
        )?;
        plan.invoke(&paths, "check")?;
        if CANCEL.load(Ordering::Acquire) {
            return Err("操作已取消".into());
        }
        if was_running != (tracked_process_running(&paths)? || probe_dsh(&paths).identified) {
            return Err("DSH 运行状态已变化，请重新检查更新".into());
        }
        validate(&paths, &record)?;
        progress("正在提交更新，此阶段不可取消…", false);
        record["phase"] = "committing".into();
        save(&paths, &record)?;
        if was_running {
            stop_dsh()?;
        }
        plan.invoke(&paths, "check")?;
        exchange(&record)?;
        #[cfg(feature = "test-isolated-port")]
        if was_running && dsh_changed && paths.state.join("qa-fail-next-joint-start").exists() {
            // Isolated-port QA only: fail the real candidate process, then exercise recovery.
            fs::remove_file(paths.state.join("qa-fail-next-joint-start"))
                .map_err(|e| e.to_string())?;
            fs::write(
                paths
                    .npm_prefix
                    .join("node_modules/@deepseek-ai/dsh/lib/bin.js"),
                "process.exit(93);",
            )
            .map_err(|e| e.to_string())?;
        }
        if was_running {
            start_dsh()?;
        }
        record["phase"] = "done".into();
        save(&paths, &record)?;
        clear_repair_needed(&paths);
        Ok(format!(
            "更新完成{}。\nDSH {target}\n{summary}",
            if was_running {
                "，DSH 已恢复运行"
            } else {
                ""
            }
        ))
    })();
    if let Err(recovery) = recover(&paths) {
        return Err(format!(
            "{}\n事务恢复或清理未完成，已保留记录：{recovery}",
            result
                .as_ref()
                .err()
                .map(String::as_str)
                .unwrap_or("更新已提交")
        ));
    }
    cleanup_npm_cache(&paths);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locked_later_file_rolls_back_an_already_exchanged_file() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = env::temp_dir().join(format!("joint-lock-{}", transaction_nonce()));
        fs::create_dir_all(&root).unwrap();
        let entries: Vec<_> = (0..2)
            .map(|i| {
                let target = root.join(format!("target{i}"));
                let stage = root.join(format!("stage{i}"));
                fs::write(&target, "old").unwrap();
                fs::write(&stage, "new").unwrap();
                swap(target, stage, root.join(format!("backup{i}")))
            })
            .collect();
        let locked = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(field(&entries[1], "target").unwrap())
            .unwrap();
        let record = json!({"swaps":entries});
        assert!(exchange(&record).is_err());
        rollback(&record).unwrap();
        for entry in record["swaps"].as_array().unwrap() {
            assert_eq!(
                fs::read_to_string(field(entry, "target").unwrap()).unwrap(),
                "old"
            );
        }
        drop(locked);
        remove(&root).unwrap();
    }

    #[test]
    fn rollback_of_first_install_and_cleanup_of_directory_link_preserve_external_data() {
        let root = env::temp_dir().join(format!("joint-first-{}", transaction_nonce()));
        fs::create_dir_all(&root).unwrap();
        let stage = root.join("stage");
        fs::write(&stage, "new").unwrap();
        let record = json!({"swaps":[swap(root.join("target"),stage.clone(),root.join("backup"))]});
        exchange(&record).unwrap();
        rollback(&record).unwrap();
        rollback(&record).unwrap();
        assert!(!root.join("target").exists());
        assert_eq!(fs::read_to_string(stage).unwrap(), "new");
        let external = root.join("external");
        fs::create_dir(&external).unwrap();
        fs::write(external.join("keep"), "keep").unwrap();
        // Directory junctions do not require Windows developer mode.
        let status = hidden_command("cmd.exe")
            .args(["/C", "mklink", "/J"])
            .arg(root.join("link"))
            .arg(&external)
            .status()
            .unwrap();
        assert!(status.success());
        remove(&root.join("link")).unwrap();
        assert!(external.join("keep").exists());
        remove(&root).unwrap();
    }
    #[test]
    fn rollback_restores_each_interrupted_move_and_is_idempotent() {
        for moves in 0..=4 {
            let root = env::temp_dir().join(format!("joint-test-{}", transaction_nonce()));
            fs::create_dir_all(&root).unwrap();
            let entries: Vec<_> = (0..2)
                .map(|i| {
                    let target = root.join(format!("target{i}"));
                    let stage = root.join(format!("stage{i}"));
                    fs::write(&target, "old").unwrap();
                    fs::write(&stage, "new").unwrap();
                    swap(target, stage, root.join(format!("backup{i}")))
                })
                .collect();
            for index in 0..moves {
                let entry = &entries[index / 2];
                if index % 2 == 0 {
                    rename(
                        &field(entry, "target").unwrap(),
                        &field(entry, "backup").unwrap(),
                    )
                    .unwrap();
                } else {
                    rename(
                        &field(entry, "stage").unwrap(),
                        &field(entry, "target").unwrap(),
                    )
                    .unwrap();
                }
            }
            let record = json!({"swaps":entries});
            rollback(&record).unwrap();
            rollback(&record).unwrap();
            for entry in record["swaps"].as_array().unwrap() {
                assert_eq!(
                    fs::read_to_string(field(entry, "target").unwrap()).unwrap(),
                    "old"
                );
            }
            remove(&root).unwrap();
        }
    }
}
