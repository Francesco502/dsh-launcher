#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::VecDeque;
use std::env;
use std::ffi::{c_void, OsStr};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use std::os::windows::process::CommandExt;

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, FILETIME, HANDLE, HWND, POINT, RECT,
};
use windows_sys::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMSBT_MAINWINDOW, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR,
    DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, Ellipse, EndPaint,
    FillRect, GetStockObject, GradientFill, InvalidateRect, RoundRect, SelectObject, SetBkMode,
    SetDCBrushColor, SetDCPenColor, SetTextColor, UpdateWindow, CLEARTYPE_QUALITY,
    CLIP_DEFAULT_PRECIS, DC_BRUSH, DC_PEN, DEFAULT_CHARSET, DEFAULT_PITCH, DT_CENTER, DT_LEFT,
    DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_NORMAL, FW_SEMIBOLD, GRADIENT_FILL_RECT_V,
    GRADIENT_RECT, NULL_BRUSH, OUT_DEFAULT_PRECIS, PAINTSTRUCT, TRANSPARENT, TRIVERTEX,
};
use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemServices::{SS_CENTERIMAGE, SS_OWNERDRAW};
use windows_sys::Win32::System::Threading::{
    CreateMutexW, GetExitCodeProcess, GetProcessTimes, OpenProcess, ReleaseMutex, CREATE_NO_WINDOW,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_DISABLED, ODS_FOCUS, ODS_SELECTED};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows_sys::Win32::UI::Shell::{
    IsUserAnAdmin, ShellExecuteW, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_SELECT, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, FindWindowW, GetClientRect, GetCursorPos, GetDlgCtrlID, GetDlgItem,
    GetMessageW, GetParent, GetSystemMetrics, GetWindowLongPtrW, GetWindowTextW, KillTimer,
    LoadCursorW, LoadIconW, MessageBoxW, PostMessageW, PostQuitMessage, RegisterClassExW,
    SendMessageW, SetForegroundWindow, SetTimer, SetWindowLongPtrW, SetWindowTextW, ShowWindow,
    TrackPopupMenu, TranslateMessage, WindowFromPoint, BS_OWNERDRAW, CREATESTRUCTW, GWLP_USERDATA,
    HICON, IDC_ARROW, IDI_APPLICATION, MB_ICONERROR, MB_OK, MF_SEPARATOR, MF_STRING, MSG,
    SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, SW_RESTORE, SW_SHOW, TPM_BOTTOMALIGN, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, WM_APP, WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU, WM_CTLCOLORSTATIC, WM_DESTROY,
    WM_DRAWITEM, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_NCCREATE, WM_NULL, WM_PAINT, WM_RBUTTONUP,
    WM_SETFONT, WM_TIMER, WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW, WS_EX_TRANSPARENT,
    WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

const DSH_PORT: u16 = 3080;
const RUNTIME_DIRECTORY: &str = "DSH-Runtime";
const NPM_GLOBAL_DIRECTORY: &str = "npm-global";
const DSH_PACKAGE: &str = "@deepseek-ai/dsh";
const NATIVE_DSH_PID_FILE: &str = "dsh-launcher-native.pid";
const DSH_LOG_DIRECTORY: &str = "logs";
const DSH_LOG_TAIL_BYTES: u64 = 4096;
const DSH_STDOUT_LOG_FILE: &str = "dsh-launcher-native.out.log";
const DSH_STDERR_LOG_FILE: &str = "dsh-launcher-native.err.log";
const DSH_PREFLIGHT_STDOUT_LOG_FILE: &str = "dsh-launcher-preflight.out.log";
const DSH_PREFLIGHT_STDERR_LOG_FILE: &str = "dsh-launcher-preflight.err.log";
const DSH_START_TIMEOUT: Duration = Duration::from_secs(45);
const DSH_STOP_TIMEOUT: Duration = Duration::from_secs(15);
const DSH_UPDATE_COMMAND_TIMEOUT: Duration = Duration::from_secs(60);
const UPGRADE_PREFLIGHT_PORT: u16 = 3081;
const PROCESS_STILL_ACTIVE: u32 = 259;
const WEB_URL: &str = "http://127.0.0.1:3080/";
const QUOTA_CONFIG_PATH: &str = "/api/dsh-quota/config";
const DSH_LATEST_REGISTRY_URL: &str = "https://registry.npmjs.org/@deepseek-ai%2fdsh/latest";
const DSH_LATEST_VERSION_SCRIPT: &str = "fetch(process.argv[1],{signal:AbortSignal.timeout(10000)}).then(r=>{if(!r.ok)throw new Error('HTTP '+r.status);return r.json()}).then(p=>console.log(p.version)).catch(e=>{console.error(e.message);process.exit(1)})";
const CREATE_NO_WINDOW_FLAG: u32 = CREATE_NO_WINDOW;

const TRAY_MESSAGE: u32 = WM_APP + 1;
const STATUS_MESSAGE: u32 = WM_APP + 2;
const SHOW_WINDOW_MESSAGE: u32 = WM_APP + 3;
const NIN_KEYSELECT: u32 = 1025;

const CMD_START: u32 = 1001;
const CMD_RESTART: u32 = 1002;
const CMD_STOP: u32 = 1003;
const CMD_UPGRADE: u32 = 1004;
const CMD_EXIT: u32 = 1005;
const CMD_SHOW: u32 = 1006;
const CMD_OPEN_WEB: u32 = 1007;
const ID_TITLE: u32 = 1101;
const ID_SUBTITLE: u32 = 1102;
const ID_STATUS: u32 = 1103;
const ID_SECTION: u32 = 1104;
const ID_FOOTER: u32 = 1105;
const TRAY_RETRY_TIMER_ID: usize = 1;
const TRAY_RETRY_INTERVAL_MS: u32 = 1000;
const HOVER_TIMER_ID: usize = 2;
const HOVER_TIMER_INTERVAL_MS: u32 = 50;
const HEALTH_TIMER_ID: usize = 3;
const HEALTH_TIMER_INTERVAL_MS: u32 = 5000;
const HOVER_STEPS: usize = 4;
const WINDOW_CLASS: &str = "DeepSeekHarnessDshControlWindow";
const WINDOW_TITLE: &str = "DSH 服务管理";
const ICON_RESOURCE_ID: usize = 1;
const GRAYSCALE_ICON_RESOURCE_ID: usize = 2;
const WINDOW_WIDTH: i32 = 620;
const WINDOW_HEIGHT: i32 = 460;
const BUTTON_STYLE: u32 = BS_OWNERDRAW as u32;

const COLOR_BACKGROUND: u32 = rgb(239, 246, 255);
const COLOR_BACKGROUND_TOP: u32 = rgb(231, 243, 255);
const COLOR_BACKGROUND_BOTTOM: u32 = rgb(247, 242, 255);
const COLOR_SURFACE: u32 = rgb(248, 251, 255);
const COLOR_SURFACE_HOVER: u32 = rgb(255, 255, 255);
const COLOR_SURFACE_PRESSED: u32 = rgb(226, 239, 255);
const COLOR_BORDER: u32 = rgb(205, 219, 237);
const COLOR_HIGHLIGHT: u32 = rgb(255, 255, 255);
const COLOR_SHADOW: u32 = rgb(188, 207, 230);
const COLOR_TEXT: u32 = rgb(24, 39, 61);
const COLOR_MUTED: u32 = rgb(82, 101, 129);
const COLOR_DISABLED: u32 = rgb(137, 151, 171);
const COLOR_GREEN: u32 = rgb(15, 143, 105);
const COLOR_RED: u32 = rgb(196, 43, 76);
const COLOR_BLUE: u32 = rgb(15, 108, 189);
const COLOR_CYAN: u32 = rgb(0, 120, 146);
const COLOR_PURPLE: u32 = rgb(112, 70, 186);
const COLOR_AMBER: u32 = rgb(180, 112, 0);

const fn rgb(red: u32, green: u32, blue: u32) -> u32 {
    red | (green << 8) | (blue << 16)
}

#[derive(Clone, Copy, Debug)]
enum Action {
    Start,
    Restart,
    Stop,
    Upgrade,
    OpenWeb,
}

impl Action {
    fn from_name(value: &str) -> Option<Self> {
        match value {
            "start" => Some(Self::Start),
            "restart" => Some(Self::Restart),
            "stop" | "close" => Some(Self::Stop),
            "upgrade" | "update" => Some(Self::Upgrade),
            "open" | "web" => Some(Self::OpenWeb),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Start => "启动服务",
            Self::Restart => "重启服务",
            Self::Stop => "停止服务",
            Self::Upgrade => "检查更新",
            Self::OpenWeb => "打开网页",
        }
    }
}

struct AppState {
    hicon: usize,
    gray_hicon: usize,
    tray_hicon: AtomicUsize,
    background_brush: usize,
    title_font: usize,
    body_font: usize,
    small_font: usize,
    button_font: usize,
    status_hwnd: AtomicUsize,
    tray_added: AtomicBool,
    busy: AtomicBool,
    health_checking: AtomicBool,
    hover_levels: [AtomicUsize; 5],
    messages: Mutex<VecDeque<String>>,
    last_health: Mutex<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NativeDshProcess {
    pid: u32,
    started_at: u64,
}

struct MutexGuard(HANDLE);

impl Drop for MutexGuard {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.0);
            CloseHandle(self.0);
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if let Some(action) = parse_action(&args) {
        attach_parent_console();
        let code = match ensure_not_elevated().and_then(|_| execute_action(action)) {
            Ok(message) => {
                println!("{message}");
                0
            }
            Err(error) => {
                eprintln!("{error}");
                1
            }
        };
        std::process::exit(code);
    }

    let _instance = match acquire_single_instance() {
        Some(value) => value,
        None => return,
    };

    if let Err(error) = ensure_not_elevated().and_then(|_| run_app()) {
        show_error_box(&error);
        std::process::exit(1);
    }
}

fn parse_action(args: &[String]) -> Option<Action> {
    if args.len() == 3 && args[1] == "--action" {
        return Action::from_name(&args[2]);
    }
    None
}

fn acquire_single_instance() -> Option<MutexGuard> {
    let name = to_wide("Local\\DeepSeekHarness.DshLauncher");
    let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
    if handle.is_null() {
        return None;
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        let class_name = to_wide(WINDOW_CLASS);
        let existing = unsafe { FindWindowW(class_name.as_ptr(), std::ptr::null()) };
        if !existing.is_null() {
            unsafe {
                PostMessageW(existing, SHOW_WINDOW_MESSAGE, 0, 0);
            }
        }
        unsafe {
            CloseHandle(handle);
        }
        return None;
    }
    Some(MutexGuard(handle))
}

fn execute_action(action: Action) -> Result<String, String> {
    match action {
        Action::Start => start_dsh(),
        Action::Restart => restart_dsh(),
        Action::Stop => stop_dsh(),
        Action::Upgrade => upgrade_dsh(),
        Action::OpenWeb => open_web_ui(),
    }
}

fn attach_parent_console() {
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

fn ensure_not_elevated() -> Result<(), String> {
    if unsafe { IsUserAnAdmin() } != 0 {
        Err(
            "请不要以管理员身份运行 DSH 启动器；它必须使用当前 Windows 用户的 .dsh 配置与插件。"
                .to_owned(),
        )
    } else {
        Ok(())
    }
}

fn start_dsh() -> Result<String, String> {
    if is_native_dsh_running()? {
        return Ok("服务已在运行 · http://127.0.0.1:3080".to_owned());
    }

    if is_dsh_running() {
        return Err("端口 3080 正被其他服务占用。请先关闭该服务后再启动。".to_owned());
    }

    let stdout = open_native_dsh_log(DSH_STDOUT_LOG_FILE)?;
    let stderr = open_native_dsh_log(DSH_STDERR_LOG_FILE)?;
    let mut command = native_dsh_command()?;
    command
        .args(["web", "--no-open", "--host", "127.0.0.1", "--port", "3080"])
        .stdout(stdout)
        .stderr(stderr);
    let mut child = command
        .spawn()
        .map_err(|error| format!("启动服务失败：{error}"))?;
    let pid = child.id();
    let process = match write_native_dsh_process(pid) {
        Ok(process) => process,
        Err(error) => {
            let _ = child.kill();
            return Err(error);
        }
    };

    let deadline = Instant::now() + DSH_START_TIMEOUT;
    while Instant::now() < deadline {
        if native_dsh_process_matches(process)? && is_http_success(DSH_PORT, "/") {
            return Ok("服务已启动 · http://127.0.0.1:3080".to_owned());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("读取服务启动状态失败：{error}"))?
        {
            let _ = clear_native_dsh_pid();
            return Err(format!(
                "服务启动进程提前退出：{status}{}",
                dsh_start_log_diagnostic()
            ));
        }
        thread::sleep(Duration::from_millis(500));
    }

    let _ = terminate_native_dsh_process(pid);
    let _ = clear_native_dsh_pid();
    Err(format!(
        "服务在 {} 秒内未启动{}",
        DSH_START_TIMEOUT.as_secs(),
        dsh_start_log_diagnostic()
    ))
}

fn restart_dsh() -> Result<String, String> {
    if is_native_dsh_running()? {
        stop_dsh()?;
    }
    start_dsh()
}

fn stop_dsh() -> Result<String, String> {
    let Some(process) = read_native_dsh_process()? else {
        return if is_dsh_running() {
            Err("端口 3080 上的服务不是由此程序启动，无法关闭。".to_owned())
        } else {
            Ok("服务当前未运行".to_owned())
        };
    };

    if !native_dsh_process_matches(process)? {
        return if is_dsh_running() {
            Err("端口 3080 上的服务无法确认归属，无法关闭。".to_owned())
        } else {
            clear_native_dsh_pid()?;
            Ok("服务当前未运行".to_owned())
        };
    }

    terminate_native_dsh_process(process.pid)?;
    let deadline = Instant::now() + DSH_STOP_TIMEOUT;
    while Instant::now() < deadline {
        if !native_dsh_process_matches(process)? || !is_dsh_running() {
            clear_native_dsh_pid()?;
            return Ok("服务已停止".to_owned());
        }
        thread::sleep(Duration::from_millis(300));
    }
    Err(format!(
        "服务未能在 {} 秒内停止",
        DSH_STOP_TIMEOUT.as_secs()
    ))
}

fn upgrade_dsh() -> Result<String, String> {
    upgrade_dsh_with_progress(&|_| {})
}

fn upgrade_dsh_with_progress(progress: &dyn Fn(&str)) -> Result<String, String> {
    progress("正在检查当前版本...");
    let before = native_dsh_version()?;
    progress("正在查询最新版本...");
    let latest = latest_dsh_version()?;
    if latest == before {
        return Ok(format!("已是最新版本 {before}"));
    }

    let stage = create_upgrade_stage()?;
    let preflight: Result<String, String> = (|| {
        progress("正在下载更新...");
        let (entry, candidate) = stage_latest_dsh(&stage)?;
        if candidate != latest {
            return Err(format!(
                "检测到版本已变化（{latest} → {candidate}），请重新检查更新"
            ));
        }
        progress("正在验证更新...");
        preflight_staged_dsh(&entry)?;
        Ok(candidate)
    })();
    let cleanup = cleanup_upgrade_stage(&stage);

    let candidate = match preflight {
        Ok(candidate) => {
            if let Err(error) = cleanup {
                return Err(format!(
                    "更新验证已完成，但无法清理临时文件；当前版本未改变：{error}"
                ));
            }
            candidate
        }
        Err(error) => {
            let cleanup_detail = cleanup
                .err()
                .map(|cleanup_error| format!("；暂存目录清理失败：{cleanup_error}"))
                .unwrap_or_default();
            return Err(format!(
                "更新验证失败，当前服务未被停止或替换：{error}{cleanup_detail}"
            ));
        }
    };

    let was_running = is_native_dsh_running()?;
    if was_running {
        progress("正在停止服务...");
        stop_dsh()?;
    }

    progress("正在安装更新...");
    let upgrade = install_global_dsh(&format!("{DSH_PACKAGE}@latest")).and_then(|_| {
        let after = native_dsh_version()?;
        if after != candidate {
            return Err(format!(
                "全局安装后的版本为 {after}，与已预检版本 {candidate} 不一致"
            ));
        }
        if was_running {
            progress("正在启动服务...");
            start_dsh()?;
        }
        Ok(after)
    });

    match upgrade {
        Ok(after) => Ok(format!("更新完成：{before} → {after}")),
        Err(error) => {
            progress("正在恢复原版本...");
            let rollback = restore_global_dsh(&before);
            match rollback {
                Ok(()) if was_running => match start_dsh() {
                    Ok(_) => Err(format!("更新失败：{error}。已恢复至 {before} 并启动服务")),
                    Err(restart_error) => Err(format!(
                        "更新失败：{error}。已恢复至 {before}，但服务启动失败：{restart_error}"
                    )),
                },
                Ok(()) => Err(format!("更新失败：{error}。已恢复至 {before}")),
                Err(rollback_error) => Err(format!(
                    "更新失败：{error}；恢复至 {before} 也失败：{rollback_error}"
                )),
            }
        }
    }
}

fn native_dsh_command() -> Result<Command, String> {
    let entry = native_dsh_entry()?;
    native_dsh_command_for_entry(&entry)
}

fn native_dsh_entry() -> Result<PathBuf, String> {
    required_file(
        native_npm_prefix()?
            .join("node_modules")
            .join("@deepseek-ai")
            .join("dsh")
            .join("lib")
            .join("bin.js"),
        "DSH",
    )
}

fn native_dsh_command_for_entry(entry: &Path) -> Result<Command, String> {
    let mut command = hidden_command(native_node_executable()?);
    configure_native_environment(&mut command)?;
    command.arg(entry);
    Ok(command)
}

fn native_npm_command() -> Result<Command, String> {
    let npm = required_file(native_runtime_root()?.join("node").join("npm.cmd"), "npm")?;
    let mut command = hidden_command(npm);
    configure_native_environment(&mut command)?;
    Ok(command)
}

fn native_dsh_version() -> Result<String, String> {
    let entry = native_dsh_entry()?;
    native_dsh_version_for_entry(&entry)
}

fn latest_dsh_version() -> Result<String, String> {
    latest_dsh_version_from_registry().or_else(|_| latest_dsh_version_from_npm())
}

fn latest_dsh_version_from_registry() -> Result<String, String> {
    let mut command = hidden_command(native_node_executable()?);
    configure_native_environment(&mut command)?;
    command.args(["-e", DSH_LATEST_VERSION_SCRIPT, DSH_LATEST_REGISTRY_URL]);
    let output = run_native_command(&mut command, "查询最新版本")?;
    parse_dsh_version(&output).ok_or_else(|| "最新版本查询未返回有效版本号".to_owned())
}

fn latest_dsh_version_from_npm() -> Result<String, String> {
    let package = format!("{DSH_PACKAGE}@latest");
    let mut command = native_npm_command()?;
    command.args([
        "view",
        &package,
        "version",
        "--fetch-timeout=10000",
        "--fetch-retries=0",
        "--loglevel=error",
    ]);
    let output = run_native_command(&mut command, "查询最新版本")?;
    parse_dsh_version(&output).ok_or_else(|| "最新版本查询未返回有效版本号".to_owned())
}

fn native_dsh_version_for_entry(entry: &Path) -> Result<String, String> {
    let package_root = entry
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "无法定位 DSH 程序目录".to_owned())?;
    let package = required_file(package_root.join("package.json"), "DSH")?;
    let contents =
        fs::read_to_string(&package).map_err(|error| format!("无法读取 DSH 版本：{error}"))?;
    parse_package_version(&contents).ok_or_else(|| "DSH 未返回版本号".to_owned())
}

fn parse_dsh_version(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .rev()
        .find(|line| is_safe_dsh_version(line))
        .map(str::to_owned)
}

fn parse_package_version(package_json: &str) -> Option<String> {
    package_json.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        if key.trim() != "\"version\"" {
            return None;
        }
        let version = value.trim().trim_end_matches(',').trim_matches('"');
        is_safe_dsh_version(version).then(|| version.to_owned())
    })
}

