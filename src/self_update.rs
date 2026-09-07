//! One-shot, same-volume update of the four portable program files.
use super::*;

const FILES: [&str; 4] = [
    "DSH-Launcher.exe",
    "runtime-manifest.json",
    "dshctl.cmd",
    "portable.flag",
];
const DIRECTORY: &str = "launcher-update";
pub(super) const EXIT: u32 = WM_APP + 8;
pub(super) const READY: u32 = WM_APP + 9;
pub(super) const CONFIRM: u32 = WM_APP + 10;
static READY_FILE: OnceLock<PathBuf> = OnceLock::new();

fn stage(root: &Path) -> PathBuf {
    root.join("data").join("updates").join(DIRECTORY)
}
fn record_file(root: &Path) -> PathBuf {
    stage(root).join("transaction.json")
}
fn read_record(root: &Path) -> Result<serde_json::Value, String> {
    serde_json::from_slice(&fs::read(record_file(root)).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}
fn write_record(root: &Path, record: &serde_json::Value) -> Result<(), String> {
    atomic_write(
        &record_file(root),
        &serde_json::to_vec(record).map_err(|e| e.to_string())?,
    )
}
fn number(record: &serde_json::Value, field: &str) -> Result<u64, String> {
    record[field]
        .as_u64()
        .ok_or_else(|| format!("更新记录缺少 {field}"))
}
fn process_matches(
    record: &serde_json::Value,
    prefix: &str,
) -> Result<Option<ProcessHandle>, String> {
    let pid = u32::try_from(number(record, &format!("{prefix}_pid"))?)
        .map_err(|_| "更新进程 PID 无效")?;
    if pid == 0 {
        return Ok(None);
    }
    let Some(handle) = ProcessHandle::open(pid)? else {
        return Ok(None);
    };
    Ok(
        (handle.times()?.0 == number(record, &format!("{prefix}_created"))? && handle.running()?)
            .then_some(handle),
    )
}
fn copy_group(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for name in FILES {
        let bytes = fs::read(from.join(name)).map_err(|e| format!("无法读取 {name}：{e}"))?;
        if fs::read(to.join(name)).ok().as_deref() == Some(bytes.as_slice()) {
            continue;
        }
        atomic_write(&to.join(name), &bytes)?;
    }
    Ok(())
}
fn cleanup(root: &Path) -> Result<(), String> {
    let directory = stage(root);
    // Fixed descendant only; refuse redirected update directories.
    check_directory(root)?;
    if directory.exists() {
        fs::remove_dir_all(directory).map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn check_directory(root: &Path) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt;
    let mut directory = root.to_path_buf();
    for part in ["data", "updates", DIRECTORY, "candidate", "backup"] {
        if part == "backup" {
            directory = stage(root);
        }
        directory.push(part);
        if let Ok(info) = fs::symlink_metadata(&directory) {
            if info.file_attributes() & 0x400 != 0 {
                return Err("更新目录不能是链接或重解析点".to_owned());
            }
        }
    }
    Ok(())
}

pub(super) fn recover(root: &Path) -> Result<bool, String> {
    check_directory(root)?;
    if !record_file(root).exists() {
        return Ok(false);
    }
    let record = read_record(root)?;
    if process_matches(&record, "helper")?.is_some() {
        if record["phase"] == "done" || record["phase"] == "restored" {
            return Ok(false);
        }
        return Err("启动器更新助手仍在运行，请稍候".to_owned());
    }
    match record["phase"].as_str() {
        Some("committing" | "verifying") => {
            // The current EXE may itself be the partially committed version.
            // Restore only after this process exits, then run the restored EXE.
            return schedule_recovery(root, record);
        }
        Some("prepared" | "done" | "restored") => {}
        _ => return Err("启动器更新事务阶段无效，保留备份供恢复".to_owned()),
    }
    cleanup(root)?;
    Ok(false)
}

fn schedule_recovery(root: &Path, mut record: serde_json::Value) -> Result<bool, String> {
    let helper_path = stage(root).join("recovery.exe");
    fs::copy(env::current_exe().map_err(|e| e.to_string())?, &helper_path)
        .map_err(|e| e.to_string())?;
    let pid = std::process::id();
    let parent = ProcessHandle::open(pid)?.ok_or("恢复父进程已退出")?;
    record["parent_pid"] = pid.into();
    record["parent_created"] = parent.times()?.0.into();
    record["helper_pid"] = 0.into();
    record["helper_created"] = 0.into();
    let token = record["token"]
        .as_str()
        .filter(|v| valid_token(v))
        .ok_or("恢复事务标识无效")?
        .to_owned();
    let _ = fs::remove_file(stage(root).join("helper-ready"));
    write_record(root, &record)?;
    let child = hidden_command(helper_path)
        .arg("--self-update-recover")
        .arg(root)
        .arg(&token)
        .env_remove(CLI_OUTPUT_ENV)
        .spawn()
        .map_err(|e| e.to_string())?;
    let helper = ProcessHandle::open(child.id())?.ok_or("恢复助手已退出")?;
    record["helper_pid"] = child.id().into();
    record["helper_created"] = helper.times()?.0.into();
    write_record(root, &record)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline && helper.running()? {
        if fs::read_to_string(stage(root).join("helper-ready"))
            .ok()
            .as_deref()
            == Some(&token)
        {
            return Ok(true);
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err("恢复助手未就绪，已保留备份".to_owned())
}

pub(super) fn confirm(hwnd: HWND, message: &str) -> bool {
    let message = message.to_owned();
    unsafe { SendMessageW(hwnd, CONFIRM, 0, &message as *const String as isize) != 0 }
}

pub(super) fn prepare(
    paths: &Paths,
    hwnd: HWND,
    progress: &dyn Fn(&str, bool),
) -> Result<bool, String> {
    let script = format!("$ErrorActionPreference='Stop';$ProgressPreference='SilentlyContinue';$r=Invoke-RestMethod -UseBasicParsing -TimeoutSec 15 -Headers @{{'User-Agent'='DSH-Launcher'}} -Uri '{RELEASE_API_URL}';if($r.draft -or $r.prerelease){{throw 'Not a stable release'}};[Console]::Out.Write($r.tag_name)");
    let output = run_capture(
        paths,
        hidden_command("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &script,
            ])
            .env("TEMP", &paths.temp)
            .env("TMP", &paths.temp),
        "检查启动器更新",
        LAUNCHER_QUERY_TIMEOUT,
        true,
    )?;
    let latest = output.trim().strip_prefix('v').ok_or("Release 标签无效")?;
    let version = parse_version(latest).ok_or("Release 版本无效")?;
    if !version.prerelease.is_empty() {
        return Err("启动器更新要求正式版本".to_owned());
    }
    if version <= parse_version(APP_VERSION).unwrap() {
        progress("启动器已是最新版本", false);
        return Ok(false);
    }
    if !confirm(hwnd, &format!("将启动器 v{APP_VERSION} 更新到 v{latest}。\n更新后重新打开窗口，运行中的 DSH 保持运行。\n\n继续？")) { return Err("操作已取消".to_owned()); }
    let root = paths.data.parent().ok_or("启动器目录无效")?;
    if recover(root)? {
        return Err("启动器正在恢复，请重新打开".to_owned());
    }
    if stage(root).exists() {
        cleanup(root)?;
    }
    fs::create_dir_all(stage(root)).map_err(|e| e.to_string())?;
    let result = (|| {
        progress("正在下载并验证启动器更新，可取消...", true);
        let script = stage(root).join("download.ps1");
        atomic_write(&script, include_bytes!("launcher_download.ps1"))?;
        run_capture(
            paths,
            hidden_command("powershell.exe")
                .args([
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                ])
                .arg(script)
                .arg("-Version")
                .arg(latest)
                .arg("-Stage")
                .arg(stage(root))
                .env("TEMP", &paths.temp)
                .env("TMP", &paths.temp),
            "下载启动器",
            Duration::from_secs(600),
            true,
        )?;
        let candidate = stage(root).join("candidate");
        let token = transaction_nonce().to_string();
        run_capture(
            paths,
            hidden_command(candidate.join(FILES[0]))
                .args(["--self-update-probe", latest, &token])
                .env_remove(CLI_OUTPUT_ENV),
            "检查候选启动器",
            Duration::from_secs(30),
            true,
        )?;
        if fs::read_to_string(candidate.join("probe-ready"))
            .ok()
            .as_deref()
            != Some(&token)
        {
            return Err("候选启动器初始化检查失败".to_owned());
        }
        if CANCEL.load(Ordering::Acquire) {
            return Err("操作已取消".to_owned());
        }
        progress("候选已验证，正在准备文件提交...", false);
        if CANCEL.load(Ordering::Acquire) {
            return Err("操作已取消".to_owned());
        }
        copy_group(root, &stage(root).join("backup"))?;
        let helper_path = stage(root).join("helper.exe");
        fs::copy(env::current_exe().map_err(|e| e.to_string())?, &helper_path)
            .map_err(|e| e.to_string())?;
        let parent_pid = std::process::id();
        let parent = ProcessHandle::open(parent_pid)?.ok_or("父进程不存在")?;
        let mut record = serde_json::json!({"phase":"prepared", "version":latest, "token":token,
            "parent_pid":parent_pid, "parent_created":parent.times()?.0, "helper_pid":0, "helper_created":0});
        write_record(root, &record)?;
        let child = hidden_command(helper_path)
            .arg("--self-update-apply")
            .arg(root)
            .arg(&token)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env_remove(CLI_OUTPUT_ENV)
            .spawn()
            .map_err(|e| e.to_string())?;
        let helper = ProcessHandle::open(child.id())?.ok_or("更新助手已退出")?;
        record["helper_pid"] = child.id().into();
        record["helper_created"] = helper.times()?.0.into();
        write_record(root, &record)?;
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline && helper.running()? {
            if fs::read_to_string(stage(root).join("helper-ready"))
                .ok()
                .as_deref()
                == Some(&token)
            {
                return Ok(true);
            }
            thread::sleep(Duration::from_millis(50));
        }
        // No commit is possible while this parent remains alive.
        if helper.running()? {
            unsafe {
                TerminateProcess(helper.0, 1);
                WaitForSingleObject(helper.0, 5000);
            }
        }
        Err("更新助手未就绪，已保留当前启动器".to_owned())
    })();
    if result.is_err() {
        let _ = recover(root);
    }
    result
}

pub(super) fn internal(args: &[String]) -> Result<Option<bool>, String> {
    if args
        .get(1)
        .is_none_or(|arg| !arg.starts_with("--self-update-"))
    {
        return Ok(None);
    }
    ensure_not_elevated()?;
    let executable = env::current_exe().map_err(|e| e.to_string())?;
    let root = executable.parent().ok_or("更新目录无效")?;
    match args.get(1).map(String::as_str) {
        Some("--self-update-probe")
            if args.len() == 4 && args[2] == APP_VERSION && valid_token(&args[3]) =>
        {
            Paths::at_root(root)?;
            atomic_write(&root.join("probe-ready"), args[3].as_bytes())?;
            Ok(Some(false))
        }
        Some("--self-update-apply" | "--self-update-recover")
            if args.len() == 4 && valid_token(&args[3]) =>
        {
            let recovery = args[1] == "--self-update-recover";
            let target = fs::canonicalize(&args[2]).map_err(|e| e.to_string())?;
            if fs::canonicalize(stage(&target).join(if recovery {
                "recovery.exe"
            } else {
                "helper.exe"
            }))
            .map_err(|e| e.to_string())?
                != fs::canonicalize(executable).map_err(|e| e.to_string())?
            {
                return Err("更新助手路径无效".to_owned());
            }
            apply(&target, &args[3], recovery)?;
            Ok(Some(false))
        }
        Some("--self-update-ready") if args.len() == 3 && valid_token(&args[2]) => {
            let record = read_record(root)?;
            if record["phase"] != "verifying"
                || record["version"] != APP_VERSION
                || record["token"] != args[2]
                || process_matches(&record, "helper")?.is_none()
            {
                return Err("更新就绪握手无效".to_owned());
            }
            READY_FILE
                .set(stage(root).join("window-ready"))
                .map_err(|_| "重复握手")?;
            Ok(Some(true))
        }
        _ => Err("内部更新参数无效".to_owned()),
    }
}
fn valid_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 80
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
pub(super) fn window_ready() {
    if let Some(path) = READY_FILE.get() {
        let value = serde_json::json!({"version":APP_VERSION, "pid":std::process::id()});
        let _ = atomic_write(path, &serde_json::to_vec(&value).unwrap());
    }
}
fn apply(root: &Path, token: &str, recovery: bool) -> Result<(), String> {
    check_directory(root)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut record = loop {
        let record = read_record(root)?;
        if record["token"] != token
            || if recovery {
                !matches!(record["phase"].as_str(), Some("committing" | "verifying"))
            } else {
                record["phase"] != "prepared"
            }
        {
            return Err("更新事务身份无效".to_owned());
        }
        if record["helper_pid"].as_u64() == Some(u64::from(std::process::id())) {
            break record;
        }
        if Instant::now() >= deadline {
            return Err("父进程未登记更新助手".to_owned());
        }
        thread::sleep(Duration::from_millis(50));
    };
    let parent = process_matches(&record, "parent")?.ok_or("更新父进程身份无效")?;
    atomic_write(&stage(root).join("helper-ready"), token.as_bytes())?;
    if unsafe { WaitForSingleObject(parent.0, 30_000) } != WAIT_OBJECT_0 {
        return Err("旧启动器未退出，未替换文件".to_owned());
    }
    let _guard = acquire_action_mutex().ok_or("另一个 DSH 操作正在执行，更新未提交")?;
    if recovery {
        copy_group(&stage(root).join("backup"), root)?;
        record["phase"] = "restored".into();
        write_record(root, &record)?;
        hidden_command(root.join(FILES[0]))
            .env_remove(CLI_OUTPUT_ENV)
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    let result = (|| {
        record["phase"] = "committing".into();
        write_record(root, &record)?;
        copy_group(&stage(root).join("candidate"), root)?;
        record["phase"] = "verifying".into();
        write_record(root, &record)?;
        let mut child = hidden_command(root.join(FILES[0]))
            .args(["--self-update-ready", token])
            .env_remove(CLI_OUTPUT_ENV)
            .spawn()
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                break;
            }
            let ready = fs::read(stage(root).join("window-ready"))
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
            if ready.is_some_and(|v| {
                v["version"] == record["version"]
                    && v["pid"].as_u64() == Some(u64::from(child.id()))
            }) {
                record["phase"] = "done".into();
                write_record(root, &record)?;
                return Ok(());
            }
            thread::sleep(Duration::from_millis(50));
        }
        child
            .kill()
            .map_err(|e| format!("新版退出失败，保留事务：{e}"))?;
        child.wait().map_err(|e| e.to_string())?;
        Err("新版窗口未就绪，恢复旧版".to_owned())
    })();
    if let Err(error) = result {
        copy_group(&stage(root).join("backup"), root)?;
        record["phase"] = "restored".into();
        write_record(root, &record)?;
        append_log(&root.join("data/logs/launcher.log"), &error);
        // The restored launcher can start while this helper finishes; no recovery needed.
        hidden_command(root.join(FILES[0]))
            .env_remove(CLI_OUTPUT_ENV)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    // This helper EXE remains locked until exit. Next ordinary launch removes the
    // completed directory; backups can be removed immediately after a successful handshake.
    if record["phase"] == "done" {
        let _ = fs::remove_dir_all(stage(root).join("backup"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn internal_tokens_are_bounded_and_path_free() {
        for token in ["", "../data", "x\\y", "a b"] {
            assert!(!valid_token(token));
        }
        assert!(valid_token("123-456"));
    }
    #[test]
    fn group_restore_is_repeatable() {
        let root = env::temp_dir().join(format!("launcher-update-test-{}", transaction_nonce()));
        let backup = root.join("backup");
        fs::create_dir_all(&backup).unwrap();
        for file in FILES {
            fs::write(backup.join(file), file).unwrap();
        }
        copy_group(&backup, &root.join("target")).unwrap();
        copy_group(&backup, &root.join("target")).unwrap();
        for file in FILES {
            assert_eq!(
                fs::read_to_string(root.join("target").join(file)).unwrap(),
                file
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
}