fn create_upgrade_stage() -> Result<PathBuf, String> {
    let root = native_runtime_root()?.join("upgrade-staging");
    fs::create_dir_all(&root).map_err(|error| format!("无法创建 DSH 升级暂存目录：{error}"))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("无法生成 DSH 升级暂存目录名称：{error}"))?
        .as_millis();
    let stage = root.join(format!("dsh-{nonce}-{}", std::process::id()));
    fs::create_dir(&stage).map_err(|error| format!("无法创建 DSH 升级暂存目录：{error}"))?;
    Ok(stage)
}

fn cleanup_upgrade_stage(stage: &Path) -> Result<(), String> {
    let root = native_runtime_root()?.join("upgrade-staging");
    if stage.parent() != Some(root.as_path()) {
        return Err("拒绝清理未由启动器创建的 DSH 升级暂存目录".to_owned());
    }
    fs::remove_dir_all(stage).map_err(|error| format!("无法清理 DSH 升级暂存目录：{error}"))
}

fn stage_latest_dsh(stage: &Path) -> Result<(PathBuf, String), String> {
    let stage_text = stage.to_string_lossy().into_owned();
    let latest_package = format!("{DSH_PACKAGE}@latest");

    let mut install = native_npm_command()?;
    install
        .env_remove("NPM_CONFIG_PREFIX")
        .current_dir(stage)
        .args([
            "install",
            "--prefix",
            &stage_text,
            "--no-package-lock",
            "--no-audit",
            "--no-fund",
            &latest_package,
        ]);
    run_native_update_command(&mut install, "下载更新")?;

    let mut rebuild = native_npm_command()?;
    rebuild
        .env_remove("NPM_CONFIG_PREFIX")
        .current_dir(stage)
        .args([
        "rebuild",
        "--prefix",
        &stage_text,
        "--allow-scripts=@deepseek-ai/dsh-subprocess-local,koffi,node-pty,@google/genai,protobufjs",
    ]);
    run_native_update_command(&mut rebuild, "准备更新")?;

    let entry = required_file(
        stage
            .join("node_modules")
            .join("@deepseek-ai")
            .join("dsh")
            .join("lib")
            .join("bin.js"),
        "待更新的 DSH",
    )?;
    let version = native_dsh_version_for_entry(&entry)?;
    Ok((entry, version))
}

fn preflight_staged_dsh(entry: &Path) -> Result<(), String> {
    if is_port_listening(UPGRADE_PREFLIGHT_PORT) {
        return Err(format!(
            "升级预检端口 {UPGRADE_PREFLIGHT_PORT} 已被占用；请先释放该端口"
        ));
    }

    let stdout = open_native_dsh_log(DSH_PREFLIGHT_STDOUT_LOG_FILE)?;
    let stderr = open_native_dsh_log(DSH_PREFLIGHT_STDERR_LOG_FILE)?;
    let mut command = native_dsh_command_for_entry(entry)?;
    command
        .args(["web", "--no-open", "--host", "127.0.0.1", "--port", "3081"])
        .stdout(stdout)
        .stderr(stderr);
    let mut child = command
        .spawn()
        .map_err(|error| format!("启动更新验证进程失败：{error}"))?;

    let result = wait_for_staged_dsh(&mut child);
    let cleanup = stop_temporary_dsh(&mut child);
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(cleanup_error)) => Err(format!(
            "更新已通过验证，但无法关闭验证进程：{cleanup_error}"
        )),
        (Err(error), Ok(())) => Err(error),
        (Err(error), Err(cleanup_error)) => {
            Err(format!("{error}；预检进程清理失败：{cleanup_error}"))
        }
    }
}

fn wait_for_staged_dsh(child: &mut std::process::Child) -> Result<(), String> {
    let deadline = Instant::now() + DSH_START_TIMEOUT;
    while Instant::now() < deadline {
        if is_http_success(UPGRADE_PREFLIGHT_PORT, "/")
            && is_http_success(UPGRADE_PREFLIGHT_PORT, QUOTA_CONFIG_PATH)
        {
            return Ok(());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("读取更新验证进程状态失败：{error}"))?
        {
            return Err(format!(
                "更新验证进程提前退出：{status}{}",
                dsh_preflight_log_diagnostic()
            ));
        }
        thread::sleep(Duration::from_millis(500));
    }
    Err(format!(
        "待更新 DSH 在 {} 秒内未通过验证{}",
        DSH_START_TIMEOUT.as_secs(),
        dsh_preflight_log_diagnostic()
    ))
}

fn stop_temporary_dsh(child: &mut std::process::Child) -> Result<(), String> {
    if child
        .try_wait()
        .map_err(|error| format!("读取更新验证进程状态失败：{error}"))?
        .is_none()
    {
        terminate_process_tree(child.id(), "更新验证进程")?;
        let _ = child.wait();
    }
    Ok(())
}

fn install_global_dsh(package_spec: &str) -> Result<(), String> {
    let mut install = native_npm_command()?;
    install.args([
        "install",
        "--global",
        "--no-audit",
        "--no-fund",
        package_spec,
    ]);
    run_native_update_command(&mut install, "安装更新")?;

    let mut rebuild = native_npm_command()?;
    rebuild.args([
        "rebuild",
        "--global",
        "--allow-scripts=@deepseek-ai/dsh-subprocess-local,koffi,node-pty,@google/genai,protobufjs",
    ]);
    run_native_update_command(&mut rebuild, "完成更新")?;
    Ok(())
}

fn restore_global_dsh(version: &str) -> Result<(), String> {
    if !is_safe_dsh_version(version) {
        return Err("已安装的 DSH 版本号格式异常，无法自动恢复".to_owned());
    }
    let package = format!("{DSH_PACKAGE}@{version}");
    install_global_dsh(&package)?;
    let restored = native_dsh_version()?;
    if restored == version {
        Ok(())
    } else {
        Err(format!("回滚后检测到版本 {restored}，预期为 {version}"))
    }
}

fn is_safe_dsh_version(version: &str) -> bool {
    !version.is_empty()
        && version.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+')
        })
}

fn native_runtime_root() -> Result<PathBuf, String> {
    let local_app_data = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "未找到运行时目录".to_owned())?;
    Ok(local_app_data.join(RUNTIME_DIRECTORY))
}

fn native_npm_prefix() -> Result<PathBuf, String> {
    let local_app_data = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "未找到 npm 目录".to_owned())?;
    Ok(local_app_data.join(NPM_GLOBAL_DIRECTORY))
}

fn native_dsh_home() -> Result<PathBuf, String> {
    env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|user_profile| user_profile.join(".dsh"))
        .ok_or_else(|| "未找到 DSH 配置目录".to_owned())
}

fn native_dsh_log_path(file_name: &str) -> Result<PathBuf, String> {
    Ok(native_dsh_home()?.join(DSH_LOG_DIRECTORY).join(file_name))
}

fn open_native_dsh_log(file_name: &str) -> Result<Stdio, String> {
    let path = native_dsh_log_path(file_name)?;
    let parent = path
        .parent()
        .ok_or_else(|| "无法定位 DSH 启动日志目录".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| format!("无法创建 DSH 启动日志目录：{error}"))?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("无法写入 DSH 启动日志：{error}"))?;
    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    writeln!(file, "\n===== DSH Launcher {file_name} {started_at} =====")
        .map_err(|error| format!("无法写入 DSH 启动日志：{error}"))?;
    Ok(Stdio::from(file))
}

fn dsh_start_log_diagnostic() -> String {
    dsh_log_diagnostic(DSH_STDERR_LOG_FILE, "启动日志")
}

fn dsh_preflight_log_diagnostic() -> String {
    dsh_log_diagnostic(DSH_PREFLIGHT_STDERR_LOG_FILE, "预检日志")
}

fn dsh_log_diagnostic(file_name: &str, label: &str) -> String {
    match native_dsh_log_path(file_name) {
        Ok(path) => match read_log_tail(&path, DSH_LOG_TAIL_BYTES) {
            Some(tail) if !tail.is_empty() => format!(
                "。{label}：{}。末尾输出：{}",
                path.display(),
                truncate(&tail, 700)
            ),
            _ => format!("。{label}：{}", path.display()),
        },
        Err(_) => String::new(),
    }
}

fn read_log_tail(path: &Path, max_bytes: u64) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(max_bytes)))
        .ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .rev()
        .take(8)
        .collect();
    lines.reverse();
    (!lines.is_empty()).then(|| lines.join(" "))
}

fn native_node_executable() -> Result<PathBuf, String> {
    required_file(
        native_runtime_root()?.join("node").join("node.exe"),
        "Node 运行时",
    )
}

fn required_file(path: PathBuf, label: &str) -> Result<PathBuf, String> {
    if path.is_file() {
        Ok(path)
    } else {
        Err(format!("未找到{label}：{}", path.display()))
    }
}

fn configure_native_environment(command: &mut Command) -> Result<(), String> {
    let runtime_root = native_runtime_root()?;
    let prefix = native_npm_prefix()?;
    let dsh_home = native_dsh_home()?;
    let user_profile = dsh_home
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "无法定位 DSH 配置所在的用户目录".to_owned())?;
    let existing_path = env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![
        runtime_root.join("node"),
        prefix.clone(),
        runtime_root.join("pwsh"),
    ];
    paths.extend(env::split_paths(&existing_path));
    let path =
        env::join_paths(paths).map_err(|error| format!("无法准备 DSH 的运行环境：{error}"))?;

    command
        .env("PATH", path)
        .env("NPM_CONFIG_PREFIX", &prefix)
        .env("DSH_HOME", dsh_home)
        .current_dir(user_profile);
    Ok(())
}

fn native_dsh_pid_path() -> Result<PathBuf, String> {
    Ok(native_runtime_root()?.join(NATIVE_DSH_PID_FILE))
}

fn write_native_dsh_process(pid: u32) -> Result<NativeDshProcess, String> {
    let Some(started_at) = native_process_started_at(pid)? else {
        return Err("服务启动进程在记录前已退出".to_owned());
    };
    let process = NativeDshProcess { pid, started_at };
    let path = native_dsh_pid_path()?;
    fs::write(&path, format!("{}:{}\n", process.pid, process.started_at))
        .map_err(|error| format!("无法记录服务进程：{error}"))?;
    Ok(process)
}

fn read_native_dsh_process() -> Result<Option<NativeDshProcess>, String> {
    let path = native_dsh_pid_path()?;
    match fs::read_to_string(&path) {
        Ok(value) => Ok(parse_native_dsh_process(&value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("无法读取服务进程记录：{error}")),
    }
}

fn parse_native_dsh_process(value: &str) -> Option<NativeDshProcess> {
    let (pid, started_at) = value.trim().split_once(':')?;
    let pid = pid.parse::<u32>().ok().filter(|pid| *pid != 0)?;
    let started_at = started_at.parse::<u64>().ok().filter(|value| *value != 0)?;
    Some(NativeDshProcess { pid, started_at })
}

fn clear_native_dsh_pid() -> Result<(), String> {
    let path = native_dsh_pid_path()?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法清除服务进程记录：{error}")),
    }
}

fn is_native_dsh_running() -> Result<bool, String> {
    let Some(process) = read_native_dsh_process()? else {
        return Ok(false);
    };
    Ok(native_dsh_process_matches(process)? && is_http_success(DSH_PORT, "/"))
}

fn native_dsh_process_matches(process: NativeDshProcess) -> Result<bool, String> {
    Ok(native_process_started_at(process.pid)? == Some(process.started_at))
}

fn native_process_started_at(pid: u32) -> Result<Option<u64>, String> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return Ok(None);
    }

    let mut exit_code = 0;
    let exit_code_result = unsafe { GetExitCodeProcess(handle, &mut exit_code) };
    if exit_code_result == 0 {
        unsafe {
            CloseHandle(handle);
        }
        return Err(format!("无法读取服务进程状态：{}", unsafe {
            GetLastError()
        }));
    }
    if exit_code != PROCESS_STILL_ACTIVE {
        unsafe {
            CloseHandle(handle);
        }
        return Ok(None);
    }

    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    let times_result =
        unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) };
    unsafe {
        CloseHandle(handle);
    }
    if times_result == 0 {
        return Err(format!("无法读取服务进程创建时间：{}", unsafe {
            GetLastError()
        }));
    }
    Ok(Some(
        (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime),
    ))
}

fn terminate_native_dsh_process(pid: u32) -> Result<(), String> {
    terminate_process_tree(pid, "服务进程")
}

fn terminate_process_tree(pid: u32, label: &str) -> Result<(), String> {
    let pid = pid.to_string();
    let output = hidden_command("taskkill.exe")
        .args(["/PID", &pid, "/T", "/F"])
        .output()
        .map_err(|error| format!("无法关闭{label}：{error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        Err(format!("关闭{label}失败：{}", truncate(detail, 300)))
    }
}

fn run_native_command(command: &mut Command, description: &str) -> Result<String, String> {
    let output = command
        .output()
        .map_err(|error| format!("{description}失败：{error}"))?;
    command_output_result(output, description)
}

fn run_native_update_command(command: &mut Command, description: &str) -> Result<String, String> {
    run_native_update_command_with_timeout(command, description, DSH_UPDATE_COMMAND_TIMEOUT)
}

fn run_native_update_command_with_timeout(
    command: &mut Command,
    description: &str,
    timeout: Duration,
) -> Result<String, String> {
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| format!("{description}失败：{error}"))?;
    let process_id = child.id();
    let timed_out = Arc::new(AtomicBool::new(false));
    let timed_out_for_watchdog = Arc::clone(&timed_out);
    let label = description.to_owned();
    let (finished_tx, finished_rx) = mpsc::channel();
    let watchdog = thread::spawn(move || {
        if finished_rx.recv_timeout(timeout).is_err() {
            timed_out_for_watchdog.store(true, Ordering::Release);
            let _ = terminate_process_tree(process_id, &label);
        }
    });

    let status = child.wait();
    let _ = finished_tx.send(());
    let _ = watchdog.join();
    let status = status.map_err(|error| format!("读取{description}结果失败：{error}"))?;

    if timed_out.load(Ordering::Acquire) {
        return Err(format!(
            "{description}超过 {} 秒未完成，已停止。请检查网络或代理后重试。",
            timeout.as_secs()
        ));
    }
    if status.success() {
        Ok(String::new())
    } else {
        Err(format!("{description}失败，退出代码：{status}"))
    }
}

fn command_output_result(output: Output, description: &str) -> Result<String, String> {
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        Err(format!("{description}失败：{}", truncate(detail, 300)))
    }
}

fn http_status(port: u16, path: &str) -> Result<u16, String> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))
        .map_err(|error| format!("无法连接 127.0.0.1:{port}{path}：{error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("无法设置 DSH 健康检查读取超时：{error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("无法设置 DSH 健康检查写入超时：{error}"))?;
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("无法发送 DSH 健康检查请求：{error}"))?;
    let mut response = [0u8; 512];
    let length = stream
        .read(&mut response)
        .map_err(|error| format!("无法读取 DSH 健康检查响应：{error}"))?;
    let response_text = String::from_utf8_lossy(&response[..length]);
    let status_line = response_text
        .lines()
        .next()
        .ok_or_else(|| "DSH 健康检查未返回 HTTP 状态行".to_owned())?;
    status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| format!("无法解析 DSH 健康检查状态：{status_line}"))?
        .parse::<u16>()
        .map_err(|error| format!("无法解析 DSH 健康检查状态：{error}"))
}

fn is_http_success(port: u16, path: &str) -> bool {
    http_status(port, path)
        .map(is_successful_http_status)
        .unwrap_or(false)
}

fn is_successful_http_status(status: u16) -> bool {
    (200..400).contains(&status)
}

fn is_port_listening(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&address, Duration::from_millis(250)).is_ok()
}

fn is_dsh_running() -> bool {
    is_port_listening(DSH_PORT)
}

fn dsh_web_health_status() -> String {
    match http_status(DSH_PORT, "/") {
        Ok(status) if is_successful_http_status(status) => {
            "服务运行中 · http://127.0.0.1:3080".to_owned()
        }
        Ok(status) => format!("服务响应异常 · HTTP {status}"),
        Err(_) if is_dsh_running() => "端口 3080 被其他服务占用".to_owned(),
        Err(_) => "服务未启动".to_owned(),
    }
}

fn tray_icon_running_state(status: &str) -> Option<bool> {
    if status.starts_with("服务运行中")
        || status.starts_with("服务已启动")
        || status.starts_with("服务已在运行")
    {
        Some(true)
    } else if status.starts_with("服务响应异常")
        || status.starts_with("端口 3080 被其他服务占用")
        || status.starts_with("服务未启动")
        || status.starts_with("服务已停止")
        || status.starts_with("服务当前未运行")
    {
        Some(false)
    } else {
        None
    }
}

fn open_web_ui() -> Result<String, String> {
    let status = http_status(DSH_PORT, "/")?;
    if !is_successful_http_status(status) {
        return Err(format!("服务页面当前不可用（HTTP {status}）"));
    }
    let url = to_wide(WEB_URL);
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            std::ptr::null(),
            url.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOW,
        )
    };
    if (result as usize) <= 32 {
        Err(format!("无法打开服务页面，错误码：{}", result as usize))
    } else {
        Ok("已打开服务页面".to_owned())
    }
}

fn hidden_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW_FLAG);
    command.stdin(Stdio::null());
    command
}

fn run_app() -> Result<(), String> {
    let hinstance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let hicon = unsafe { load_app_icon(hinstance, ICON_RESOURCE_ID) };
    let gray_hicon = unsafe { load_app_icon(hinstance, GRAYSCALE_ICON_RESOURCE_ID) };
    if hicon.is_null() || gray_hicon.is_null() {
        return Err("无法加载应用图标".to_owned());
    }

    let background_brush = unsafe { CreateSolidBrush(COLOR_BACKGROUND) };
    let title_font = unsafe { create_ui_font(-27, FW_SEMIBOLD as i32) };
    let body_font = unsafe { create_ui_font(-16, FW_NORMAL as i32) };
    let small_font = unsafe { create_ui_font(-14, FW_NORMAL as i32) };
    let button_font = unsafe { create_ui_font(-16, FW_SEMIBOLD as i32) };
    if background_brush.is_null()
        || title_font.is_null()
        || body_font.is_null()
        || small_font.is_null()
        || button_font.is_null()
    {
        unsafe {
            delete_ui_resources(
                background_brush as usize,
                title_font as usize,
                body_font as usize,
                small_font as usize,
                button_font as usize,
            );
        }
        return Err("创建界面资源失败".to_owned());
    }

    let class_name = to_wide(WINDOW_CLASS);
    let window_title = to_wide(WINDOW_TITLE);
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfnWndProc: Some(window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: hicon,
        hCursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
        hbrBackground: background_brush,
        lpszMenuName: std::ptr::null(),
        lpszClassName: class_name.as_ptr(),
        hIconSm: hicon,
    };

    if unsafe { RegisterClassExW(&class) } == 0 {
        unsafe {
            delete_ui_resources(
                background_brush as usize,
                title_font as usize,
                body_font as usize,
                small_font as usize,
                button_font as usize,
            );
        }
        return Err("注册托盘窗口失败".to_owned());
    }

    let initial_status = dsh_web_health_status();
    let initial_tray_icon = if tray_icon_running_state(&initial_status) == Some(true) {
        hicon
    } else {
        gray_hicon
    };
    let shared = Arc::new(AppState {
        hicon: hicon as usize,
        gray_hicon: gray_hicon as usize,
        tray_hicon: AtomicUsize::new(initial_tray_icon as usize),
        background_brush: background_brush as usize,
        title_font: title_font as usize,
        body_font: body_font as usize,
        small_font: small_font as usize,
        button_font: button_font as usize,
        status_hwnd: AtomicUsize::new(0),
        tray_added: AtomicBool::new(false),
        busy: AtomicBool::new(false),
        health_checking: AtomicBool::new(false),
        hover_levels: std::array::from_fn(|_| AtomicUsize::new(0)),
        messages: Mutex::new(VecDeque::new()),
        last_health: Mutex::new(initial_status.clone()),
    });
    let state_ptr = Box::into_raw(Box::new(Arc::clone(&shared)));
    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let x = ((screen_width - WINDOW_WIDTH) / 2).max(0);
    let y = ((screen_height - WINDOW_HEIGHT) / 2).max(0);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_APPWINDOW,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            x,
            y,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            state_ptr.cast::<c_void>(),
        )
    };
    if hwnd.is_null() {
        unsafe {
            drop(Box::from_raw(state_ptr));
            delete_ui_resources(
                background_brush as usize,
                title_font as usize,
                body_font as usize,
                small_font as usize,
                button_font as usize,
            );
        }
        return Err("创建主窗口失败".to_owned());
    }
    unsafe {
        let dark_mode: i32 = 0;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
            (&dark_mode as *const i32).cast::<c_void>(),
            std::mem::size_of::<i32>() as u32,
        );
        let backdrop = DWMSBT_MAINWINDOW;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE as u32,
            (&backdrop as *const i32).cast::<c_void>(),
            std::mem::size_of::<i32>() as u32,
        );
        let corner = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&corner as *const i32).cast::<c_void>(),
            std::mem::size_of::<i32>() as u32,
        );
        for (attribute, color) in [
            (DWMWA_CAPTION_COLOR, COLOR_BACKGROUND_TOP),
            (DWMWA_BORDER_COLOR, COLOR_BORDER),
            (DWMWA_TEXT_COLOR, COLOR_TEXT),
        ] {
            let _ = DwmSetWindowAttribute(
                hwnd,
                attribute as u32,
                (&color as *const u32).cast::<c_void>(),
                std::mem::size_of::<u32>() as u32,
            );
        }
        SetWindowTextW(hwnd, window_title.as_ptr());
    }

    if let Err(error) = unsafe { create_controls(hwnd, hinstance, &shared) } {
        unsafe {
            DestroyWindow(hwnd);
        }
        return Err(error);
    }
    unsafe {
        SetTimer(hwnd, HOVER_TIMER_ID, HOVER_TIMER_INTERVAL_MS, None);
        SetTimer(hwnd, HEALTH_TIMER_ID, HEALTH_TIMER_INTERVAL_MS, None);
    }

    let tray_result = unsafe {
        add_tray_icon(
            hwnd,
            shared.tray_hicon.load(Ordering::Acquire) as HICON,
            "DSH Launcher - 就绪",
        )
    };
    if tray_result == 0 {
        eprintln!(
            "添加托盘图标失败: last_error={}, hwnd=0x{:x}, hicon=0x{:x}, notify_size={}",
            unsafe { GetLastError() },
            hwnd as usize,
            hicon as usize,
            std::mem::size_of::<NOTIFYICONDATAW>(),
        );
        unsafe {
            SetTimer(hwnd, TRAY_RETRY_TIMER_ID, TRAY_RETRY_INTERVAL_MS, None);
        }
    } else {
        shared.tray_added.store(true, Ordering::Release);
    }

    unsafe {
        set_status_text(&shared, &initial_status);
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
    }

    let mut message = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
        if result <= 0 {
            break;
        }
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

fn show_error_box(error: &str) {
    let title = to_wide("DSH 启动器错误");
    let message = to_wide(error);
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

unsafe fn load_app_icon(
    hinstance: windows_sys::Win32::Foundation::HINSTANCE,
    resource_id: usize,
) -> HICON {
    let embedded = LoadIconW(hinstance, resource_id as *const u16);
    if embedded.is_null() {
        LoadIconW(std::ptr::null_mut(), IDI_APPLICATION)
    } else {
        embedded
    }
}

unsafe fn create_ui_font(height: i32, weight: i32) -> windows_sys::Win32::Graphics::Gdi::HFONT {
    let face_name = to_wide("Segoe UI");
    CreateFontW(
        height,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        (DEFAULT_PITCH | FF_DONTCARE) as u32,
        face_name.as_ptr(),
    )
}

unsafe fn delete_ui_resources(
    background_brush: usize,
    title_font: usize,
    body_font: usize,
    small_font: usize,
    button_font: usize,
) {
    for handle in [
        background_brush,
        title_font,
        body_font,
        small_font,
        button_font,
    ] {
        if handle != 0 {
            DeleteObject(handle as *mut c_void);
        }
    }
}

unsafe fn create_controls(
    hwnd: HWND,
    hinstance: windows_sys::Win32::Foundation::HINSTANCE,
    state: &AppState,
) -> Result<(), String> {
    let controls = [
        create_control(
            hwnd,
            hinstance,
            "STATIC",
            "DSH 服务管理",
            WS_CHILD | WS_VISIBLE | SS_CENTERIMAGE,
            78,
            18,
            494,
            36,
            ID_TITLE,
        ),
        create_control(
            hwnd,
            hinstance,
            "STATIC",
            "启动、停止与更新",
            WS_CHILD | WS_VISIBLE | SS_CENTERIMAGE,
            78,
            52,
            494,
            22,
            ID_SUBTITLE,
        ),
        create_control(
            hwnd,
            hinstance,
            "STATIC",
            "正在检测 DSH 状态...",
            WS_CHILD | WS_VISIBLE | SS_OWNERDRAW,
            32,
            92,
            540,
            62,
            ID_STATUS,
        ),
        create_control(
            hwnd,
            hinstance,
            "STATIC",
            "快速操作",
            WS_CHILD | WS_VISIBLE | SS_CENTERIMAGE,
            32,
            174,
            540,
            24,
            ID_SECTION,
        ),
        create_control(
            hwnd,
            hinstance,
            "BUTTON",
            "启动服务",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BUTTON_STYLE,
            32,
            208,
            262,
            58,
            CMD_START,
        ),
        create_control(
            hwnd,
            hinstance,
            "BUTTON",
            "停止服务",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BUTTON_STYLE,
            310,
            208,
            262,
            58,
            CMD_STOP,
        ),
        create_control(
            hwnd,
            hinstance,
            "BUTTON",
            "重启服务",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BUTTON_STYLE,
            32,
            280,
            262,
            58,
            CMD_RESTART,
        ),
        create_control(
            hwnd,
            hinstance,
            "BUTTON",
            "检查更新",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BUTTON_STYLE,
            310,
            280,
            262,
            58,
            CMD_UPGRADE,
        ),
        create_control(
            hwnd,
            hinstance,
            "STATIC",
            "关闭窗口后仍在后台运行",
            WS_CHILD | WS_VISIBLE | SS_CENTERIMAGE,
            32,
            365,
            380,
            34,
            ID_FOOTER,
        ),
        create_control(
            hwnd,
            hinstance,
            "BUTTON",
            "退出程序",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BUTTON_STYLE,
            466,
            365,
            106,
            34,
            CMD_EXIT,
        ),
    ];

    if controls.iter().any(|control| control.is_null()) {
        return Err("创建窗口控件失败".to_owned());
    }
    SendMessageW(controls[0], WM_SETFONT, state.title_font, 1);
    SendMessageW(controls[1], WM_SETFONT, state.body_font, 1);
    SendMessageW(controls[2], WM_SETFONT, state.body_font, 1);
    SendMessageW(controls[3], WM_SETFONT, state.button_font, 1);
    for control in &controls[4..8] {
        SendMessageW(*control, WM_SETFONT, state.button_font, 1);
    }
    SendMessageW(controls[8], WM_SETFONT, state.small_font, 1);
    SendMessageW(controls[9], WM_SETFONT, state.small_font, 1);
    state
        .status_hwnd
        .store(controls[2] as usize, Ordering::Release);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
unsafe fn create_control(
    parent: HWND,
    hinstance: windows_sys::Win32::Foundation::HINSTANCE,
    class_name: &str,
    text: &str,
    style: u32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    id: u32,
) -> HWND {
    let class_name = to_wide(class_name);
    let text = to_wide(text);
    let menu = if id == 0 {
        std::ptr::null_mut()
    } else {
        id as usize as *mut c_void
    };
    let extended_style = if class_name == to_wide("STATIC") {
        WS_EX_TRANSPARENT
    } else {
        0
    };
    CreateWindowExW(
        extended_style,
        class_name.as_ptr(),
        text.as_ptr(),
        style,
        x,
        y,
        width,
        height,
        parent,
        menu,
        hinstance,
        std::ptr::null(),
    )
}

unsafe fn set_status_text(state: &AppState, message: &str) {
    let status_hwnd = state.status_hwnd.load(Ordering::Acquire) as HWND;
    if !status_hwnd.is_null() {
        let message = to_wide(message);
        SetWindowTextW(status_hwnd, message.as_ptr());
        InvalidateRect(status_hwnd, std::ptr::null(), 1);
    }
}

unsafe fn set_action_buttons_enabled(hwnd: HWND, enabled: bool) {
    for id in [CMD_START, CMD_STOP, CMD_RESTART, CMD_UPGRADE] {
        let control = GetDlgItem(hwnd, id as i32);
        if !control.is_null() {
            EnableWindow(control, enabled as i32);
            InvalidateRect(control, std::ptr::null(), 1);
        }
    }
}

unsafe fn paint_window(hwnd: HWND, state: &AppState) {
    let mut paint = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut paint);
    if hdc.is_null() {
        return;
    }

    let mut client = RECT::default();
    GetClientRect(hwnd, &mut client);
    fill_gradient(hdc, client, COLOR_BACKGROUND_TOP, COLOR_BACKGROUND_BOTTOM);

    draw_ellipse(
        hdc,
        RECT {
            left: -110,
            top: -130,
            right: 220,
            bottom: 170,
        },
        rgb(218, 236, 255),
    );
    draw_ellipse(
        hdc,
        RECT {
            left: 440,
            top: -100,
            right: 720,
            bottom: 160,
        },
        rgb(237, 228, 255),
    );

    draw_round_rect(
        hdc,
        RECT {
            left: 33,
            top: 25,
            right: 67,
            bottom: 67,
        },
        rgb(154, 184, 220),
        rgb(154, 184, 220),
        11,
    );
    draw_round_rect(
        hdc,
        RECT {
            left: 32,
            top: 22,
            right: 64,
            bottom: 64,
        },
        COLOR_BLUE,
        rgb(75, 151, 214),
        11,
    );
    draw_text(
        hdc,
        state.button_font,
        COLOR_HIGHLIGHT,
        "D",
        RECT {
            left: 32,
            top: 22,
            right: 64,
            bottom: 64,
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    let divider = RECT {
        left: 32,
        top: 352,
        right: 572,
        bottom: 353,
    };
    fill_solid_rect(hdc, divider, rgb(207, 222, 240));
    EndPaint(hwnd, &paint);
}

unsafe fn draw_owner_item(state: &AppState, item: &DRAWITEMSTRUCT) {
    if item.CtlID == ID_STATUS {
        draw_status(state, item);
    } else {
        draw_button(state, item);
    }
}

unsafe fn draw_status(state: &AppState, item: &DRAWITEMSTRUCT) {
    let shadow = RECT {
        left: item.rcItem.left + 2,
        top: item.rcItem.top + 4,
        right: item.rcItem.right - 1,
        bottom: item.rcItem.bottom - 1,
    };
    draw_round_rect(item.hDC, shadow, COLOR_SHADOW, COLOR_SHADOW, 14);
    let card = RECT {
        left: item.rcItem.left + 1,
        top: item.rcItem.top + 1,
        right: item.rcItem.right - 1,
        bottom: item.rcItem.bottom - 4,
    };
    draw_round_rect(item.hDC, card, COLOR_SURFACE, COLOR_HIGHLIGHT, 14);
    draw_round_rect(
        item.hDC,
        inset_rect(card, 1),
        COLOR_SURFACE,
        COLOR_BORDER,
        13,
    );
    let text = read_control_text(item.hwndItem);
    let accent = status_accent(&text);

    let center_y = (card.top + card.bottom) / 2;
    let old_brush = SelectObject(item.hDC, GetStockObject(DC_BRUSH));
    let old_pen = SelectObject(item.hDC, GetStockObject(DC_PEN));
    SetDCBrushColor(item.hDC, accent);
    SetDCPenColor(item.hDC, accent);
    Ellipse(
        item.hDC,
        card.left + 20,
        center_y - 6,
        card.left + 32,
        center_y + 6,
    );
    SelectObject(item.hDC, old_pen);
    SelectObject(item.hDC, old_brush);

    draw_text(
        item.hDC,
        state.small_font,
        COLOR_MUTED,
        "服务状态",
        RECT {
            left: card.left + 48,
            top: card.top + 7,
            right: card.right - 18,
            bottom: card.top + 27,
        },
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
    draw_text(
        item.hDC,
        state.body_font,
        COLOR_TEXT,
        &text,
        RECT {
            left: card.left + 48,
            top: card.top + 27,
            right: card.right - 18,
            bottom: card.bottom - 5,
        },
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
}

unsafe fn draw_button(state: &AppState, item: &DRAWITEMSTRUCT) {
    let disabled = item.itemState & ODS_DISABLED != 0;
    let selected = item.itemState & ODS_SELECTED != 0;
    let focused = item.itemState & ODS_FOCUS != 0;
    let (title, detail, accent, glyph) = button_spec(item.CtlID);
    let hover_level = button_index(item.CtlID)
        .map(|index| state.hover_levels[index].load(Ordering::Acquire))
        .unwrap_or(0);
    let surface = if disabled {
        rgb(239, 244, 250)
    } else if selected {
        COLOR_SURFACE_PRESSED
    } else {
        blend_color(COLOR_SURFACE, COLOR_SURFACE_HOVER, hover_level, HOVER_STEPS)
    };
    let border = if disabled {
        COLOR_BORDER
    } else if focused {
        COLOR_BLUE
    } else {
        blend_color(COLOR_BORDER, accent, hover_level, HOVER_STEPS)
    };
    let lift = ((hover_level * 2) / HOVER_STEPS) as i32;
    let pressed_offset = if selected { 2 } else { 0 };
    let card = RECT {
        left: item.rcItem.left + 1,
        top: item.rcItem.top + 2 - lift + pressed_offset,
        right: item.rcItem.right - 1,
        bottom: item.rcItem.bottom - 2 - lift + pressed_offset,
    };
    let shadow = RECT {
        left: card.left + 1,
        top: card.top + 3,
        right: card.right + 1,
        bottom: card.bottom + 3,
    };
    draw_round_rect(
        item.hDC,
        shadow,
        blend_color(rgb(220, 231, 244), COLOR_SHADOW, hover_level, HOVER_STEPS),
        blend_color(rgb(220, 231, 244), COLOR_SHADOW, hover_level, HOVER_STEPS),
        12,
    );
    draw_round_rect(item.hDC, card, surface, border, 12);
    draw_round_rect(item.hDC, inset_rect(card, 1), surface, COLOR_HIGHLIGHT, 11);

    if item.CtlID == CMD_EXIT {
        draw_text(
            item.hDC,
            state.small_font,
            if disabled { COLOR_DISABLED } else { COLOR_TEXT },
            title,
            card,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        return;
    }

    let color = if disabled { COLOR_DISABLED } else { accent };
    let icon_rect = RECT {
        left: card.left + 17,
        top: card.top + 12,
        right: card.left + 47,
        bottom: card.top + 42,
    };
    draw_ellipse(
        item.hDC,
        icon_rect,
        if disabled {
            rgb(226, 233, 241)
        } else {
            button_tint(item.CtlID)
        },
    );
    draw_text(
        item.hDC,
        state.body_font,
        color,
        glyph,
        icon_rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    draw_text(
        item.hDC,
        state.button_font,
        if disabled { COLOR_DISABLED } else { COLOR_TEXT },
        title,
        RECT {
            left: card.left + 59,
            top: card.top + 6,
            right: card.right - 12,
            bottom: card.top + 31,
        },
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
    draw_text(
        item.hDC,
        state.small_font,
        if disabled {
            COLOR_DISABLED
        } else {
            COLOR_MUTED
        },
        detail,
        RECT {
            left: card.left + 59,
            top: card.top + 29,
            right: card.right - 12,
            bottom: card.bottom - 4,
        },
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
}

unsafe fn fill_gradient(hdc: *mut c_void, rect: RECT, top: u32, bottom: u32) {
    let vertices = [
        gradient_vertex(rect.left, rect.top, top),
        gradient_vertex(rect.right, rect.bottom, bottom),
    ];
    let mesh = GRADIENT_RECT {
        UpperLeft: 0,
        LowerRight: 1,
    };
    GradientFill(
        hdc,
        vertices.as_ptr(),
        vertices.len() as u32,
        (&mesh as *const GRADIENT_RECT).cast::<c_void>(),
        1,
        GRADIENT_FILL_RECT_V,
    );
}

fn gradient_vertex(x: i32, y: i32, color: u32) -> TRIVERTEX {
    TRIVERTEX {
        x,
        y,
        Red: ((color & 0xff) * 257) as u16,
        Green: (((color >> 8) & 0xff) * 257) as u16,
        Blue: (((color >> 16) & 0xff) * 257) as u16,
        Alpha: u16::MAX,
    }
}

unsafe fn fill_solid_rect(hdc: *mut c_void, rect: RECT, color: u32) {
    let brush = CreateSolidBrush(color);
    if !brush.is_null() {
        FillRect(hdc, &rect, brush);
        DeleteObject(brush as *mut c_void);
    }
}

unsafe fn draw_ellipse(hdc: *mut c_void, rect: RECT, color: u32) {
    let old_brush = SelectObject(hdc, GetStockObject(DC_BRUSH));
    let old_pen = SelectObject(hdc, GetStockObject(DC_PEN));
    SetDCBrushColor(hdc, color);
    SetDCPenColor(hdc, color);
    Ellipse(hdc, rect.left, rect.top, rect.right, rect.bottom);
    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
}

fn inset_rect(rect: RECT, amount: i32) -> RECT {
    RECT {
        left: rect.left + amount,
        top: rect.top + amount,
        right: rect.right - amount,
        bottom: rect.bottom - amount,
    }
}

unsafe fn draw_round_rect(hdc: *mut c_void, rect: RECT, fill: u32, border: u32, radius: i32) {
    let old_brush = SelectObject(hdc, GetStockObject(DC_BRUSH));
    let old_pen = SelectObject(hdc, GetStockObject(DC_PEN));
    SetDCBrushColor(hdc, fill);
    SetDCPenColor(hdc, border);
    RoundRect(
        hdc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        radius,
        radius,
    );
    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_brush);
}

unsafe fn draw_text(
    hdc: *mut c_void,
    font: usize,
    color: u32,
    text: &str,
    mut rect: RECT,
    format: u32,
) {
    let old_font = SelectObject(hdc, font as *mut c_void);
    SetBkMode(hdc, TRANSPARENT as i32);
    SetTextColor(hdc, color);
    let text = to_wide(text);
    DrawTextW(
        hdc,
        text.as_ptr(),
        (text.len() - 1) as i32,
        &mut rect,
        format,
    );
    SelectObject(hdc, old_font);
}

unsafe fn read_control_text(hwnd: HWND) -> String {
    let mut buffer = [0u16; 512];
    let length = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}

fn button_spec(id: u32) -> (&'static str, &'static str, u32, &'static str) {
    match id {
        CMD_START => ("启动服务", "开启 DSH", COLOR_BLUE, "▶"),
        CMD_STOP => ("停止服务", "关闭当前服务", COLOR_RED, "■"),
        CMD_RESTART => ("重启服务", "重新启动 DSH", COLOR_CYAN, "↻"),
        CMD_UPGRADE => ("检查更新", "下载并安装新版本", COLOR_PURPLE, "↑"),
        CMD_EXIT => ("退出程序", "", COLOR_BORDER, ""),
        _ => ("", "", COLOR_BORDER, ""),
    }
}

fn button_tint(id: u32) -> u32 {
    match id {
        CMD_START => rgb(222, 239, 255),
        CMD_STOP => rgb(255, 229, 235),
        CMD_RESTART => rgb(220, 244, 248),
        CMD_UPGRADE => rgb(239, 231, 255),
        _ => rgb(232, 238, 246),
    }
}

fn button_index(id: u32) -> Option<usize> {
    match id {
        CMD_START => Some(0),
        CMD_STOP => Some(1),
        CMD_RESTART => Some(2),
        CMD_UPGRADE => Some(3),
        CMD_EXIT => Some(4),
        _ => None,
    }
}

fn blend_color(from: u32, to: u32, step: usize, total: usize) -> u32 {
    let blend_channel = |shift: u32| {
        let start = ((from >> shift) & 0xff) as i32;
        let end = ((to >> shift) & 0xff) as i32;
        (start + (end - start) * step.min(total) as i32 / total.max(1) as i32) as u32
    };
    rgb(blend_channel(0), blend_channel(8), blend_channel(16))
}

unsafe fn animate_hover(hwnd: HWND, state: &AppState) {
    let mut point = POINT::default();
    let hovered_id = if GetCursorPos(&mut point) != 0 {
        let control = WindowFromPoint(point);
        let id = if control.is_null() || GetParent(control) != hwnd {
            0
        } else {
            GetDlgCtrlID(control) as u32
        };
        if button_index(id).is_some() {
            id
        } else {
            0
        }
    } else {
        0
    };

    for id in [CMD_START, CMD_STOP, CMD_RESTART, CMD_UPGRADE, CMD_EXIT] {
        let index = button_index(id).expect("button id must have a hover index");
        let current = state.hover_levels[index].load(Ordering::Acquire);
        let next = if id == hovered_id {
            (current + 1).min(HOVER_STEPS)
        } else {
            current.saturating_sub(1)
        };
        if current != next {
            state.hover_levels[index].store(next, Ordering::Release);
            let control = GetDlgItem(hwnd, id as i32);
            if !control.is_null() {
                InvalidateRect(control, std::ptr::null(), 1);
            }
        }
    }
}

fn status_accent(text: &str) -> u32 {
    if text.contains("正在运行") {
        COLOR_GREEN
    } else if text.contains("正在") {
        COLOR_AMBER
    } else if text.contains("失败")
        || text.contains("错误")
        || text.contains("无效")
        || text.contains("未找到")
        || text.contains("异常")
        || text.contains("占用")
        || text.contains("不可用")
        || text.contains("拒绝")
    {
        COLOR_RED
    } else if text.contains("关闭") || text.contains("未运行") {
        COLOR_DISABLED
    } else if text.contains("运行")
        || text.contains("启动")
        || text.contains("升级")
        || text.contains("最新")
        || text.contains("重启")
    {
        COLOR_GREEN
    } else {
        COLOR_BLUE
    }
}

unsafe fn show_main_window(hwnd: HWND) {
    ShowWindow(hwnd, SW_RESTORE);
    SetForegroundWindow(hwnd);
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    match message {
        WM_NCCREATE => {
            let create = lparam as *const CREATESTRUCTW;
            if !create.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*create).lpCreateParams as isize);
            }
            1
        }
        WM_COMMAND => {
            let command = (wparam & 0xffff) as u32;
            if command == CMD_EXIT {
                if let Some(state) = state_for(hwnd) {
                    if state.busy.load(Ordering::Acquire) {
                        push_status(hwnd, &state, "正在处理，请完成后再退出".to_owned());
                    } else {
                        DestroyWindow(hwnd);
                    }
                } else {
                    DestroyWindow(hwnd);
                }
            } else if command == CMD_SHOW {
                show_main_window(hwnd);
            } else if let Some(action) = action_from_command(command) {
                if let Some(state) = state_for(hwnd) {
                    spawn_action(hwnd, state, action);
                }
            }
            0
        }
        WM_PAINT => {
            if let Some(state) = state_for(hwnd) {
                paint_window(hwnd, &state);
                0
            } else {
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
        }
        WM_DRAWITEM => {
            let item = lparam as *const DRAWITEMSTRUCT;
            if !item.is_null() {
                if let Some(state) = state_for(hwnd) {
                    draw_owner_item(&state, &*item);
                    return 1;
                }
            }
            0
        }
        WM_CTLCOLORSTATIC => {
            if state_for(hwnd).is_some() {
                let hdc = wparam as *mut c_void;
                let control = lparam as HWND;
                let id = GetDlgCtrlID(control) as u32;
                SetBkMode(hdc, TRANSPARENT as i32);
                let color = if id == ID_TITLE || id == ID_SECTION {
                    COLOR_TEXT
                } else {
                    COLOR_MUTED
                };
                SetTextColor(hdc, color);
                GetStockObject(NULL_BRUSH) as isize
            } else {
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
        }
        STATUS_MESSAGE => {
            if let Some(state) = state_for(hwnd) {
                let latest = state
                    .messages
                    .lock()
                    .ok()
                    .and_then(|mut messages| messages.pop_back());
                if let Some(message) = latest {
                    set_status_text(&state, &message);
                    if let Some(running) = tray_icon_running_state(&message) {
                        let icon = if running {
                            state.hicon
                        } else {
                            state.gray_hicon
                        };
                        state.tray_hicon.store(icon, Ordering::Release);
                        let _ = update_tray_icon(hwnd, icon as HICON);
                    }
                    let _ = update_tray_tooltip(
                        hwnd,
                        state.tray_hicon.load(Ordering::Acquire) as HICON,
                        &message,
                    );
                    set_action_buttons_enabled(hwnd, !state.busy.load(Ordering::Acquire));
                }
            }
            0
        }
        WM_TIMER if wparam == TRAY_RETRY_TIMER_ID => {
            if let Some(state) = state_for(hwnd) {
                if !state.tray_added.load(Ordering::Acquire) {
                    let result = add_tray_icon(
                        hwnd,
                        state.tray_hicon.load(Ordering::Acquire) as HICON,
                        "DSH Launcher",
                    );
                    if result != 0 {
                        state.tray_added.store(true, Ordering::Release);
                        KillTimer(hwnd, TRAY_RETRY_TIMER_ID);
                    }
                }
            }
            0
        }
        WM_TIMER if wparam == HOVER_TIMER_ID => {
            if let Some(state) = state_for(hwnd) {
                animate_hover(hwnd, &state);
            }
            0
        }
        WM_TIMER if wparam == HEALTH_TIMER_ID => {
            if let Some(state) = state_for(hwnd) {
                schedule_health_check(hwnd, state);
            }
            0
        }
        TRAY_MESSAGE => {
            let event = tray_event(lparam);
            if tray_event_opens_window(event) {
                show_main_window(hwnd);
            } else if tray_event_opens_menu(event) {
                show_menu(hwnd);
            }
            0
        }
        SHOW_WINDOW_MESSAGE => {
            show_main_window(hwnd);
            0
        }
        WM_CLOSE => {
            if let Some(state) = state_for(hwnd) {
                if state.tray_added.load(Ordering::Acquire) {
                    ShowWindow(hwnd, SW_HIDE);
                } else if state.busy.load(Ordering::Acquire) {
                    push_status(hwnd, &state, "正在处理，请稍候".to_owned());
                } else {
                    DestroyWindow(hwnd);
                }
            }
            0
        }
        WM_DESTROY => {
            if let Some(state) = state_for(hwnd) {
                KillTimer(hwnd, TRAY_RETRY_TIMER_ID);
                KillTimer(hwnd, HOVER_TIMER_ID);
                KillTimer(hwnd, HEALTH_TIMER_ID);
                if state.tray_added.swap(false, Ordering::AcqRel) {
                    delete_tray_icon(hwnd);
                }
                state.busy.store(false, Ordering::Release);
                delete_ui_resources(
                    state.background_brush,
                    state.title_font,
                    state.body_font,
                    state.small_font,
                    state.button_font,
                );
            }
            PostQuitMessage(0);
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            if ptr != 0 {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(ptr as *mut Arc<AppState>));
            }
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn state_for(hwnd: HWND) -> Option<Arc<AppState>> {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    if ptr == 0 {
        None
    } else {
        Some((&*(ptr as *const Arc<AppState>)).clone())
    }
}

fn spawn_action(hwnd: HWND, state: Arc<AppState>, action: Action) {
    if state.busy.swap(true, Ordering::AcqRel) {
        push_status(hwnd, &state, "已有操作正在执行".to_owned());
        return;
    }
    unsafe {
        set_action_buttons_enabled(hwnd, false);
    }
    push_status(hwnd, &state, format!("正在{}...", action.label()));
    let hwnd_value = hwnd as usize;
    thread::spawn(move || {
        let result = match action {
            Action::Upgrade => upgrade_dsh_with_progress(&|message| {
                push_status(hwnd_value as HWND, &state, message.to_owned());
            }),
            _ => execute_action(action),
        };
        state.busy.store(false, Ordering::Release);
        let message = match result {
            Ok(message) => message,
            Err(error) => error,
        };
        push_status(hwnd_value as HWND, &state, message);
    });
}

fn push_status(hwnd: HWND, state: &AppState, message: String) {
    if let Ok(mut messages) = state.messages.lock() {
        messages.push_back(message);
        while messages.len() > 8 {
            messages.pop_front();
        }
    }
    unsafe {
        PostMessageW(hwnd, STATUS_MESSAGE, 0, 0);
    }
}

fn schedule_health_check(hwnd: HWND, state: Arc<AppState>) {
    if state.busy.load(Ordering::Acquire) || state.health_checking.swap(true, Ordering::AcqRel) {
        return;
    }
    let hwnd_value = hwnd as usize;
    thread::spawn(move || {
        let message = dsh_web_health_status();
        let changed = state
            .last_health
            .lock()
            .map(|mut last_health| {
                if *last_health == message {
                    false
                } else {
                    *last_health = message.clone();
                    true
                }
            })
            .unwrap_or(false);
        state.health_checking.store(false, Ordering::Release);
        if changed {
            push_status(hwnd_value as HWND, &state, message);
        }
    });
}

fn action_from_command(command: u32) -> Option<Action> {
    match command {
        CMD_START => Some(Action::Start),
        CMD_RESTART => Some(Action::Restart),
        CMD_STOP => Some(Action::Stop),
        CMD_UPGRADE => Some(Action::Upgrade),
        CMD_OPEN_WEB => Some(Action::OpenWeb),
        _ => None,
    }
}

fn tray_event(lparam: isize) -> u32 {
    (lparam as usize & 0xffff) as u32
}

fn tray_event_opens_window(event: u32) -> bool {
    matches!(
        event,
        WM_LBUTTONUP | WM_LBUTTONDBLCLK | NIN_SELECT | NIN_KEYSELECT
    )
}

fn tray_event_opens_menu(event: u32) -> bool {
    matches!(event, WM_RBUTTONUP | WM_CONTEXTMENU)
}

unsafe fn show_menu(hwnd: HWND) {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    let labels = [
        (CMD_SHOW, "打开面板"),
        (CMD_OPEN_WEB, "打开网页"),
        (CMD_START, "启动服务"),
        (CMD_RESTART, "重启服务"),
        (CMD_STOP, "停止服务"),
        (CMD_UPGRADE, "检查更新"),
        (CMD_EXIT, "退出"),
    ];
    let wide_labels: Vec<Vec<u16>> = labels.iter().map(|(_, label)| to_wide(label)).collect();
    for ((command, _), label) in labels.iter().zip(wide_labels.iter()) {
        if *command == CMD_START || *command == CMD_EXIT {
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        }
        AppendMenuW(menu, MF_STRING, *command as usize, label.as_ptr());
    }

    let mut point = POINT { x: 0, y: 0 };
    GetCursorPos(&mut point);
    SetForegroundWindow(hwnd);
    let selected = TrackPopupMenu(
        menu,
        TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_BOTTOMALIGN,
        point.x,
        point.y,
        0,
        hwnd,
        std::ptr::null(),
    );
    DestroyMenu(menu);
    PostMessageW(hwnd, WM_NULL, 0, 0);
    if selected != 0 {
        PostMessageW(hwnd, WM_COMMAND, selected as usize, 0);
    }
}

unsafe fn add_tray_icon(hwnd: HWND, hicon: HICON, tooltip: &str) -> i32 {
    let mut data = notify_data(hwnd, hicon);
    copy_wide(&mut data.szTip, tooltip);
    let result = Shell_NotifyIconW(NIM_ADD, &data);
    if result != 0 {
        let mut version = notify_data(hwnd, hicon);
        version.uFlags = NIF_MESSAGE;
        version.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &version);
    }
    result
}

unsafe fn update_tray_tooltip(hwnd: HWND, hicon: HICON, tooltip: &str) -> i32 {
    let mut data = notify_data(hwnd, hicon);
    data.uFlags = NIF_TIP;
    copy_wide(&mut data.szTip, tooltip);
    Shell_NotifyIconW(NIM_MODIFY, &data)
}

unsafe fn update_tray_icon(hwnd: HWND, hicon: HICON) -> i32 {
    let mut data = notify_data(hwnd, hicon);
    data.uFlags = NIF_ICON;
    Shell_NotifyIconW(NIM_MODIFY, &data)
}

unsafe fn delete_tray_icon(hwnd: HWND) {
    let data = notify_data(hwnd, std::ptr::null_mut());
    Shell_NotifyIconW(NIM_DELETE, &data);
}

fn notify_data(hwnd: HWND, hicon: HICON) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW::default();
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = 1;
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = TRAY_MESSAGE;
    data.hIcon = hicon;
    data
}

fn copy_wide(destination: &mut [u16], value: &str) {
    let encoded: Vec<u16> = value
        .encode_utf16()
        .take(destination.len().saturating_sub(1))
        .collect();
    destination.fill(0);
    destination[..encoded.len()].copy_from_slice(&encoded);
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_names_cover_launcher_operations() {
        assert!(Action::from_name("start").is_some());
        assert!(Action::from_name("restart").is_some());
        assert!(Action::from_name("stop").is_some());
        assert!(Action::from_name("upgrade").is_some());
        assert!(Action::from_name("open").is_some());
        assert!(Action::from_name("web").is_some());
        assert!(Action::from_name("unknown").is_none());
    }

    #[test]
    fn dsh_versions_are_checked_before_rollback() {
        assert!(is_safe_dsh_version("0.1.1-rc.2"));
        assert!(is_safe_dsh_version("1.2.3+build.4"));
        assert!(!is_safe_dsh_version(""));
        assert!(!is_safe_dsh_version("1.2.3;malicious"));
    }

    #[test]
    fn version_parser_ignores_npm_noise() {
        assert_eq!(
            parse_dsh_version("npm notice checking\n0.1.1-rc.2\n"),
            Some("0.1.1-rc.2".to_owned())
        );
        assert_eq!(parse_dsh_version("not a version"), None);
    }

    #[test]
    fn package_version_parser_reads_dsh_metadata() {
        assert_eq!(
            parse_package_version(
                "{\n  \"name\": \"@deepseek-ai/dsh\",\n  \"version\": \"0.1.1-rc.2\"\n}"
            ),
            Some("0.1.1-rc.2".to_owned())
        );
        assert_eq!(
            parse_package_version("{\n  \"version\": \"not safe;\"\n}"),
            None
        );
    }

    #[test]
    fn health_check_accepts_success_and_redirect_responses() {
        assert!(is_successful_http_status(200));
        assert!(is_successful_http_status(302));
        assert!(!is_successful_http_status(404));
        assert!(!is_successful_http_status(500));
    }

    #[test]
    fn tray_icon_follows_dsh_health_and_known_actions() {
        assert_eq!(
            tray_icon_running_state("服务运行中 · http://127.0.0.1:3080"),
            Some(true)
        );
        assert_eq!(
            tray_icon_running_state("服务已启动 · http://127.0.0.1:3080"),
            Some(true)
        );
        assert_eq!(
            tray_icon_running_state("服务响应异常 · HTTP 500"),
            Some(false)
        );
        assert_eq!(tray_icon_running_state("服务未启动"), Some(false));
        assert_eq!(tray_icon_running_state("更新失败：网络错误"), None);
    }

    #[test]
    fn wide_strings_are_terminated() {
        let value = to_wide("DSH");
        assert_eq!(value.last(), Some(&0));
    }

    #[test]
    fn truncate_respects_character_count() {
        assert_eq!(truncate("abcdef", 3), "abc");
        assert_eq!(truncate("中文测试", 2), "中文");
    }

    #[test]
    fn native_process_record_parser_rejects_stale_or_invalid_values() {
        assert_eq!(
            parse_native_dsh_process("8044:123456\n"),
            Some(NativeDshProcess {
                pid: 8044,
                started_at: 123456,
            })
        );
        assert_eq!(parse_native_dsh_process("8044\n"), None);
        assert_eq!(parse_native_dsh_process("0:123456"), None);
        assert_eq!(parse_native_dsh_process("not-a-pid:123456"), None);
    }

    #[test]
    fn tray_events_decode_version_four_payloads() {
        let version_four_context_menu = ((1usize << 16) | WM_CONTEXTMENU as usize) as isize;
        let version_four_select = ((1usize << 16) | NIN_SELECT as usize) as isize;

        assert_eq!(tray_event(version_four_context_menu), WM_CONTEXTMENU);
        assert_eq!(tray_event(version_four_select), NIN_SELECT);
        assert!(tray_event_opens_menu(tray_event(version_four_context_menu)));
        assert!(tray_event_opens_window(tray_event(version_four_select)));
    }

    #[test]
    fn tray_event_routes_legacy_and_modern_interactions() {
        assert!(tray_event_opens_window(WM_LBUTTONUP));
        assert!(tray_event_opens_window(WM_LBUTTONDBLCLK));
        assert!(tray_event_opens_window(NIN_KEYSELECT));
        assert!(tray_event_opens_menu(WM_RBUTTONUP));
        assert!(tray_event_opens_menu(WM_CONTEXTMENU));
    }

    #[test]
    fn update_timeout_ends_a_stalled_child_process() {
        let mut command = hidden_command("cmd.exe");
        command.args(["/C", "ping 127.0.0.1 -n 10 > NUL"]);
        let started = Instant::now();

        let error = run_native_update_command_with_timeout(
            &mut command,
            "测试下载",
            Duration::from_millis(250),
        )
        .expect_err("stalled command should be stopped");

        assert!(error.contains("超过"));
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
