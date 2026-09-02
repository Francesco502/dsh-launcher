#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::VecDeque;
use std::env;
use std::ffi::{c_void, OsStr};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{SocketAddr, TcpStream};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM, POINT, RECT, WPARAM,
};
use windows_sys::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_ROUND,
};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawFocusRect, DrawTextW, EndPaint,
    FillRect, GetStockObject, GetSysColor, InvalidateRect, RoundRect, SelectObject, SetBkColor,
    SetBkMode, SetDCPenColor, SetTextColor, COLOR_BTNFACE, COLOR_BTNTEXT, COLOR_GRAYTEXT,
    COLOR_WINDOW, COLOR_WINDOWTEXT, DC_PEN, DEFAULT_PITCH, DT_CENTER, DT_SINGLELINE, DT_VCENTER,
    FF_DONTCARE, FW_NORMAL, FW_SEMIBOLD, GB2312_CHARSET, OPAQUE, OUT_DEFAULT_PRECIS, PAINTSTRUCT,
    TRANSPARENT,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, GetDiskFreeSpaceExW, MoveFileExW, WriteFile, FILE_ATTRIBUTE_NORMAL,
    FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE, MOVEFILE_REPLACE_EXISTING,
    MOVEFILE_WRITE_THROUGH, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{
    AttachConsole, GetConsoleMode, GetStdHandle, WriteConsoleW, ATTACH_PARENT_PROCESS,
    STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemServices::SS_CENTER;
use windows_sys::Win32::System::Threading::{
    CreateMutexW, GetExitCodeProcess, OpenProcess, ReleaseMutex, TerminateProcess,
    CREATE_NO_WINDOW, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};
use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_DISABLED, ODS_FOCUS, ODS_SELECTED};
use windows_sys::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, GetDpiForSystem, GetDpiForWindow, SetProcessDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, VK_ESCAPE};
use windows_sys::Win32::UI::Shell::{
    IsUserAnAdmin, SetCurrentProcessExplicitAppUserModelID, ShellExecuteW, Shell_NotifyIconW,
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_ERROR, NIIF_INFO, NIM_ADD, NIM_DELETE,
    NIM_MODIFY, NIM_SETVERSION, NIN_SELECT, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, FindWindowW, GetClientRect, GetCursorPos, GetDlgCtrlID, GetDlgItem,
    GetMessageW, GetWindowLongPtrW, IsDialogMessageW, IsWindowVisible, KillTimer, LoadCursorW,
    LoadIconW, MessageBoxW, PostMessageW, PostQuitMessage, RegisterClassExW,
    RegisterWindowMessageW, SendMessageW, SetForegroundWindow, SetTimer, SetWindowLongPtrW,
    SetWindowPos, SetWindowTextW, ShowWindow, SystemParametersInfoW, TrackPopupMenu,
    TranslateMessage, BS_OWNERDRAW, CREATESTRUCTW, EVENT_OBJECT_NAMECHANGE, GWLP_USERDATA, HICON,
    ICON_BIG, ICON_SMALL, ICON_SMALL2, IDC_ARROW, IDYES, MB_ICONERROR, MB_ICONINFORMATION,
    MB_ICONQUESTION, MB_OK, MB_YESNO, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG, OBJID_CLIENT,
    SPI_GETHIGHCONTRAST, SWP_NOACTIVATE, SWP_NOZORDER, SW_HIDE, SW_RESTORE, SW_SHOW, SW_SHOWNORMAL,
    TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU,
    WM_CREATE, WM_CTLCOLORSTATIC, WM_DESTROY, WM_DPICHANGED, WM_DRAWITEM, WM_KEYDOWN,
    WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_NCCREATE, WM_NULL, WM_PAINT, WM_RBUTTONUP, WM_SETFONT,
    WM_SETICON, WM_SETTINGCHANGE, WM_SIZE, WM_TIMER, WNDCLASSEXW, WS_CAPTION, WS_CHILD,
    WS_EX_APPWINDOW, WS_EX_CONTROLPARENT, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP,
    WS_VISIBLE,
};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const WINDOW_CLASS: &str = "DeepSeek.DSHLauncher.Window";
const WINDOW_TITLE: &str = "DSH启动器";
const APP_USER_MODEL_ID: &str = "DeepSeek.DSHLauncher";
const PORTABLE_MARKER: &str = "portable.flag";
const MANIFEST_FILE: &str = "runtime-manifest.json";
const MANIFEST_TEXT: &str = include_str!("../runtime-manifest.json");
const DATA_DIRECTORY: &str = "data";
const DSH_PORT: u16 = 3080;
const WEB_URL: &str = "http://127.0.0.1:3080/";
const NODE_DOWNLOAD_URL: &str = "https://nodejs.org/en/download";
const RELEASE_API_URL: &str =
    "https://api.github.com/repos/Francesco502/dsh-launcher/releases/latest";
const RELEASE_PAGE_URL: &str = "https://github.com/Francesco502/dsh-launcher/releases/latest";
const CREATE_NO_WINDOW_FLAG: u32 = CREATE_NO_WINDOW;
const STILL_ACTIVE_EXIT_CODE: u32 = 259;
const START_TIMEOUT: Duration = Duration::from_secs(300);
const STOP_TIMEOUT: Duration = Duration::from_secs(12);
const NPM_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const QUERY_TIMEOUT: Duration = Duration::from_secs(60);
const HEALTH_TIMEOUT: Duration = Duration::from_millis(350);
const LAUNCHER_QUERY_TIMEOUT: Duration = Duration::from_secs(30);
const LOG_LIMIT: u64 = 5 * 1024 * 1024;
const LOG_COPIES: usize = 5;
const UPDATE_REQUIRED_FREE_SPACE: u64 = 512 * 1024 * 1024;
const DSH_HTML_MARKER: &str = "__DSH_BOOT__";
const DSH_AUTH_MARKER: &str = "dsh web authentication required";
const CLI_OUTPUT_ENV: &str = "DSH_LAUNCHER_OUTPUT";

const ICON_BLUE: usize = 1;
const ICON_BLACK: usize = 2;
const TRAY_MESSAGE: u32 = WM_APP + 1;
const UI_MESSAGE: u32 = WM_APP + 2;
const SHOW_MESSAGE: u32 = WM_APP + 3;
const NIN_KEYSELECT: u32 = 1025;
const TIMER_TRAY_RETRY: usize = 1;
const TIMER_HEALTH: usize = 2;
const HEALTH_INTERVAL_MS: u32 = 15_000;
const CMD_MAIN: u32 = 1001;
const CMD_WEB: u32 = 1002;
const CMD_UPDATE_DSH: u32 = 1003;
const CMD_CHECK_LAUNCHER: u32 = 1004;
const CMD_SHOW: u32 = 1005;
const CMD_EXIT: u32 = 1006;
const ID_TITLE: u32 = 1101;
const ID_STATUS: u32 = 1102;
const CLIENT_WIDTH: i32 = 484;
const CLIENT_HEIGHT: i32 = 351;
const FONT_FAMILY: &str = "Microsoft YaHei UI";
const BLUE: u32 = rgb(58, 84, 220);
const BLUE_DARK: u32 = rgb(43, 66, 184);
const BG: u32 = rgb(244, 247, 252);
const SURFACE: u32 = rgb(255, 255, 255);
const TEXT: u32 = rgb(28, 38, 58);
const MUTED: u32 = rgb(91, 105, 128);
const BORDER: u32 = rgb(208, 217, 232);
const DISABLED: u32 = rgb(151, 160, 177);

const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Start,
    Stop,
    Upgrade,
    Open,
}

impl Action {
    fn from_name(value: &str) -> Option<Self> {
        match value {
            "start" => Some(Self::Start),
            "stop" => Some(Self::Stop),
            "upgrade" => Some(Self::Upgrade),
            "open" => Some(Self::Open),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Managed,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProfileMode {
    Portable,
    User,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Installation {
    source: Source,
    node: PathBuf,
    entry: PathBuf,
    version: String,
    profile: ProfileMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RuntimeManifest {
    schema_version: u64,
    architecture: String,
    package: String,
    registry: String,
    entry: String,
    node_download_page: String,
}

#[derive(Clone, Debug)]
struct Paths {
    data: PathBuf,
    npm_prefix: PathBuf,
    profile: PathBuf,
    state: PathBuf,
    updates: PathBuf,
    logs: PathBuf,
    cache: PathBuf,
    temp: PathBuf,
}

impl Paths {
    fn discover() -> Result<Self, String> {
        let executable = env::current_exe().map_err(|error| format!("无法定位启动器：{error}"))?;
        let root = executable
            .parent()
            .ok_or_else(|| "无法定位启动器目录".to_owned())?
            .to_path_buf();
        if !root.join(PORTABLE_MARKER).is_file() {
            return Err(
                "请下载并完整解压 DSH 启动器便携包；当前目录缺少 portable.flag。".to_owned(),
            );
        }
        verify_external_manifest(&root)?;
        let data = root.join(DATA_DIRECTORY);
        let paths = Self {
            npm_prefix: data.join("npm-global"),
            profile: data.join("profile").join(".dsh"),
            state: data.join("state"),
            updates: data.join("updates"),
            logs: data.join("logs"),
            cache: data.join("cache").join("npm"),
            temp: data.join("tmp"),
            data,
        };
        paths.ensure_layout()?;
        Ok(paths)
    }

    fn ensure_layout(&self) -> Result<(), String> {
        for path in [
            &self.data,
            &self.npm_prefix,
            &self.profile,
            &self.state,
            &self.updates,
            &self.logs,
            &self.cache,
            &self.temp,
        ] {
            fs::create_dir_all(path)
                .map_err(|error| format!("无法创建目录 {}：{error}", path.display()))?;
        }
        Ok(())
    }

    fn managed_package(&self) -> PathBuf {
        self.npm_prefix
            .join("node_modules")
            .join("@deepseek-ai")
            .join("dsh")
    }

    fn pid_file(&self) -> PathBuf {
        self.state.join("dsh.pid")
    }

    fn install_state_file(&self) -> PathBuf {
        self.state.join("dsh-install.json")
    }

    fn transaction_file(&self) -> PathBuf {
        self.state.join("dsh-update-transaction.json")
    }

    fn repair_file(&self) -> PathBuf {
        self.state.join("dsh-repair-needed")
    }
}

#[derive(Clone, Debug)]
struct UpdateTransaction {
    phase: String,
    target: PathBuf,
    backup: Option<PathBuf>,
    stage: PathBuf,
    profile: ProfileMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: Vec<Identifier>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Identifier {
    Number(u64),
    Text(String),
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
            .then_with(
                || match (self.prerelease.is_empty(), other.prerelease.is_empty()) {
                    (true, true) => std::cmp::Ordering::Equal,
                    (true, false) => std::cmp::Ordering::Greater,
                    (false, true) => std::cmp::Ordering::Less,
                    (false, false) => compare_identifiers(&self.prerelease, &other.prerelease),
                },
            )
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.prerelease.is_empty() {
            write!(formatter, "-")?;
            for (index, value) in self.prerelease.iter().enumerate() {
                if index > 0 {
                    write!(formatter, ".")?;
                }
                match value {
                    Identifier::Number(number) => write!(formatter, "{number}")?,
                    Identifier::Text(text) => write!(formatter, "{text}")?,
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct Snapshot {
    installation: Option<Installation>,
    node_available: bool,
    npm_available: bool,
    running: bool,
    healthy: bool,
    auth_unavailable: bool,
    repair_needed: bool,
    discovery_error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct DshProbe {
    identified: bool,
    web_url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MainButton {
    Start,
    Stop,
    InstallDsh,
    InstallNode,
    RepairDsh,
    Cancel,
    Busy,
}

fn main_button(snapshot: &Snapshot, busy: bool, cancelable: bool) -> MainButton {
    if busy {
        return if cancelable {
            MainButton::Cancel
        } else {
            MainButton::Busy
        };
    }
    if snapshot.running {
        MainButton::Stop
    } else if snapshot.repair_needed {
        if snapshot.node_available && snapshot.npm_available {
            MainButton::RepairDsh
        } else {
            MainButton::InstallNode
        }
    } else if snapshot.installation.is_some() {
        MainButton::Start
    } else if snapshot.node_available && snapshot.npm_available {
        MainButton::InstallDsh
    } else {
        MainButton::InstallNode
    }
}

struct AppState {
    paths: Paths,
    blue_icon: AtomicUsize,
    black_icon: AtomicUsize,
    tray_icon: AtomicUsize,
    tray_added: AtomicBool,
    taskbar_created: u32,
    background_brush: usize,
    control_background_brush: AtomicUsize,
    title_font: AtomicUsize,
    body_font: AtomicUsize,
    small_font: AtomicUsize,
    snapshot: Mutex<Snapshot>,
    status: Mutex<String>,
    messages: Mutex<VecDeque<String>>,
    busy: AtomicBool,
    cancelable: AtomicBool,
    health_checking: AtomicBool,
    close_notice_shown: AtomicBool,
    high_contrast: AtomicBool,
}

#[derive(Clone, Copy, Debug)]
enum Operation {
    Start,
    Stop,
    Install,
    Upgrade,
}

static PATHS: OnceLock<Result<Paths, String>> = OnceLock::new();
static MANIFEST: OnceLock<Result<RuntimeManifest, String>> = OnceLock::new();
static CANCEL: AtomicBool = AtomicBool::new(false);
const ACTION_MUTEX: &str = "Local\\DeepSeek.DSHLauncher.Action";

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
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let args: Vec<String> = env::args().collect();
    if is_release_smoke(&args) {
        attach_console();
        exit_with(run_release_smoke());
    }
    let action = match parse_action(&args) {
        Ok(value) => value,
        Err(error) => {
            attach_console();
            write_console(&error, true);
            std::process::exit(2);
        }
    };
    if let Some(action) = action {
        attach_console();
        let result = acquire_action_mutex()
            .ok_or_else(|| "已有启动器操作正在执行".to_owned())
            .and_then(|_guard| ensure_not_elevated().and_then(|_| execute_action(action)));
        exit_with(result);
    }
    let _instance = match acquire_single_instance() {
        Some(value) => value,
        None => return,
    };
    if let Err(error) = ensure_not_elevated().and_then(|_| run_app()) {
        show_error_box(std::ptr::null_mut(), &error);
        std::process::exit(1);
    }
}

fn is_release_smoke(args: &[String]) -> bool {
    args.len() == 2 && args[1] == "--release-smoke"
}

fn parse_action(args: &[String]) -> Result<Option<Action>, String> {
    if args.len() == 1 {
        return Ok(None);
    }
    if args.len() != 3 || args[1] != "--action" {
        return Err(format!("未知参数：{}", args[1..].join(" ")));
    }
    Action::from_name(&args[2]).map(Some).ok_or_else(|| {
        format!(
            "未知操作“{}”；可用操作：start、stop、upgrade、open",
            args[2]
        )
    })
}

fn run_release_smoke() -> Result<String, String> {
    let paths = app_paths()?;
    recover_update_transaction(&paths)?;
    cleanup_npm_cache(&paths);
    Ok(format!("DSH启动器 v{APP_VERSION} 轻量便携包初始化检查通过"))
}

fn execute_action(action: Action) -> Result<String, String> {
    let paths = app_paths()?;
    recover_for_use(&paths)?;
    match action {
        Action::Start => start_dsh(),
        Action::Stop => stop_dsh(),
        Action::Upgrade => install_or_update(false, &|_, _| {}, None),
        Action::Open => open_dsh_web(&paths),
    }
}

fn exit_with(result: Result<String, String>) -> ! {
    match result {
        Ok(message) => {
            write_console(&message, false);
            std::process::exit(0);
        }
        Err(error) => {
            write_console(&error, true);
            std::process::exit(1);
        }
    }
}

fn ensure_not_elevated() -> Result<(), String> {
    if unsafe { IsUserAnAdmin() } != 0 {
        Err("请不要以管理员身份运行；DSH 应使用当前 Windows 用户配置。".to_owned())
    } else {
        Ok(())
    }
}

fn app_paths() -> Result<Paths, String> {
    PATHS.get_or_init(Paths::discover).clone()
}

fn runtime_manifest() -> Result<RuntimeManifest, String> {
    MANIFEST
        .get_or_init(|| parse_runtime_manifest(MANIFEST_TEXT))
        .clone()
}

fn parse_runtime_manifest(text: &str) -> Result<RuntimeManifest, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|error| format!("运行清单 JSON 无效：{error}"))?;
    let string = |object: &serde_json::Value, key: &str| {
        object
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("运行清单缺少 {key}"))
    };
    let dsh = value
        .get("dsh")
        .ok_or_else(|| "运行清单缺少 dsh".to_owned())?;
    let node = value
        .get("node")
        .ok_or_else(|| "运行清单缺少 node".to_owned())?;
    let manifest = RuntimeManifest {
        schema_version: value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| "运行清单缺少 schema_version".to_owned())?,
        architecture: string(&value, "architecture")?,
        package: string(dsh, "package")?,
        registry: string(dsh, "registry")?,
        entry: string(dsh, "entry")?,
        node_download_page: string(node, "download_page")?,
    };
    if manifest.schema_version != 2
        || manifest.architecture != "x86_64-pc-windows-gnu"
        || manifest.package != "@deepseek-ai/dsh"
        || manifest.registry != "https://registry.npmjs.org/"
        || manifest.entry != "lib/bin.js"
        || manifest.node_download_page != NODE_DOWNLOAD_URL
    {
        return Err("运行清单不符合 0.3.1 轻量契约".to_owned());
    }
    Ok(manifest)
}

fn verify_external_manifest(root: &Path) -> Result<(), String> {
    let embedded = runtime_manifest()?;
    let path = root.join(MANIFEST_FILE);
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("缺少或无法读取 {}：{error}", path.display()))?;
    let external = parse_runtime_manifest(&text)?;
    if external != embedded {
        return Err("便携包 runtime-manifest.json 与启动器不一致".to_owned());
    }
    Ok(())
}

fn discover_installation(paths: &Paths) -> Result<Option<Installation>, String> {
    let node = find_command("node.exe");
    let Some(node) = node else {
        return Ok(None);
    };
    let managed = paths.managed_package();
    if let Some(installation) = installation_from_package(
        &managed,
        Source::Managed,
        node.clone(),
        read_profile_mode(paths).unwrap_or(ProfileMode::Portable),
    )? {
        return Ok(Some(installation));
    }
    if let Some(dsh_command) = find_command("dsh.cmd") {
        if let Some(prefix) = dsh_command.parent() {
            if let Some(installation) = installation_from_package(
                &prefix.join("node_modules").join("@deepseek-ai").join("dsh"),
                Source::System,
                node.clone(),
                ProfileMode::User,
            )? {
                return Ok(Some(installation));
            }
        }
    }
    let npm = match find_command("npm.cmd") {
        Some(value) => value,
        None => return Ok(None),
    };
    let output = hidden_command(&npm)
        .args(["root", "-g"])
        .output()
        .map_err(|error| format!("无法查询 npm 全局目录：{error}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if root.is_empty() {
        return Ok(None);
    }
    installation_from_package(
        &PathBuf::from(root).join("@deepseek-ai").join("dsh"),
        Source::System,
        node,
        ProfileMode::User,
    )
}

fn installation_from_package(
    package: &Path,
    source: Source,
    node: PathBuf,
    profile: ProfileMode,
) -> Result<Option<Installation>, String> {
    let manifest = runtime_manifest()?;
    let entry = package.join(&manifest.entry);
    let metadata = package.join("package.json");
    if !entry.is_file() || !metadata.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&metadata)
        .map_err(|error| format!("无法读取 DSH 元数据 {}：{error}", metadata.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("DSH 元数据无效：{error}"))?;
    if value.get("name").and_then(serde_json::Value::as_str) != Some(manifest.package.as_str()) {
        return Err(format!("DSH 包名无效：{}", metadata.display()));
    }
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "DSH 元数据缺少版本".to_owned())?;
    parse_version(version).ok_or_else(|| format!("DSH 版本号无效：{version}"))?;
    Ok(Some(Installation {
        source,
        node,
        entry,
        version: version.to_owned(),
        profile,
    }))
}

fn find_command(name: &str) -> Option<PathBuf> {
    env::var_os("PATH")
        .into_iter()
        .flat_map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .map(|directory| directory.join(name))
        .find(|path| path.is_file())
}

fn inspect_snapshot(paths: &Paths, installation: Option<Installation>) -> Snapshot {
    let node_available = find_command("node.exe").is_some();
    let npm_available = find_command("npm.cmd").is_some();
    let tracked = tracked_process_running(paths).unwrap_or(false);
    let probe = probe_dsh(paths);
    let healthy = probe.web_url.is_some();
    let running = tracked || probe.identified;
    let repair_needed = paths.repair_file().is_file()
        || (paths.managed_package().exists() && installation.is_none());
    Snapshot {
        installation,
        node_available,
        npm_available,
        running,
        healthy,
        auth_unavailable: probe.identified && !healthy,
        repair_needed,
        discovery_error: None,
    }
}

fn refresh_discovery(paths: &Paths) -> Snapshot {
    match discover_installation(paths) {
        Ok(installation) => inspect_snapshot(paths, installation),
        Err(error) => {
            let mut snapshot = inspect_snapshot(paths, None);
            snapshot.discovery_error = Some(error);
            snapshot
        }
    }
}

fn start_dsh() -> Result<String, String> {
    let paths = app_paths()?;
    if paths.repair_file().is_file() {
        return Err("DSH 最新版本需要重新安装；请先使用“重新安装 DSH”或 upgrade。".to_owned());
    }
    let installation = discover_installation(&paths)?
        .ok_or_else(|| "未找到 DSH；请先在启动器中安装 DSH。".to_owned())?;
    start_installation(&paths, &installation)
}

fn start_installation(paths: &Paths, installation: &Installation) -> Result<String, String> {
    let probe = probe_dsh(paths);
    if probe.web_url.is_some() {
        clear_repair_needed(paths);
        return Ok(format!("DSH 已在运行 · {WEB_URL}"));
    }
    if probe.identified {
        return Err("DSH 已在运行，但认证 URL 不可用；请先停止后由启动器重新启动。".to_owned());
    }
    if tracked_process_running(paths)? {
        return Err("DSH 进程正在启动，但 Web UI 尚未就绪；请稍候。".to_owned());
    }
    if tcp_open(DSH_PORT) {
        return Err("3080 端口已被其他程序占用；未启动 DSH。".to_owned());
    }
    rotate_log(&paths.logs.join("dsh.out.log"))?;
    rotate_log(&paths.logs.join("dsh.err.log"))?;
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.logs.join("dsh.out.log"))
        .map_err(|error| format!("无法打开 DSH 输出日志：{error}"))?;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.logs.join("dsh.err.log"))
        .map_err(|error| format!("无法打开 DSH 错误日志：{error}"))?;
    let mut command = hidden_command(&installation.node);
    command
        .arg(&installation.entry)
        .args(["web", "--no-open", "--host", "127.0.0.1", "--port", "3080"])
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    if installation.profile == ProfileMode::Portable {
        fs::create_dir_all(&paths.profile)
            .map_err(|error| format!("无法创建便携配置目录：{error}"))?;
        command.env("DSH_HOME", &paths.profile);
    }
    command.env("TEMP", &paths.temp).env("TMP", &paths.temp);
    let child = command
        .spawn()
        .map_err(|error| format!("无法启动 DSH：{error}"))?;
    fs::write(paths.pid_file(), child.id().to_string())
        .map_err(|error| format!("无法记录 DSH 进程：{error}"))?;
    let deadline = Instant::now() + START_TIMEOUT;
    while Instant::now() < deadline {
        if probe_dsh(paths).web_url.is_some() {
            clear_repair_needed(paths);
            return Ok(format!("DSH 已启动 · {WEB_URL}"));
        }
        if !process_running(child.id())? {
            let _ = fs::remove_file(paths.pid_file());
            mark_repair_needed(paths, installation);
            return Err(format!(
                "DSH 启动进程已退出。日志：{}",
                paths.logs.join("dsh.err.log").display()
            ));
        }
        thread::sleep(Duration::from_millis(350));
    }
    let _ = terminate_pid(child.id());
    let _ = fs::remove_file(paths.pid_file());
    mark_repair_needed(paths, installation);
    Err(format!(
        "DSH 启动超时。日志：{}",
        paths.logs.join("dsh.err.log").display()
    ))
}

fn stop_dsh() -> Result<String, String> {
    let paths = app_paths()?;
    let pid = read_tracked_pid(&paths).filter(|pid| process_running(*pid).unwrap_or(false));
    let pid = match pid {
        Some(pid) => pid,
        None if tcp_open(DSH_PORT) => find_external_dsh_pid(DSH_PORT)?.ok_or_else(|| {
            "3080 端口正在使用，但无法确认其属于 DSH；未停止任何进程。".to_owned()
        })?,
        None => {
            let _ = fs::remove_file(paths.pid_file());
            return Ok("DSH 已停止".to_owned());
        }
    };
    let command_line = process_command_line(pid)?
        .ok_or_else(|| format!("无法验证进程 {pid} 的命令行；未停止任何进程。"))?;
    if !is_dsh_command(&command_line, DSH_PORT) {
        return Err(format!(
            "进程 {pid} 不是可验证的 DSH 服务；未停止任何进程。"
        ));
    }
    let forced = terminate_pid(pid)?;
    let _ = fs::remove_file(paths.pid_file());
    append_log(
        &paths.logs.join("launcher.log"),
        &format!("停止 DSH pid={pid} forced={forced}"),
    );
    Ok(if forced {
        "DSH 已停止（常规停止超时，已强制结束）".to_owned()
    } else {
        "DSH 已停止".to_owned()
    })
}

fn install_or_update(
    allow_install: bool,
    progress: &dyn Fn(&str, bool),
    confirm: Option<&dyn Fn(&str) -> bool>,
) -> Result<String, String> {
    let paths = app_paths()?;
    let current = match discover_installation(&paths) {
        Ok(value) => value,
        Err(error) if paths.managed_package().exists() => {
            progress("检测到管理副本损坏，将重新安装当前最新版...", true);
            append_log(&paths.logs.join("launcher.log"), &error);
            None
        }
        Err(error) => return Err(error),
    };
    let repair_version = read_repair_version(&paths);
    let repair_requested =
        paths.repair_file().is_file() || (paths.managed_package().exists() && current.is_none());
    if current.is_none() && !allow_install && !repair_requested {
        return Err("未找到 DSH；请先使用“安装 DSH”。".to_owned());
    }
    let npm = find_command("npm.cmd").ok_or_else(|| "未找到 npm；请先安装 Node.js。".to_owned())?;
    progress("正在查询 DSH 官方版本列表...", true);
    let latest = latest_dsh_version(&paths, &npm)?;
    let mut target_version = latest.clone();
    if let Some(current) = &current {
        let current_version = parse_version(&current.version)
            .ok_or_else(|| format!("当前 DSH 版本无效：{}", current.version))?;
        let latest_version =
            parse_version(&latest).expect("registry parser returned valid version");
        if latest_version < current_version {
            if repair_requested {
                target_version = current.version.clone();
            } else {
                return Ok(format!(
                    "当前 DSH {} 高于官方版本 {latest}，不降级",
                    current.version
                ));
            }
        } else if latest_version == current_version && !repair_requested {
            return Ok(format!("DSH 已是最新版本 · {}", current.version));
        }
    } else if let Some(repair) = &repair_version {
        if parse_version(repair).is_some_and(|value| {
            value > parse_version(&target_version).expect("registry parser returned valid version")
        }) {
            target_version = repair.clone();
        }
    }
    if CANCEL.load(Ordering::Acquire) {
        return Err("操作已取消".to_owned());
    }
    if let Some(confirm) = confirm {
        let description = match (&current, repair_requested) {
            (Some(_), true) => format!(
                "将重新安装 DSH {}，不会恢复旧版本。\n管理目录：{}\n\n继续？",
                target_version,
                paths.npm_prefix.display()
            ),
            (Some(installed), false) => format!(
                "将把 DSH {} 更新到 {}。\n管理目录：{}\n\n继续？",
                installed.version,
                target_version,
                paths.npm_prefix.display()
            ),
            (None, _) => format!(
                "将安装 @deepseek-ai/dsh@{}。\n管理目录：{}\n\n继续？",
                target_version,
                paths.npm_prefix.display()
            ),
        };
        if !confirm(&description) {
            return Err("操作已取消".to_owned());
        }
    }
    let was_running = tracked_process_running(&paths)? || probe_dsh(&paths).identified;
    if !was_running && tcp_open(DSH_PORT) {
        return Err("3080 端口已被其他程序占用，无法安全更新 DSH。".to_owned());
    }
    let target_profile = update_profile_mode(current.as_ref(), read_profile_mode(&paths));
    progress(
        &format!("正在暂存 @deepseek-ai/dsh@{target_version}..."),
        true,
    );
    let stage = stage_dsh(&paths, &npm, &target_version)?;
    if CANCEL.load(Ordering::Acquire) {
        let _ = safe_remove_dir(&paths, &stage);
        return Err("操作已取消".to_owned());
    }
    progress("正在提交更新，此阶段不可取消...", false);
    if was_running {
        progress("正在停止旧 DSH...", false);
        if let Err(error) = stop_dsh() {
            let cleanup = safe_remove_dir(&paths, &stage).err();
            return Err(match cleanup {
                Some(cleanup) => format!("{error}\n同时无法清理候选目录：{cleanup}"),
                None => error,
            });
        }
    }
    progress("正在提交 DSH 目录交换...", false);
    promote_stage(&paths, &stage, target_profile)?;
    if let Err(error) = finalize_committed_transaction(&paths) {
        append_log(
            &paths.logs.join("launcher.log"),
            &format!("最新 DSH 已提交，旧更新文件稍后清理：{error}"),
        );
    }
    clear_repair_needed(&paths);
    if was_running {
        let start_result = discover_installation(&paths)
            .and_then(|installation| installation.ok_or_else(|| "提交后无法发现 DSH".to_owned()))
            .and_then(|promoted| start_installation(&paths, &promoted));
        match start_result {
            Ok(_) => Ok(format!("DSH 已更新到 {target_version} 并重新启动")),
            Err(error) => Err(format!(
                "DSH 已更新到 {target_version}，但启动失败：{error}\n最新版本已保留，可重新安装同一版本。"
            )),
        }
    } else {
        Ok(format!("DSH 已更新到 {target_version}"))
    }
}

fn update_profile_mode(current: Option<&Installation>, saved: Option<ProfileMode>) -> ProfileMode {
    current
        .map(|installation| installation.profile)
        .or(saved)
        .unwrap_or(ProfileMode::Portable)
}

fn latest_dsh_version(paths: &Paths, npm: &Path) -> Result<String, String> {
    let manifest = runtime_manifest()?;
    let mut command = hidden_command(npm);
    command.args([
        "view",
        manifest.package.as_str(),
        "versions",
        "--json",
        "--registry",
        manifest.registry.as_str(),
        "--no-audit",
        "--no-fund",
    ]);
    command
        .env("npm_config_cache", &paths.cache)
        .env("TEMP", &paths.temp)
        .env("TMP", &paths.temp);
    let result = run_capture(paths, &mut command, "查询 DSH 版本", QUERY_TIMEOUT, true);
    cleanup_npm_cache(paths);
    let output = result?;
    parse_latest_version(&output).ok_or_else(|| "官方版本列表中没有有效 SemVer".to_owned())
}

fn parse_latest_version(text: &str) -> Option<String> {
    let candidates: Vec<String> = serde_json::from_str::<Vec<String>>(text).unwrap_or_else(|_| {
        text.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect()
    });
    candidates
        .into_iter()
        .filter_map(|text| parse_version(&text).map(|version| (version, text)))
        .max_by(|left, right| left.0.cmp(&right.0))
        .map(|(_, text)| text)
}

fn stage_dsh(paths: &Paths, npm: &Path, version: &str) -> Result<PathBuf, String> {
    ensure_update_space(paths, UPDATE_REQUIRED_FREE_SPACE)?;
    let manifest = runtime_manifest()?;
    let stage = paths
        .updates
        .join(format!("dsh-stage-{}", transaction_nonce()));
    fs::create_dir_all(&stage).map_err(|error| format!("无法创建更新暂存目录：{error}"))?;
    let spec = format!("{}@{version}", manifest.package);
    let mut command = hidden_command(npm);
    command.args([
        "install",
        "--prefix",
        stage.to_string_lossy().as_ref(),
        "--ignore-scripts",
        "--omit=dev",
        "--no-audit",
        "--no-fund",
        "--registry",
        manifest.registry.as_str(),
        spec.as_str(),
    ]);
    command
        .env("npm_config_cache", &paths.cache)
        .env("TEMP", &paths.temp)
        .env("TMP", &paths.temp);
    let result = run_capture(paths, &mut command, "安装 DSH 候选包", NPM_TIMEOUT, true);
    cleanup_npm_cache(paths);
    if let Err(error) = result {
        let _ = safe_remove_dir(paths, &stage);
        return Err(error);
    }
    let package = stage.join("node_modules").join("@deepseek-ai").join("dsh");
    if let Err(error) = verify_staged_package(&package, version) {
        let _ = safe_remove_dir(paths, &stage);
        return Err(error);
    }
    Ok(stage)
}

fn verify_staged_package(package: &Path, expected: &str) -> Result<(), String> {
    let manifest = runtime_manifest()?;
    let metadata = fs::read_to_string(package.join("package.json"))
        .map_err(|error| format!("候选包缺少 package.json：{error}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&metadata).map_err(|error| format!("候选包元数据无效：{error}"))?;
    if value.get("name").and_then(serde_json::Value::as_str) != Some(manifest.package.as_str())
        || value.get("version").and_then(serde_json::Value::as_str) != Some(expected)
        || !package.join(&manifest.entry).is_file()
    {
        return Err("候选 DSH 的包名、版本或入口文件不匹配".to_owned());
    }
    let dependencies = value
        .get("dependencies")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "候选 DSH 元数据缺少 dependencies".to_owned())?;
    let top_level = package
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "候选 DSH 目录结构无效".to_owned())?;
    for dependency in dependencies.keys() {
        let hoisted = top_level.join(dependency).join("package.json");
        let nested = package
            .join("node_modules")
            .join(dependency)
            .join("package.json");
        if !hoisted.is_file() && !nested.is_file() {
            return Err(format!("候选 DSH 缺少运行依赖：{dependency}"));
        }
    }
    Ok(())
}

fn ensure_update_space(paths: &Paths, required: u64) -> Result<(), String> {
    let directory = to_wide(&paths.updates.to_string_lossy());
    let mut available = 0u64;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            directory.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(format!("无法检查更新磁盘空间：{}", unsafe {
            GetLastError()
        }));
    }
    if available < required {
        return Err(format!(
            "更新目录可用空间不足：至少需要 {} MiB",
            required / 1024 / 1024
        ));
    }
    Ok(())
}

fn cleanup_npm_cache(paths: &Paths) {
    if paths.cache.exists() {
        if let Err(error) = safe_remove_dir(paths, &paths.cache) {
            append_log(
                &paths.logs.join("launcher.log"),
                &format!("无法清理 npm 缓存：{error}"),
            );
            return;
        }
    }
    if let Err(error) = fs::create_dir_all(&paths.cache) {
        append_log(
            &paths.logs.join("launcher.log"),
            &format!("无法重建 npm 缓存目录：{error}"),
        );
    }
}

fn promote_stage(
    paths: &Paths,
    stage: &Path,
    profile: ProfileMode,
) -> Result<UpdateTransaction, String> {
    ensure_under(&paths.data, stage)?;
    let target = paths.npm_prefix.clone();
    fs::create_dir_all(&paths.data).map_err(|error| format!("无法创建数据目录：{error}"))?;
    let backup = target.exists().then(|| {
        paths
            .updates
            .join(format!("dsh-old-{}", transaction_nonce()))
    });
    let mut transaction = UpdateTransaction {
        phase: "prepared".to_owned(),
        target: target.clone(),
        backup: backup.clone(),
        stage: stage.to_path_buf(),
        profile,
    };
    write_transaction(paths, &transaction)?;
    if let Err(error) = write_profile_mode(paths, profile) {
        let _ = fs::remove_file(paths.transaction_file());
        return Err(error);
    }
    if let Some(backup) = &backup {
        fs::rename(&target, backup).map_err(|error| format!("无法备份当前 DSH：{error}"))?;
        transaction.phase = "old-moved".to_owned();
        write_transaction(paths, &transaction)?;
    }
    if let Err(error) = fs::rename(stage, &target) {
        return Err(format!(
            "无法提交最新 DSH 候选目录：{error}；旧版本不会恢复，下次启动将继续提交"
        ));
    }
    transaction.phase = "committed".to_owned();
    if let Err(error) = write_transaction(paths, &transaction) {
        return Err(format!(
            "最新 DSH 已提交，但无法记录清理事务：{error}；最新目录已保留"
        ));
    }
    Ok(transaction)
}

fn recover_update_transaction(paths: &Paths) -> Result<(), String> {
    let transaction = match read_transaction(paths) {
        Ok(Some(transaction)) => transaction,
        Ok(None) if paths.repair_file().is_file() => return Ok(()),
        Ok(None) => return cleanup_update_artifacts(paths),
        Err(error) => {
            append_log(
                &paths.logs.join("launcher.log"),
                &format!("更新事务损坏，按最新暂存恢复：{error}"),
            );
            return recover_orphaned_stage(paths);
        }
    };

    let mut transaction = transaction;
    write_profile_mode(paths, transaction.profile)?;
    let candidate = if transaction.stage.exists() {
        &transaction.stage
    } else {
        if transaction.phase == "prepared"
            && transaction
                .backup
                .as_ref()
                .is_some_and(|path| !path.exists())
        {
            return recovery_failed(
                paths,
                &transaction.stage,
                "更新尚未提交且最新暂存目录已丢失",
            );
        }
        &transaction.target
    };
    if let Err(error) = verify_recovery_candidate(candidate) {
        return recovery_failed(paths, candidate, &error);
    }
    if transaction.stage.exists() {
        if transaction.target.exists() {
            if let Some(backup) = &transaction.backup {
                if !backup.exists() {
                    fs::rename(&transaction.target, backup)
                        .map_err(|error| format!("无法继续移出旧 DSH：{error}"))?;
                } else {
                    safe_remove_dir(paths, &transaction.target)?;
                }
            } else {
                safe_remove_dir(paths, &transaction.target)?;
            }
        }
        fs::rename(&transaction.stage, &transaction.target)
            .map_err(|error| format!("无法继续提交最新 DSH：{error}"))?;
    }
    transaction.phase = "committed".to_owned();
    write_transaction(paths, &transaction)?;
    finalize_committed_transaction(paths)?;
    clear_repair_needed(paths);
    Ok(())
}

fn recovery_version(prefix: &Path) -> Option<String> {
    let text =
        fs::read_to_string(prefix.join("node_modules/@deepseek-ai/dsh/package.json")).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    if value.get("name")?.as_str()? != "@deepseek-ai/dsh" {
        return None;
    }
    let version = value.get("version")?.as_str()?;
    parse_version(version).map(|_| version.to_owned())
}

fn verify_recovery_candidate(prefix: &Path) -> Result<(), String> {
    let version =
        recovery_version(prefix).ok_or_else(|| "更新候选丢失或 DSH 元数据无效".to_owned())?;
    verify_staged_package(&prefix.join("node_modules/@deepseek-ai/dsh"), &version)
}

fn recovery_failed(paths: &Paths, candidate: &Path, reason: &str) -> Result<(), String> {
    let version = recovery_version(candidate).or_else(|| read_repair_version(paths));
    atomic_write(&paths.repair_file(), version.unwrap_or_default().as_bytes())?;
    fs::remove_file(paths.transaction_file())
        .map_err(|error| format!("无法结束损坏的更新事务：{error}"))?;
    Err(format!(
        "{reason}；请重新安装 DSH 最新版本，旧版本不会重新启用。"
    ))
}

fn recover_for_use(paths: &Paths) -> Result<(), String> {
    match recover_update_transaction(paths) {
        Err(error) if paths.repair_file().is_file() && !paths.transaction_file().exists() => {
            append_log(&paths.logs.join("launcher.log"), &error);
            Ok(())
        }
        result => result,
    }
}

fn finalize_committed_transaction(paths: &Paths) -> Result<(), String> {
    let Some(transaction) = read_transaction(paths)? else {
        return Ok(());
    };
    if transaction.phase != "committed" {
        return Ok(());
    }
    if let Some(backup) = transaction.backup {
        if backup.exists() {
            safe_remove_dir(paths, &backup)?;
        }
    }
    if transaction.stage.exists() {
        safe_remove_dir(paths, &transaction.stage)?;
    }
    fs::remove_file(paths.transaction_file())
        .map_err(|error| format!("无法清理更新事务：{error}"))?;
    cleanup_update_artifacts(paths)
}

fn write_transaction(paths: &Paths, transaction: &UpdateTransaction) -> Result<(), String> {
    let profile = match transaction.profile {
        ProfileMode::Portable => "portable",
        ProfileMode::User => "user",
    };
    let value = serde_json::json!({
        "phase": transaction.phase,
        "target": transaction.target.to_string_lossy(),
        "backup": transaction.backup.as_ref().map(|path| path.to_string_lossy().to_string()),
        "stage": transaction.stage.to_string_lossy(),
        "profile": profile,
    });
    atomic_write(&paths.transaction_file(), value.to_string().as_bytes())
        .map_err(|error| format!("无法写入更新事务：{error}"))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("无法定位文件父目录：{}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("无法创建目录 {}：{error}", parent.display()))?;
    let temporary = parent.join(format!(".write-{}.tmp", transaction_nonce()));
    let mut file = fs::File::create(&temporary)
        .map_err(|error| format!("无法创建临时文件 {}：{error}", temporary.display()))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("无法写入临时文件 {}：{error}", temporary.display()))?;
    drop(file);
    let from = to_wide(&temporary.to_string_lossy());
    let to = to_wide(&path.to_string_lossy());
    let moved = unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if moved == 0 {
        let error = unsafe { GetLastError() };
        let _ = fs::remove_file(&temporary);
        return Err(format!("无法原子替换 {}：{error}", path.display()));
    }
    Ok(())
}

fn recover_orphaned_stage(paths: &Paths) -> Result<(), String> {
    let mut stages = update_directories(paths, "dsh-stage-")?;
    stages.sort_by_key(|path| path.metadata().and_then(|value| value.modified()).ok());
    if let Some(stage) = stages.pop() {
        if let Err(error) = verify_recovery_candidate(&stage) {
            return recovery_failed(paths, &stage, &error);
        }
        promote_stage(
            paths,
            &stage,
            read_profile_mode(paths).unwrap_or(ProfileMode::Portable),
        )?;
        finalize_committed_transaction(paths)?;
        clear_repair_needed(paths);
        return Ok(());
    }
    recovery_failed(paths, &paths.updates, "事务损坏且没有可验证的最新暂存目录")
}

fn cleanup_update_artifacts(paths: &Paths) -> Result<(), String> {
    for prefix in ["dsh-stage-", "dsh-old-", "dsh-rollback-"] {
        for directory in update_directories(paths, prefix)? {
            safe_remove_dir(paths, &directory)?;
        }
    }
    Ok(())
}

fn update_directories(paths: &Paths, prefix: &str) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(&paths.updates)
        .map_err(|error| format!("无法读取更新目录 {}：{error}", paths.updates.display()))?;
    Ok(entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            path.file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.starts_with(prefix))
        })
        .collect())
}

fn read_transaction(paths: &Paths) -> Result<Option<UpdateTransaction>, String> {
    let file = paths.transaction_file();
    if !file.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&file).map_err(|error| format!("无法读取更新事务：{error}"))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("更新事务无效：{error}"))?;
    let path = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| format!("更新事务缺少 {key}"))
    };
    let profile = match value.get("profile").and_then(serde_json::Value::as_str) {
        Some("user") => ProfileMode::User,
        Some("portable") => ProfileMode::Portable,
        _ => return Err("更新事务 profile 无效".to_owned()),
    };
    let transaction = UpdateTransaction {
        phase: value
            .get("phase")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "更新事务缺少 phase".to_owned())?
            .to_owned(),
        target: path("target")?,
        backup: value
            .get("backup")
            .and_then(serde_json::Value::as_str)
            .map(PathBuf::from),
        stage: path("stage")?,
        profile,
    };
    ensure_under(&paths.data, &transaction.target)?;
    ensure_under(&paths.data, &transaction.stage)?;
    if let Some(backup) = &transaction.backup {
        ensure_under(&paths.data, backup)?;
    }
    Ok(Some(transaction))
}

fn read_profile_mode(paths: &Paths) -> Option<ProfileMode> {
    let text = fs::read_to_string(paths.install_state_file()).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    match value.get("profile").and_then(serde_json::Value::as_str) {
        Some("user") => Some(ProfileMode::User),
        Some("portable") => Some(ProfileMode::Portable),
        _ => None,
    }
}

fn write_profile_mode(paths: &Paths, mode: ProfileMode) -> Result<(), String> {
    let profile = match mode {
        ProfileMode::Portable => "portable",
        ProfileMode::User => "user",
    };
    atomic_write(
        &paths.install_state_file(),
        serde_json::json!({ "profile": profile })
            .to_string()
            .as_bytes(),
    )
    .map_err(|error| format!("无法记录 DSH 配置模式：{error}"))
}

fn ensure_under(root: &Path, path: &Path) -> Result<(), String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("无法验证数据目录：{error}"))?;
    let candidate = if path.exists() {
        path.canonicalize()
            .map_err(|error| format!("无法验证目录 {}：{error}", path.display()))?
    } else {
        let parent = path
            .parent()
            .ok_or_else(|| format!("路径没有父目录：{}", path.display()))?;
        let parent = parent
            .canonicalize()
            .map_err(|error| format!("无法验证父目录 {}：{error}", parent.display()))?;
        parent.join(
            path.file_name()
                .ok_or_else(|| "路径没有文件名".to_owned())?,
        )
    };
    if candidate.starts_with(&root) && candidate != root {
        Ok(())
    } else {
        Err(format!("拒绝操作便携数据目录外的路径：{}", path.display()))
    }
}

fn safe_remove_dir(paths: &Paths, path: &Path) -> Result<(), String> {
    ensure_under(&paths.data, path)?;
    for attempt in 0..20 {
        if !path.exists() {
            return Ok(());
        }
        match fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(_) if attempt < 19 => {
                thread::sleep(Duration::from_millis(250));
            }
            Err(error) => {
                return Err(format!("无法删除目录 {}：{error}", path.display()));
            }
        }
    }
    Ok(())
}

fn parse_version(value: &str) -> Option<Version> {
    let mut build_parts = value.split('+');
    let core_and_build = build_parts.next()?;
    if let Some(build) = build_parts.next() {
        if build.is_empty()
            || build_parts.next().is_some()
            || build.split('.').any(|part| {
                part.is_empty()
                    || !part
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
            })
        {
            return None;
        }
    }
    let (core, prerelease) = core_and_build
        .split_once('-')
        .map_or((core_and_build, ""), |parts| parts);
    let numbers: Vec<&str> = core.split('.').collect();
    if numbers.len() != 3 || numbers.iter().any(|part| !valid_numeric(part)) {
        return None;
    }
    let prerelease = if prerelease.is_empty() {
        Vec::new()
    } else {
        let values: Option<Vec<Identifier>> = prerelease
            .split('.')
            .map(|part| {
                if part.is_empty()
                    || !part
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
                {
                    None
                } else if part.chars().all(|ch| ch.is_ascii_digit()) {
                    if valid_numeric(part) {
                        part.parse().ok().map(Identifier::Number)
                    } else {
                        None
                    }
                } else {
                    Some(Identifier::Text(part.to_owned()))
                }
            })
            .collect();
        values?
    };
    Some(Version {
        major: numbers[0].parse().ok()?,
        minor: numbers[1].parse().ok()?,
        patch: numbers[2].parse().ok()?,
        prerelease,
    })
}

fn valid_numeric(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|ch| ch.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

fn compare_identifiers(left: &[Identifier], right: &[Identifier]) -> std::cmp::Ordering {
    for (left, right) in left.iter().zip(right) {
        let order = match (left, right) {
            (Identifier::Number(a), Identifier::Number(b)) => a.cmp(b),
            (Identifier::Number(_), Identifier::Text(_)) => std::cmp::Ordering::Less,
            (Identifier::Text(_), Identifier::Number(_)) => std::cmp::Ordering::Greater,
            (Identifier::Text(a), Identifier::Text(b)) => a.cmp(b),
        };
        if order != std::cmp::Ordering::Equal {
            return order;
        }
    }
    left.len().cmp(&right.len())
}

fn run_capture(
    paths: &Paths,
    command: &mut Command,
    label: &str,
    timeout: Duration,
    cancellable: bool,
) -> Result<String, String> {
    let nonce = transaction_nonce();
    let stdout_path = paths.logs.join(format!("command-{nonce}.out"));
    let stderr_path = paths.logs.join(format!("command-{nonce}.err"));
    let stdout =
        fs::File::create(&stdout_path).map_err(|error| format!("无法创建命令输出日志：{error}"))?;
    let stderr =
        fs::File::create(&stderr_path).map_err(|error| format!("无法创建命令错误日志：{error}"))?;
    command
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    let mut child = command.spawn().map_err(|error| {
        let _ = fs::remove_file(&stdout_path);
        let _ = fs::remove_file(&stderr_path);
        format!("{label}无法启动：{error}")
    })?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("无法读取{label}状态：{error}"))?
        {
            break status;
        }
        if cancellable && CANCEL.load(Ordering::Acquire) {
            let _ = force_terminate_process_tree(child.id());
            let _ = child.wait();
            let _ = fs::remove_file(&stdout_path);
            let _ = fs::remove_file(&stderr_path);
            return Err("操作已取消".to_owned());
        }
        if Instant::now() >= deadline {
            let _ = force_terminate_process_tree(child.id());
            let _ = child.wait();
            let _ = fs::remove_file(&stdout_path);
            let _ = fs::remove_file(&stderr_path);
            return Err(format!("{label}超过 {} 秒", timeout.as_secs()));
        }
        thread::sleep(Duration::from_millis(100));
    };
    let stdout = fs::read_to_string(&stdout_path).unwrap_or_default();
    let stderr = fs::read_to_string(&stderr_path).unwrap_or_default();
    let _ = fs::remove_file(&stdout_path);
    let _ = fs::remove_file(&stderr_path);
    if status.success() {
        Ok(stdout.trim().to_owned())
    } else {
        let detail = if stderr.trim().is_empty() {
            stdout
        } else {
            stderr
        };
        Err(format!("{label}失败：{}", truncate(detail.trim(), 1200)))
    }
}

fn hidden_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW_FLAG);
    command
}

fn tcp_open(port: u16) -> bool {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    TcpStream::connect_timeout(&address, HEALTH_TIMEOUT).is_ok()
}

fn probe_dsh(paths: &Paths) -> DshProbe {
    probe_dsh_with(paths, probe_url)
}

fn probe_dsh_with<F>(paths: &Paths, probe: F) -> DshProbe
where
    F: Fn(&str) -> ProbeResponse,
{
    let authenticated = logged_web_url(paths);
    match probe(WEB_URL) {
        ProbeResponse::Ready => DshProbe {
            identified: true,
            web_url: Some(WEB_URL.to_owned()),
        },
        ProbeResponse::AuthenticationRequired => {
            let web_url = authenticated.filter(|url| probe(url.as_str()) == ProbeResponse::Ready);
            DshProbe {
                identified: true,
                web_url,
            }
        }
        ProbeResponse::NotDsh => DshProbe {
            identified: false,
            web_url: None,
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProbeResponse {
    Ready,
    AuthenticationRequired,
    NotDsh,
}

fn probe_url(url: &str) -> ProbeResponse {
    let Some(target) = url.strip_prefix("http://127.0.0.1:3080") else {
        return ProbeResponse::NotDsh;
    };
    if !target.starts_with('/')
        || target
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte == b' ')
    {
        return ProbeResponse::NotDsh;
    }
    let address = SocketAddr::from(([127, 0, 0, 1], DSH_PORT));
    let Ok(mut stream) = TcpStream::connect_timeout(&address, HEALTH_TIMEOUT) else {
        return ProbeResponse::NotDsh;
    };
    let _ = stream.set_read_timeout(Some(HEALTH_TIMEOUT));
    let _ = stream.set_write_timeout(Some(HEALTH_TIMEOUT));
    if stream
        .write_all(
            format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:3080\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .is_err()
    {
        return ProbeResponse::NotDsh;
    }
    let mut response = Vec::with_capacity(4096);
    let mut buffer = [0u8; 4096];
    while response.len() < 256 * 1024 {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                response.extend_from_slice(&buffer[..read]);
                if response
                    .windows(DSH_HTML_MARKER.len())
                    .any(|window| window.eq_ignore_ascii_case(DSH_HTML_MARKER.as_bytes()))
                {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(_) => return ProbeResponse::NotDsh,
        }
    }
    classify_dsh_response(&response)
}

fn classify_dsh_response(response: &[u8]) -> ProbeResponse {
    let text = String::from_utf8_lossy(response);
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok());
    if status.is_some_and(|code| (200..300).contains(&code)) && text.contains(DSH_HTML_MARKER) {
        ProbeResponse::Ready
    } else if status == Some(401) && text.contains(DSH_AUTH_MARKER) {
        ProbeResponse::AuthenticationRequired
    } else {
        ProbeResponse::NotDsh
    }
}

#[derive(Default)]
struct AuthLogCache {
    path: PathBuf,
    created: Option<SystemTime>,
    modified: Option<SystemTime>,
    length: u64,
    offset: u64,
    url: Option<String>,
}

fn logged_web_url(paths: &Paths) -> Option<String> {
    static CACHE: OnceLock<Mutex<AuthLogCache>> = OnceLock::new();
    let path = paths.logs.join("dsh.out.log");
    let mut file = fs::File::open(&path).ok()?;
    let metadata = file.metadata().ok()?;
    let mut cache = CACHE
        .get_or_init(|| Mutex::new(AuthLogCache::default()))
        .lock()
        .ok()?;
    let created = metadata.created().ok();
    let modified = metadata.modified().ok();
    let length = metadata.len();
    if cache.path != path
        || cache.created != created
        || length < cache.length
        || (length == cache.length && modified != cache.modified)
    {
        *cache = AuthLogCache {
            path,
            created,
            ..AuthLogCache::default()
        };
    }
    if cache.length != length || cache.modified != modified {
        file.seek(SeekFrom::Start(cache.offset)).ok()?;
        let mut reader = BufReader::new(file.take(length.saturating_sub(cache.offset)));
        let mut line = Vec::new();
        while reader.read_until(b'\n', &mut line).ok()? != 0 {
            let text = String::from_utf8_lossy(&line);
            if let Some(url) = text.trim().strip_prefix("dsh web: ").map(str::trim) {
                if valid_web_url(url) {
                    cache.url = Some(url.to_owned());
                }
            }
            // Reread a partial final line when the writer appends its remainder.
            if !line.ends_with(b"\n") {
                break;
            }
            cache.offset += line.len() as u64;
            line.clear();
        }
        cache.length = length;
        cache.modified = modified;
    }
    cache.url.clone()
}

fn valid_web_url(url: &str) -> bool {
    let Some(suffix) = url.strip_prefix(WEB_URL) else {
        return false;
    };
    if suffix.is_empty() {
        return true;
    }
    let Some(token) = suffix.strip_prefix("?token=") else {
        return false;
    };
    !token.is_empty()
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn open_dsh_web(paths: &Paths) -> Result<String, String> {
    let probe = probe_dsh(paths);
    if let Some(url) = probe.web_url {
        open_url(&url)?;
        Ok("已打开 Web UI".to_owned())
    } else if probe.identified {
        Err("DSH 已运行，但认证 URL 不可用；请停止后由启动器重新启动。".to_owned())
    } else {
        Err("DSH Web UI 尚未就绪".to_owned())
    }
}

fn mark_repair_needed(paths: &Paths, installation: &Installation) {
    if installation.source == Source::Managed {
        let _ = atomic_write(&paths.repair_file(), installation.version.as_bytes());
    }
}

fn read_repair_version(paths: &Paths) -> Option<String> {
    let value = fs::read_to_string(paths.repair_file()).ok()?;
    let value = value.trim();
    parse_version(value).map(|_| value.to_owned())
}

fn clear_repair_needed(paths: &Paths) {
    let _ = fs::remove_file(paths.repair_file());
}

fn read_tracked_pid(paths: &Paths) -> Option<u32> {
    fs::read_to_string(paths.pid_file())
        .ok()?
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|pid| *pid > 0)
}

fn tracked_process_running(paths: &Paths) -> Result<bool, String> {
    let Some(pid) = read_tracked_pid(paths) else {
        return Ok(false);
    };
    if process_running(pid)? {
        Ok(true)
    } else {
        let _ = fs::remove_file(paths.pid_file());
        Ok(false)
    }
}

fn process_running(pid: u32) -> Result<bool, String> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return Ok(false);
    }
    let mut code = 0u32;
    let ok = unsafe { GetExitCodeProcess(handle, &mut code) } != 0;
    unsafe { CloseHandle(handle) };
    if ok {
        Ok(code == STILL_ACTIVE_EXIT_CODE)
    } else {
        Err(format!("无法读取进程 {pid} 状态"))
    }
}

fn find_external_dsh_pid(port: u16) -> Result<Option<u32>, String> {
    let output = hidden_command("netstat.exe")
        .args(["-ano", "-p", "tcp"])
        .output()
        .map_err(|error| format!("无法执行 netstat：{error}"))?;
    if !output.status.success() {
        return Err("netstat 无法验证 3080 端口进程".to_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| parse_listening_pid(line, port)))
}

fn parse_listening_pid(line: &str, port: u16) -> Option<u32> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 5 || fields[0] != "TCP" || fields[3] != "LISTENING" {
        return None;
    }
    let local_port = fields[1].rsplit(':').next()?.parse::<u16>().ok()?;
    (local_port == port)
        .then(|| fields[4].parse::<u32>().ok())
        .flatten()
}

fn process_command_line(pid: u32) -> Result<Option<String>, String> {
    let script = format!(
        "$p=Get-CimInstance Win32_Process -Filter 'ProcessId={pid}' -ErrorAction Stop; [Console]::Out.Write($p.CommandLine)"
    );
    let output = hidden_command("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &script,
        ])
        .output()
        .map_err(|error| format!("无法查询进程命令行：{error}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((!value.is_empty()).then_some(value))
}

fn is_dsh_command(command_line: &str, port: u16) -> bool {
    let lower = command_line.to_ascii_lowercase().replace('/', "\\");
    let entry = lower.contains("@deepseek-ai\\dsh\\lib\\bin.js");
    let arguments: Vec<&str> = lower
        .split_whitespace()
        .map(|part| part.trim_matches('"'))
        .collect();
    let web = arguments.contains(&"web");
    let mut explicit_port: Option<u16> = None;
    for (index, argument) in arguments.iter().enumerate() {
        if *argument == "--port" {
            explicit_port = arguments
                .get(index + 1)
                .and_then(|value| value.parse().ok());
        } else if let Some(value) = argument.strip_prefix("--port=") {
            explicit_port = value.parse().ok();
        }
    }
    entry && web && explicit_port.map_or(port == DSH_PORT, |value| value == port)
}

fn terminate_pid(pid: u32) -> Result<bool, String> {
    let _ = hidden_command("taskkill.exe")
        .args(["/PID", &pid.to_string(), "/T"])
        .output();
    let deadline = Instant::now() + STOP_TIMEOUT;
    while Instant::now() < deadline {
        if !process_running(pid)? {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(200));
    }
    force_terminate_process_tree(pid)?;
    Ok(true)
}

fn force_terminate_process_tree(pid: u32) -> Result<(), String> {
    let _ = hidden_command("taskkill.exe")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .output();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if !process_running(pid)? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    if handle.is_null() {
        return Err(format!("无法打开进程 {pid} 进行强制停止"));
    }
    let ok = unsafe { TerminateProcess(handle, 1) } != 0;
    unsafe { CloseHandle(handle) };
    if ok {
        Ok(())
    } else {
        Err(format!("无法停止进程 {pid}"))
    }
}

fn rotate_log(path: &Path) -> Result<(), String> {
    if path.metadata().map(|value| value.len()).unwrap_or(0) < LOG_LIMIT {
        return Ok(());
    }
    for index in (1..LOG_COPIES).rev() {
        let source = rotated_path(path, index);
        let target = rotated_path(path, index + 1);
        if target.exists() {
            let _ = fs::remove_file(&target);
        }
        if source.exists() {
            fs::rename(&source, &target)
                .map_err(|error| format!("无法轮转日志 {}：{error}", source.display()))?;
        }
    }
    fs::rename(path, rotated_path(path, 1))
        .map_err(|error| format!("无法轮转日志 {}：{error}", path.display()))
}

fn rotated_path(path: &Path, index: usize) -> PathBuf {
    path.with_extension(format!("log.{index}"))
}

fn append_log(path: &Path, message: &str) {
    let _ = rotate_log(path);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{} {message}", transaction_nonce());
    }
}

fn transaction_nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn open_url(url: &str) -> Result<(), String> {
    let operation = to_wide("open");
    let url = to_wide(url);
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            operation.as_ptr(),
            url.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if result > 32 {
        Ok(())
    } else {
        Err("无法打开系统浏览器".to_owned())
    }
}

fn check_launcher_update(paths: &Paths) -> Result<String, String> {
    let script = format!(
        "$ProgressPreference='SilentlyContinue';$r=Invoke-RestMethod -UseBasicParsing -TimeoutSec 15 -Headers @{{'User-Agent'='DSH-Launcher'}} -Uri '{}';[Console]::Out.Write($r.tag_name)",
        RELEASE_API_URL
    );
    let mut command = hidden_command("powershell.exe");
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &script,
        ])
        .env("TEMP", &paths.temp)
        .env("TMP", &paths.temp);
    let output = run_capture(
        paths,
        &mut command,
        "检查启动器更新",
        LAUNCHER_QUERY_TIMEOUT,
        false,
    )?;
    let latest_text = output.trim().trim_start_matches('v').to_owned();
    let current =
        parse_version(APP_VERSION).ok_or_else(|| format!("启动器版本无效：{APP_VERSION}"))?;
    let latest =
        parse_version(&latest_text).ok_or_else(|| format!("Release 版本无效：{latest_text}"))?;
    if latest > current {
        open_url(RELEASE_PAGE_URL)?;
        Ok(format!("发现启动器 v{latest_text}，已打开 Release 页面"))
    } else {
        Ok(format!("启动器已是最新版本 · v{APP_VERSION}"))
    }
}

fn acquire_action_mutex() -> Option<MutexGuard> {
    create_mutex(ACTION_MUTEX)
}

fn acquire_single_instance() -> Option<MutexGuard> {
    let executable = env::current_exe().ok()?;
    let normalized = executable.to_string_lossy().to_ascii_lowercase();
    let mut hash = 0xcbf29ce484222325u64;
    for byte in normalized.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    let name = format!("Local\\DeepSeek.DSHLauncher.{hash:016x}");
    let guard = create_mutex(&name);
    if guard.is_none() {
        let class = to_wide(WINDOW_CLASS);
        let existing = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
        if !existing.is_null() {
            unsafe { PostMessageW(existing, SHOW_MESSAGE, 0, 0) };
        }
    }
    guard
}

fn create_mutex(name: &str) -> Option<MutexGuard> {
    let name = to_wide(name);
    let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
    if handle.is_null() {
        return None;
    }
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        unsafe { CloseHandle(handle) };
        None
    } else {
        Some(MutexGuard(handle))
    }
}

fn run_app() -> Result<(), String> {
    let paths = app_paths()?;
    if let Some(_guard) = acquire_action_mutex() {
        recover_for_use(&paths)?;
        cleanup_npm_cache(&paths);
    } else {
        append_log(
            &paths.logs.join("launcher.log"),
            "启动时检测到另一个 DSH 操作，已跳过更新目录清理",
        );
    }
    let snapshot = refresh_discovery(&paths);
    let initial_status = status_for_snapshot(&snapshot);
    let hinstance = unsafe { GetModuleHandleW(std::ptr::null()) };
    if hinstance.is_null() {
        return Err("无法获取程序模块".to_owned());
    }
    unsafe {
        let app_id = to_wide(APP_USER_MODEL_ID);
        let _ = SetCurrentProcessExplicitAppUserModelID(app_id.as_ptr());
    }
    let blue = unsafe { load_icon(hinstance, ICON_BLUE) };
    let black = unsafe { load_icon(hinstance, ICON_BLACK) };
    if blue.is_null() || black.is_null() {
        return Err("启动器图标资源缺失；请使用完整 Release 包。".to_owned());
    }
    let high_contrast = is_high_contrast();
    let background = unsafe {
        CreateSolidBrush(if high_contrast {
            GetSysColor(COLOR_WINDOW)
        } else {
            BG
        })
    };
    let control_background = unsafe {
        CreateSolidBrush(if high_contrast {
            GetSysColor(COLOR_WINDOW)
        } else {
            BG
        })
    };
    let taskbar_created = unsafe {
        let value = to_wide("TaskbarCreated");
        RegisterWindowMessageW(value.as_ptr())
    };
    let state = Arc::new(AppState {
        paths,
        blue_icon: AtomicUsize::new(blue as usize),
        black_icon: AtomicUsize::new(black as usize),
        tray_icon: AtomicUsize::new(if snapshot.healthy { blue } else { black } as usize),
        tray_added: AtomicBool::new(false),
        taskbar_created,
        background_brush: background as usize,
        control_background_brush: AtomicUsize::new(control_background as usize),
        title_font: AtomicUsize::new(0),
        body_font: AtomicUsize::new(0),
        small_font: AtomicUsize::new(0),
        snapshot: Mutex::new(snapshot),
        status: Mutex::new(initial_status),
        messages: Mutex::new(VecDeque::new()),
        busy: AtomicBool::new(false),
        cancelable: AtomicBool::new(false),
        health_checking: AtomicBool::new(false),
        close_notice_shown: AtomicBool::new(false),
        high_contrast: AtomicBool::new(high_contrast),
    });
    unsafe { create_main_window(hinstance, state) }
}

unsafe fn create_main_window(hinstance: *mut c_void, state: Arc<AppState>) -> Result<(), String> {
    let class_name = to_wide(WINDOW_CLASS);
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfnWndProc: Some(window_proc),
        hInstance: hinstance,
        hIcon: state.black_icon.load(Ordering::Acquire) as HICON,
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        hbrBackground: state.background_brush as *mut c_void,
        lpszClassName: class_name.as_ptr(),
        hIconSm: state.black_icon.load(Ordering::Acquire) as HICON,
        ..WNDCLASSEXW::default()
    };
    if RegisterClassExW(&class) == 0 {
        return Err(format!("无法注册窗口：{}", GetLastError()));
    }
    let title = to_wide(WINDOW_TITLE);
    let state_ptr = Box::into_raw(Box::new(state));
    let extended_style = WS_EX_APPWINDOW | WS_EX_CONTROLPARENT;
    let window_style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
    let dpi = GetDpiForSystem().max(96);
    let (client_width, client_height) = desired_client_size(dpi);
    let mut window_rect = RECT {
        left: 0,
        top: 0,
        right: client_width,
        bottom: client_height,
    };
    if AdjustWindowRectExForDpi(&mut window_rect, window_style, 0, extended_style, dpi) == 0 {
        drop(Box::from_raw(state_ptr));
        return Err(format!("无法计算窗口尺寸：{}", GetLastError()));
    }
    let hwnd = CreateWindowExW(
        extended_style,
        class_name.as_ptr(),
        title.as_ptr(),
        window_style,
        200,
        160,
        window_rect.right - window_rect.left,
        window_rect.bottom - window_rect.top,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        hinstance,
        state_ptr.cast::<c_void>(),
    );
    if hwnd.is_null() {
        drop(Box::from_raw(state_ptr));
        return Err(format!("无法创建窗口：{}", GetLastError()));
    }
    ShowWindow(hwnd, SW_SHOW);
    let mut message = MSG::default();
    while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
        if IsDialogMessageW(hwnd, &message) == 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    if let Some(state) = state_for(hwnd) {
        if state.taskbar_created != 0 && message == state.taskbar_created {
            add_tray(hwnd, &state);
            return 0;
        }
    }
    match message {
        WM_NCCREATE => 1,
        WM_CREATE => {
            let Some(state) = state_for(hwnd) else {
                return -1;
            };
            set_window_icon(hwnd, state.tray_icon.load(Ordering::Acquire) as HICON);
            set_text(hwnd, WINDOW_TITLE);
            create_controls(hwnd);
            recreate_fonts(hwnd, &state);
            apply_window_style(hwnd);
            layout(hwnd);
            refresh_controls(hwnd, &state);
            if add_tray(hwnd, &state) == 0 {
                SetTimer(hwnd, TIMER_TRAY_RETRY, 1000, None);
            }
            SetTimer(hwnd, TIMER_HEALTH, HEALTH_INTERVAL_MS, None);
            schedule_health(hwnd, state);
            0
        }
        WM_SIZE => {
            layout(hwnd);
            0
        }
        WM_DPICHANGED => {
            if let Some(state) = state_for(hwnd) {
                let suggested = &*(lparam as *const RECT);
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                recreate_fonts(hwnd, &state);
                layout(hwnd);
            }
            0
        }
        WM_SETTINGCHANGE => {
            if let Some(state) = state_for(hwnd) {
                let high_contrast = is_high_contrast();
                state.high_contrast.store(high_contrast, Ordering::Release);
                let brush = CreateSolidBrush(if high_contrast {
                    GetSysColor(COLOR_WINDOW)
                } else {
                    BG
                });
                replace_brush(&state.control_background_brush, brush);
                recreate_fonts(hwnd, &state);
                InvalidateRect(hwnd, std::ptr::null(), 1);
            }
            0
        }
        WM_COMMAND => {
            let command = (wparam & 0xffff) as u32;
            match command {
                CMD_MAIN => handle_main_button(hwnd),
                CMD_WEB => {
                    let result = state_for(hwnd)
                        .ok_or_else(|| "启动器状态不可用".to_owned())
                        .and_then(|state| open_dsh_web(&state.paths));
                    if let Err(error) = result {
                        show_error_box(hwnd, &error);
                    }
                }
                CMD_UPDATE_DSH => request_operation(hwnd, Operation::Upgrade),
                CMD_CHECK_LAUNCHER => request_launcher_check(hwnd),
                CMD_SHOW => show_main_window(hwnd),
                CMD_EXIT
                    if state_for(hwnd).is_some_and(|state| !state.busy.load(Ordering::Acquire)) =>
                {
                    DestroyWindow(hwnd);
                }
                CMD_EXIT => {}
                _ => {}
            }
            0
        }
        WM_KEYDOWN if wparam as u16 == VK_ESCAPE => {
            if let Some(state) = state_for(hwnd) {
                hide_main_window(hwnd, &state);
            }
            0
        }
        WM_DRAWITEM => {
            let item = lparam as *const DRAWITEMSTRUCT;
            if !item.is_null() {
                if let Some(state) = state_for(hwnd) {
                    draw_button(&state, &*item);
                    return 1;
                }
            }
            0
        }
        WM_CTLCOLORSTATIC => {
            if let Some(state) = state_for(hwnd) {
                let hdc = wparam as *mut c_void;
                let id = GetDlgCtrlID(lparam as HWND) as u32;
                let high_contrast = state.high_contrast.load(Ordering::Acquire);
                SetBkMode(hdc, OPAQUE as i32);
                let color = if high_contrast {
                    GetSysColor(COLOR_WINDOWTEXT)
                } else if id == ID_TITLE {
                    TEXT
                } else {
                    MUTED
                };
                let background = if high_contrast {
                    GetSysColor(COLOR_WINDOW)
                } else {
                    BG
                };
                SetBkColor(hdc, background);
                SetTextColor(hdc, color);
                return state.control_background_brush.load(Ordering::Acquire) as isize;
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut paint);
            let mut rect = RECT::default();
            GetClientRect(hwnd, &mut rect);
            if let Some(state) = state_for(hwnd) {
                let color = if state.high_contrast.load(Ordering::Acquire) {
                    GetSysColor(COLOR_WINDOW)
                } else {
                    BG
                };
                let brush = CreateSolidBrush(color);
                FillRect(hdc, &rect, brush);
                DeleteObject(brush);
            }
            EndPaint(hwnd, &paint);
            0
        }
        UI_MESSAGE => {
            if let Some(state) = state_for(hwnd) {
                if let Some(next) = state
                    .messages
                    .lock()
                    .ok()
                    .and_then(|mut queue| queue.pop_front())
                {
                    if let Ok(mut status) = state.status.lock() {
                        *status = next;
                    }
                }
                refresh_controls(hwnd, &state);
            }
            0
        }
        WM_TIMER if wparam == TIMER_TRAY_RETRY => {
            if let Some(state) = state_for(hwnd) {
                if add_tray(hwnd, &state) != 0 {
                    KillTimer(hwnd, TIMER_TRAY_RETRY);
                }
            }
            0
        }
        WM_TIMER if wparam == TIMER_HEALTH => {
            if IsWindowVisible(hwnd) != 0 {
                if let Some(state) = state_for(hwnd) {
                    schedule_health(hwnd, state);
                }
            } else {
                KillTimer(hwnd, TIMER_HEALTH);
            }
            0
        }
        TRAY_MESSAGE => {
            let event = tray_event(lparam);
            if matches!(
                event,
                WM_LBUTTONUP | WM_LBUTTONDBLCLK | NIN_SELECT | NIN_KEYSELECT
            ) {
                show_main_window(hwnd);
            } else if matches!(event, WM_RBUTTONUP | WM_CONTEXTMENU) {
                if let Some(state) = state_for(hwnd) {
                    refresh_local_state(hwnd, &state);
                    show_tray_menu(hwnd, &state);
                }
            }
            0
        }
        SHOW_MESSAGE => {
            show_main_window(hwnd);
            0
        }
        WM_CLOSE => {
            if let Some(state) = state_for(hwnd) {
                hide_main_window(hwnd, &state);
            }
            0
        }
        WM_DESTROY => {
            if let Some(state) = state_for(hwnd) {
                KillTimer(hwnd, TIMER_TRAY_RETRY);
                KillTimer(hwnd, TIMER_HEALTH);
                if state.tray_added.swap(false, Ordering::AcqRel) {
                    delete_tray(hwnd);
                }
                for font in [
                    state.title_font.load(Ordering::Acquire),
                    state.body_font.load(Ordering::Acquire),
                    state.small_font.load(Ordering::Acquire),
                ] {
                    if font != 0 {
                        DeleteObject(font as *mut c_void);
                    }
                }
                if state.background_brush != 0 {
                    DeleteObject(state.background_brush as *mut c_void);
                }
                let control_background = state.control_background_brush.load(Ordering::Acquire);
                if control_background != 0 && control_background != state.background_brush {
                    DeleteObject(control_background as *mut c_void);
                }
            }
            let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            if pointer != 0 {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(pointer as *mut Arc<AppState>));
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn create_controls(hwnd: HWND) {
    create_control(hwnd, "STATIC", WINDOW_TITLE, ID_TITLE, SS_CENTER, false);
    create_control(
        hwnd,
        "STATIC",
        "正在检查本机状态...",
        ID_STATUS,
        SS_CENTER,
        false,
    );
    create_control(
        hwnd,
        "BUTTON",
        "启动 DSH",
        CMD_MAIN,
        BS_OWNERDRAW as u32,
        true,
    );
    create_control(
        hwnd,
        "BUTTON",
        "打开 Web UI",
        CMD_WEB,
        BS_OWNERDRAW as u32,
        true,
    );
    create_control(
        hwnd,
        "BUTTON",
        "更新 DSH",
        CMD_UPDATE_DSH,
        BS_OWNERDRAW as u32,
        true,
    );
    create_control(
        hwnd,
        "BUTTON",
        &format!("v{APP_VERSION} · 检查启动器更新"),
        CMD_CHECK_LAUNCHER,
        BS_OWNERDRAW as u32,
        true,
    );
}

unsafe fn create_control(
    parent: HWND,
    class: &str,
    text: &str,
    id: u32,
    style: u32,
    tab: bool,
) -> HWND {
    let class = to_wide(class);
    let text = to_wide(text);
    CreateWindowExW(
        0,
        class.as_ptr(),
        text.as_ptr(),
        WS_CHILD | WS_VISIBLE | style | if tab { WS_TABSTOP } else { 0 },
        0,
        0,
        1,
        1,
        parent,
        id as usize as *mut c_void,
        GetModuleHandleW(std::ptr::null()),
        std::ptr::null_mut(),
    )
}

unsafe fn recreate_fonts(hwnd: HWND, state: &AppState) {
    let dpi = GetDpiForWindow(hwnd).max(96);
    let title = create_font(scale(24, dpi), FW_SEMIBOLD as i32);
    let body = create_font(scale(15, dpi), FW_NORMAL as i32);
    let small = create_font(scale(12, dpi), FW_NORMAL as i32);
    replace_font(&state.title_font, title);
    replace_font(&state.body_font, body);
    replace_font(&state.small_font, small);
    SendMessageW(
        GetDlgItem(hwnd, ID_TITLE as i32),
        WM_SETFONT,
        title as usize,
        1,
    );
    for id in [ID_STATUS, CMD_MAIN, CMD_WEB, CMD_UPDATE_DSH] {
        SendMessageW(GetDlgItem(hwnd, id as i32), WM_SETFONT, body as usize, 1);
    }
    SendMessageW(
        GetDlgItem(hwnd, CMD_CHECK_LAUNCHER as i32),
        WM_SETFONT,
        small as usize,
        1,
    );
}

unsafe fn create_font(height: i32, weight: i32) -> *mut c_void {
    let family = to_wide(FONT_FAMILY);
    CreateFontW(
        -height,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        GB2312_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        0,
        5,
        (DEFAULT_PITCH | FF_DONTCARE).into(),
        family.as_ptr(),
    )
}

unsafe fn replace_font(slot: &AtomicUsize, new_font: *mut c_void) {
    let old = slot.swap(new_font as usize, Ordering::AcqRel);
    if old != 0 {
        DeleteObject(old as *mut c_void);
    }
}

unsafe fn replace_brush(slot: &AtomicUsize, new_brush: *mut c_void) {
    let old = slot.swap(new_brush as usize, Ordering::AcqRel);
    if old != 0 {
        DeleteObject(old as *mut c_void);
    }
}

unsafe fn layout(hwnd: HWND) {
    let dpi = GetDpiForWindow(hwnd).max(96);
    let mut client = RECT::default();
    GetClientRect(hwnd, &mut client);
    let width = client.right - client.left;
    let margin = scale(34, dpi);
    let content = width - margin * 2;
    move_control(
        hwnd,
        ID_TITLE,
        margin,
        scale(28, dpi),
        content,
        scale(42, dpi),
    );
    move_control(
        hwnd,
        ID_STATUS,
        margin,
        scale(78, dpi),
        content,
        scale(48, dpi),
    );
    move_control(
        hwnd,
        CMD_MAIN,
        margin,
        scale(144, dpi),
        content,
        scale(48, dpi),
    );
    let gap = scale(12, dpi);
    let half = (content - gap) / 2;
    move_control(hwnd, CMD_WEB, margin, scale(208, dpi), half, scale(44, dpi));
    move_control(
        hwnd,
        CMD_UPDATE_DSH,
        margin + half + gap,
        scale(208, dpi),
        half,
        scale(44, dpi),
    );
    move_control(
        hwnd,
        CMD_CHECK_LAUNCHER,
        margin,
        scale(284, dpi),
        content,
        scale(28, dpi),
    );
}

unsafe fn move_control(hwnd: HWND, id: u32, x: i32, y: i32, width: i32, height: i32) {
    SetWindowPos(
        GetDlgItem(hwnd, id as i32),
        std::ptr::null_mut(),
        x,
        y,
        width,
        height,
        SWP_NOZORDER | SWP_NOACTIVATE,
    );
}

fn scale(value: i32, dpi: u32) -> i32 {
    value * dpi as i32 / 96
}

fn desired_client_size(dpi: u32) -> (i32, i32) {
    (scale(CLIENT_WIDTH, dpi), scale(CLIENT_HEIGHT, dpi))
}

unsafe fn draw_button(state: &AppState, item: &DRAWITEMSTRUCT) {
    let disabled = item.itemState & ODS_DISABLED != 0;
    let selected = item.itemState & ODS_SELECTED != 0;
    let footer = item.CtlID == CMD_CHECK_LAUNCHER;
    let primary = item.CtlID == CMD_MAIN;
    let high_contrast = state.high_contrast.load(Ordering::Acquire);
    let fill = if high_contrast {
        GetSysColor(COLOR_BTNFACE)
    } else if footer {
        BG
    } else if disabled {
        rgb(233, 237, 244)
    } else if primary {
        if selected {
            BLUE_DARK
        } else {
            BLUE
        }
    } else if selected {
        rgb(235, 240, 250)
    } else {
        SURFACE
    };
    let text_color = if high_contrast {
        if disabled {
            GetSysColor(COLOR_GRAYTEXT)
        } else {
            GetSysColor(COLOR_BTNTEXT)
        }
    } else if disabled {
        DISABLED
    } else if footer {
        BLUE_DARK
    } else if primary {
        rgb(255, 255, 255)
    } else {
        TEXT
    };
    let brush = CreateSolidBrush(fill);
    if footer {
        FillRect(item.hDC, &item.rcItem, brush);
    } else {
        SelectObject(item.hDC, brush);
        SelectObject(item.hDC, GetStockObject(DC_PEN));
        SetDCPenColor(item.hDC, if high_contrast { text_color } else { BORDER });
        RoundRect(
            item.hDC,
            item.rcItem.left,
            item.rcItem.top,
            item.rcItem.right,
            item.rcItem.bottom,
            12,
            12,
        );
    }
    DeleteObject(brush);
    let mut text = [0u16; 128];
    let length = windows_sys::Win32::UI::WindowsAndMessaging::GetWindowTextW(
        item.hwndItem,
        text.as_mut_ptr(),
        text.len() as i32,
    );
    SetBkMode(item.hDC, TRANSPARENT as i32);
    SetTextColor(item.hDC, text_color);
    let mut rect = item.rcItem;
    DrawTextW(
        item.hDC,
        text.as_ptr(),
        length,
        &mut rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    if item.itemState & ODS_FOCUS != 0 {
        let mut focus = item.rcItem;
        focus.left += 4;
        focus.top += 4;
        focus.right -= 4;
        focus.bottom -= 4;
        DrawFocusRect(item.hDC, &focus);
    }
}

unsafe fn refresh_controls(hwnd: HWND, state: &AppState) {
    let snapshot = state
        .snapshot
        .lock()
        .map(|value| value.clone())
        .unwrap_or(Snapshot {
            installation: None,
            node_available: false,
            npm_available: false,
            running: false,
            healthy: false,
            auth_unavailable: false,
            repair_needed: false,
            discovery_error: None,
        });
    let busy = state.busy.load(Ordering::Acquire);
    let kind = main_button(&snapshot, busy, state.cancelable.load(Ordering::Acquire));
    let label = match kind {
        MainButton::Start => "启动 DSH",
        MainButton::Stop => "停止 DSH",
        MainButton::InstallDsh => "安装 DSH",
        MainButton::InstallNode => "安装 Node.js",
        MainButton::RepairDsh => "重新安装 DSH",
        MainButton::Cancel => "取消",
        MainButton::Busy => "正在处理...",
    };
    set_text(GetDlgItem(hwnd, CMD_MAIN as i32), label);
    EnableWindow(
        GetDlgItem(hwnd, CMD_MAIN as i32),
        (kind != MainButton::Busy) as i32,
    );
    EnableWindow(
        GetDlgItem(hwnd, CMD_WEB as i32),
        (!busy && snapshot.healthy) as i32,
    );
    let update = GetDlgItem(hwnd, CMD_UPDATE_DSH as i32);
    ShowWindow(
        update,
        if snapshot.installation.is_some() {
            SW_SHOW
        } else {
            SW_HIDE
        },
    );
    EnableWindow(
        update,
        (!busy && snapshot.installation.is_some() && snapshot.npm_available) as i32,
    );
    EnableWindow(GetDlgItem(hwnd, CMD_CHECK_LAUNCHER as i32), (!busy) as i32);
    let status = state
        .status
        .lock()
        .map(|value| value.clone())
        .unwrap_or_default();
    set_text(GetDlgItem(hwnd, ID_STATUS as i32), &status);
    let icon = if snapshot.healthy {
        state.blue_icon.load(Ordering::Acquire)
    } else {
        state.black_icon.load(Ordering::Acquire)
    };
    state.tray_icon.store(icon, Ordering::Release);
    set_window_icon(hwnd, icon as HICON);
    update_tray(hwnd, icon as HICON, &status);
    for id in [CMD_MAIN, CMD_WEB, CMD_UPDATE_DSH, CMD_CHECK_LAUNCHER] {
        InvalidateRect(GetDlgItem(hwnd, id as i32), std::ptr::null(), 1);
    }
}

unsafe fn handle_main_button(hwnd: HWND) {
    let Some(state) = state_for(hwnd) else {
        return;
    };
    let snapshot = state.snapshot.lock().map(|value| value.clone()).unwrap();
    match main_button(
        &snapshot,
        state.busy.load(Ordering::Acquire),
        state.cancelable.load(Ordering::Acquire),
    ) {
        MainButton::Start => request_operation(hwnd, Operation::Start),
        MainButton::Stop => request_operation(hwnd, Operation::Stop),
        MainButton::InstallDsh => request_operation(hwnd, Operation::Install),
        MainButton::InstallNode => {
            let _ = open_url(NODE_DOWNLOAD_URL);
            show_info_box(
                hwnd,
                "请安装 Node.js LTS。安装完成后，请在托盘菜单选择“退出”，再重新运行启动器；仅关闭或重新打开面板不会刷新 Node.js 和 npm 的安装路径。",
            );
        }
        MainButton::RepairDsh => request_operation(hwnd, Operation::Upgrade),
        MainButton::Cancel => {
            CANCEL.store(true, Ordering::Release);
            push_status(hwnd, &state, "正在取消...".to_owned());
        }
        MainButton::Busy => {}
    }
}

unsafe fn request_operation(hwnd: HWND, operation: Operation) {
    let Some(state) = state_for(hwnd) else {
        return;
    };
    if state.busy.swap(true, Ordering::AcqRel) {
        return;
    }
    let cancelable = matches!(operation, Operation::Install | Operation::Upgrade);
    state.cancelable.store(cancelable, Ordering::Release);
    CANCEL.store(false, Ordering::Release);
    refresh_controls(hwnd, &state);
    let hwnd_value = hwnd as usize;
    thread::spawn(move || {
        let _guard = match acquire_action_mutex() {
            Some(value) => value,
            None => {
                finish_operation(
                    hwnd_value as HWND,
                    &state,
                    Err("已有启动器操作正在执行".to_owned()),
                );
                return;
            }
        };
        let progress = |message: &str, cancelable: bool| {
            state.cancelable.store(cancelable, Ordering::Release);
            push_status(hwnd_value as HWND, &state, message.to_owned());
        };
        let result = recover_for_use(&state.paths).and_then(|_| match operation {
            Operation::Start => start_dsh(),
            Operation::Stop => stop_dsh(),
            Operation::Install => {
                let confirm = |message: &str| unsafe { confirm_box(hwnd_value as HWND, message) };
                install_or_update(true, &progress, Some(&confirm))
            }
            Operation::Upgrade => {
                let confirm = |message: &str| unsafe { confirm_box(hwnd_value as HWND, message) };
                install_or_update(false, &progress, Some(&confirm))
            }
        });
        finish_operation(hwnd_value as HWND, &state, result);
    });
}

fn finish_operation(hwnd: HWND, state: &Arc<AppState>, result: Result<String, String>) {
    state.busy.store(false, Ordering::Release);
    state.cancelable.store(false, Ordering::Release);
    CANCEL.store(false, Ordering::Release);
    let snapshot = refresh_discovery(&state.paths);
    if let Ok(mut current) = state.snapshot.lock() {
        *current = snapshot;
    }
    match result {
        Ok(message) => {
            push_status(hwnd, state, message.clone());
            unsafe { notify(hwnd, "DSH 操作完成", &message, false) };
        }
        Err(error) => {
            append_log(&state.paths.logs.join("launcher.log"), &error);
            push_status(hwnd, state, format!("操作失败：{}", first_line(&error)));
            unsafe {
                notify(hwnd, "DSH 操作失败", first_line(&error), true);
                show_error_box(
                    hwnd,
                    &format!(
                        "{error}\n\n日志：{}\n可按 Ctrl+C 复制此对话框内容。",
                        state.paths.logs.join("launcher.log").display()
                    ),
                );
            }
        }
    }
    unsafe { refresh_controls(hwnd, state) };
}

unsafe fn request_launcher_check(hwnd: HWND) {
    let Some(state) = state_for(hwnd) else {
        return;
    };
    if state.busy.swap(true, Ordering::AcqRel) {
        return;
    }
    refresh_controls(hwnd, &state);
    push_status(hwnd, &state, "正在检查启动器更新...".to_owned());
    let hwnd_value = hwnd as usize;
    thread::spawn(move || {
        let result = check_launcher_update(&state.paths);
        state.busy.store(false, Ordering::Release);
        match result {
            Ok(message) => push_status(hwnd_value as HWND, &state, message),
            Err(error) => {
                append_log(&state.paths.logs.join("launcher.log"), &error);
                push_status(hwnd_value as HWND, &state, format!("检查失败：{error}"));
            }
        }
    });
}

fn push_status(hwnd: HWND, state: &AppState, message: String) {
    if let Ok(mut queue) = state.messages.lock() {
        queue.push_back(message);
        while queue.len() > 8 {
            queue.pop_front();
        }
    }
    unsafe { PostMessageW(hwnd, UI_MESSAGE, 0, 0) };
}

unsafe fn schedule_health(hwnd: HWND, state: Arc<AppState>) {
    let visible = IsWindowVisible(hwnd) != 0;
    let busy = state.busy.load(Ordering::Acquire);
    let checking = state.health_checking.swap(true, Ordering::AcqRel);
    if !health_check_allowed(visible, busy, checking) {
        if !checking {
            state.health_checking.store(false, Ordering::Release);
        }
        return;
    }
    let hwnd_value = hwnd as usize;
    thread::spawn(move || {
        let mut snapshot = state.snapshot.lock().map(|value| value.clone()).unwrap();
        let tracked = tracked_process_running(&state.paths).unwrap_or(false);
        let probe = probe_dsh(&state.paths);
        snapshot.healthy = probe.web_url.is_some();
        snapshot.auth_unavailable = probe.identified && !snapshot.healthy;
        snapshot.running = tracked || probe.identified;
        let status = status_for_snapshot(&snapshot);
        if let Ok(mut current) = state.snapshot.lock() {
            *current = snapshot;
        }
        state.health_checking.store(false, Ordering::Release);
        if unsafe { IsWindowVisible(hwnd_value as HWND) } != 0 {
            push_status(hwnd_value as HWND, &state, status);
        }
    });
}

fn health_check_allowed(visible: bool, busy: bool, checking: bool) -> bool {
    visible && !busy && !checking
}

fn status_for_snapshot(snapshot: &Snapshot) -> String {
    if snapshot.healthy {
        let version = snapshot
            .installation
            .as_ref()
            .map(|value| format!(" · DSH {}", value.version))
            .unwrap_or_default();
        format!("运行中{version} · {WEB_URL}")
    } else if snapshot.auth_unavailable {
        "DSH 已运行，但认证 URL 不可用；请停止后由启动器重新启动。".to_owned()
    } else if snapshot.running {
        "DSH 进程存在，但 Web UI 未就绪".to_owned()
    } else if let Some(error) = &snapshot.discovery_error {
        format!("DSH 安装需要修复 · {}", first_line(error))
    } else if snapshot.repair_needed {
        "DSH 最新版本需要重新安装".to_owned()
    } else if let Some(installation) = &snapshot.installation {
        format!("已停止 · DSH {}", installation.version)
    } else if snapshot.node_available && snapshot.npm_available {
        "未安装 DSH".to_owned()
    } else {
        "未找到 Node.js / npm".to_owned()
    }
}

unsafe fn refresh_local_state(hwnd: HWND, state: &AppState) {
    let snapshot = refresh_discovery(&state.paths);
    let status = status_for_snapshot(&snapshot);
    if let Ok(mut current) = state.snapshot.lock() {
        *current = snapshot;
    }
    if let Ok(mut current) = state.status.lock() {
        *current = status;
    }
    refresh_controls(hwnd, state);
}

unsafe fn show_main_window(hwnd: HWND) {
    ShowWindow(hwnd, SW_RESTORE);
    ShowWindow(hwnd, SW_SHOW);
    SetForegroundWindow(hwnd);
    if let Some(state) = state_for(hwnd) {
        refresh_local_state(hwnd, &state);
        SetTimer(hwnd, TIMER_HEALTH, HEALTH_INTERVAL_MS, None);
        schedule_health(hwnd, state);
    }
}

unsafe fn hide_main_window(hwnd: HWND, state: &AppState) {
    if !state.close_notice_shown.swap(true, Ordering::AcqRel) {
        notify(
            hwnd,
            "DSH启动器已隐藏",
            "左键鲸鱼图标可重新打开，右键可使用快捷菜单。",
            false,
        );
    }
    KillTimer(hwnd, TIMER_HEALTH);
    ShowWindow(hwnd, SW_HIDE);
}

unsafe fn show_tray_menu(hwnd: HWND, state: &AppState) {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    let snapshot = state.snapshot.lock().map(|value| value.clone()).unwrap();
    let dynamic = if snapshot.running {
        "停止 DSH"
    } else if snapshot.repair_needed {
        "重新安装 DSH"
    } else {
        "启动 DSH"
    };
    let entries = [
        (CMD_SHOW, "打开面板"),
        (CMD_MAIN, dynamic),
        (CMD_WEB, "打开 Web UI"),
        (CMD_EXIT, "退出"),
    ];
    let busy = state.busy.load(Ordering::Acquire);
    for (index, (command, label)) in entries.iter().enumerate() {
        if index == 3 {
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        }
        let disabled = match *command {
            CMD_SHOW => false,
            CMD_EXIT => busy,
            CMD_WEB => busy || !snapshot.healthy,
            CMD_MAIN => {
                busy || (snapshot.installation.is_none() && !snapshot.repair_needed)
                    || (snapshot.repair_needed && !snapshot.npm_available)
            }
            _ => busy,
        };
        let label = to_wide(label);
        AppendMenuW(
            menu,
            MF_STRING | if disabled { MF_GRAYED } else { 0 },
            *command as usize,
            label.as_ptr(),
        );
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

unsafe fn add_tray(hwnd: HWND, state: &AppState) -> i32 {
    let icon = state.tray_icon.load(Ordering::Acquire) as HICON;
    let mut data = notify_data(hwnd, icon);
    copy_wide(&mut data.szTip, WINDOW_TITLE);
    let result = Shell_NotifyIconW(NIM_ADD, &data);
    if result != 0 {
        state.tray_added.store(true, Ordering::Release);
        let mut version = notify_data(hwnd, icon);
        version.uFlags = NIF_MESSAGE;
        version.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &version);
    }
    result
}

unsafe fn update_tray(hwnd: HWND, icon: HICON, status: &str) {
    let mut data = notify_data(hwnd, icon);
    data.uFlags = NIF_ICON | NIF_TIP;
    copy_wide(&mut data.szTip, &format!("DSH启动器 · {status}"));
    Shell_NotifyIconW(NIM_MODIFY, &data);
}

unsafe fn delete_tray(hwnd: HWND) {
    let data = notify_data(hwnd, std::ptr::null_mut());
    Shell_NotifyIconW(NIM_DELETE, &data);
}

unsafe fn notify(hwnd: HWND, title: &str, message: &str, error: bool) {
    let mut data = notify_data(hwnd, std::ptr::null_mut());
    data.uFlags = NIF_INFO;
    copy_wide(&mut data.szInfoTitle, title);
    copy_wide(&mut data.szInfo, message);
    data.dwInfoFlags = if error { NIIF_ERROR } else { NIIF_INFO };
    Shell_NotifyIconW(NIM_MODIFY, &data);
}

fn notify_data(hwnd: HWND, icon: HICON) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: TRAY_MESSAGE,
        hIcon: icon,
        ..NOTIFYICONDATAW::default()
    }
}

unsafe fn set_window_icon(hwnd: HWND, icon: HICON) {
    SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, icon as isize);
    SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, icon as isize);
    SendMessageW(hwnd, WM_SETICON, ICON_SMALL2 as usize, icon as isize);
}

unsafe fn load_icon(module: *mut c_void, id: usize) -> HICON {
    LoadIconW(module, id as *const u16)
}

unsafe fn apply_window_style(hwnd: HWND) {
    let dark = 0i32;
    let corner = DWMWCP_ROUND;
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
        (&dark as *const i32).cast::<c_void>(),
        std::mem::size_of::<i32>() as u32,
    );
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE as u32,
        (&corner as *const i32).cast::<c_void>(),
        std::mem::size_of::<i32>() as u32,
    );
}

fn is_high_contrast() -> bool {
    let mut contrast = HIGHCONTRASTW {
        cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
        dwFlags: 0,
        lpszDefaultScheme: std::ptr::null_mut(),
    };
    unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            (&mut contrast as *mut HIGHCONTRASTW).cast::<c_void>(),
            0,
        ) != 0
            && contrast.dwFlags & HCF_HIGHCONTRASTON != 0
    }
}

unsafe fn state_for(hwnd: HWND) -> Option<Arc<AppState>> {
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
    if pointer == 0 {
        None
    } else {
        Some((&*(pointer as *const Arc<AppState>)).clone())
    }
}

fn tray_event(lparam: LPARAM) -> u32 {
    (lparam as usize & 0xffff) as u32
}

unsafe fn set_text(hwnd: HWND, text: &str) {
    let text = to_wide(text);
    SetWindowTextW(hwnd, text.as_ptr());
    windows_sys::Win32::UI::Accessibility::NotifyWinEvent(
        EVENT_OBJECT_NAMECHANGE,
        hwnd,
        OBJID_CLIENT,
        0,
    );
}

unsafe fn confirm_box(hwnd: HWND, text: &str) -> bool {
    let text = to_wide(text);
    let title = to_wide(WINDOW_TITLE);
    MessageBoxW(
        hwnd,
        text.as_ptr(),
        title.as_ptr(),
        MB_YESNO | MB_ICONQUESTION,
    ) == IDYES
}

unsafe fn show_info_box(hwnd: HWND, text: &str) {
    let text = to_wide(text);
    let title = to_wide(WINDOW_TITLE);
    MessageBoxW(
        hwnd,
        text.as_ptr(),
        title.as_ptr(),
        MB_OK | MB_ICONINFORMATION,
    );
}

fn show_error_box(hwnd: HWND, text: &str) {
    let text = to_wide(text);
    let title = to_wide(WINDOW_TITLE);
    unsafe { MessageBoxW(hwnd, text.as_ptr(), title.as_ptr(), MB_OK | MB_ICONERROR) };
}

fn attach_console() {
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

fn write_console(message: &str, error: bool) {
    let text = format!("{message}\r\n");
    if let Some(path) = env::var_os(CLI_OUTPUT_ENV).map(PathBuf::from) {
        if fs::write(path, text.as_bytes()).is_ok() {
            return;
        }
    }
    let handle = unsafe {
        GetStdHandle(if error {
            STD_ERROR_HANDLE
        } else {
            STD_OUTPUT_HANDLE
        })
    };
    if !handle.is_null() && handle != -1isize as HANDLE {
        write_console_handle(handle, &text);
    } else {
        let device = to_wide("CONOUT$");
        let handle = unsafe {
            CreateFileW(
                device.as_ptr(),
                FILE_GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if !handle.is_null() && handle != -1isize as HANDLE {
            write_console_handle(handle, &text);
            unsafe {
                CloseHandle(handle);
            }
        }
    }
}

fn write_console_handle(handle: HANDLE, text: &str) {
    let mut mode = 0;
    let mut written = 0;
    unsafe {
        if GetConsoleMode(handle, &mut mode) != 0 {
            let wide: Vec<u16> = text.encode_utf16().collect();
            let _ = WriteConsoleW(
                handle,
                wide.as_ptr(),
                wide.len() as u32,
                &mut written,
                std::ptr::null(),
            );
        } else {
            let _ = WriteFile(
                handle,
                text.as_ptr(),
                text.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            );
        }
    }
}

fn to_wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn copy_wide(target: &mut [u16], text: &str) {
    target.fill(0);
    let encoded: Vec<u16> = text
        .encode_utf16()
        .take(target.len().saturating_sub(1))
        .collect();
    target[..encoded.len()].copy_from_slice(&encoded);
}

fn truncate(value: &str, count: usize) -> String {
    value.chars().take(count).collect()
}

fn first_line(value: &str) -> &str {
    value.lines().next().unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths(base: &Path) -> Paths {
        Paths {
            data: base.join("data"),
            npm_prefix: base.join("data/npm-global"),
            profile: base.join("data/profile/.dsh"),
            state: base.join("data/state"),
            updates: base.join("data/updates"),
            logs: base.join("data/logs"),
            cache: base.join("data/cache/npm"),
            temp: base.join("data/tmp"),
        }
    }

    fn write_test_package(prefix: &Path, version: &str) {
        let package = prefix.join("node_modules/@deepseek-ai/dsh");
        fs::create_dir_all(package.join("lib")).unwrap();
        fs::write(package.join("lib/bin.js"), "// test fixture").unwrap();
        fs::write(
            package.join("package.json"),
            serde_json::json!({
                "name": "@deepseek-ai/dsh", "version": version, "dependencies": {}
            })
            .to_string(),
        )
        .unwrap();
    }

    fn snapshot(installed: bool, node: bool, npm: bool, running: bool) -> Snapshot {
        Snapshot {
            installation: installed.then(|| Installation {
                source: Source::Managed,
                node: PathBuf::from("node.exe"),
                entry: PathBuf::from("bin.js"),
                version: "0.1.2-alpha.4".to_owned(),
                profile: ProfileMode::Portable,
            }),
            node_available: node,
            npm_available: npm,
            running,
            healthy: running,
            auth_unavailable: false,
            repair_needed: false,
            discovery_error: None,
        }
    }

    #[test]
    fn cli_accepts_only_four_public_actions() {
        for action in ["start", "stop", "upgrade", "open"] {
            assert!(Action::from_name(action).is_some());
        }
        for removed in [
            "restart",
            "repair",
            "migrate",
            "cleanup-legacy",
            "launcher-update",
            "data",
            "self-update",
        ] {
            assert!(Action::from_name(removed).is_none());
        }
        assert!(parse_action(&[
            "launcher.exe".to_owned(),
            "--data-dir".to_owned(),
            "D:\\data".to_owned(),
        ])
        .is_err());
    }

    #[test]
    fn dynamic_main_button_covers_all_states() {
        assert_eq!(
            main_button(&snapshot(true, true, true, false), false, false),
            MainButton::Start
        );
        assert_eq!(
            main_button(&snapshot(true, true, true, true), false, false),
            MainButton::Stop
        );
        assert_eq!(
            main_button(&snapshot(false, true, true, false), false, false),
            MainButton::InstallDsh
        );
        assert_eq!(
            main_button(&snapshot(false, false, false, false), false, false),
            MainButton::InstallNode
        );
        let mut repair = snapshot(true, true, true, false);
        repair.repair_needed = true;
        assert_eq!(main_button(&repair, false, false), MainButton::RepairDsh);
        repair.npm_available = false;
        assert_eq!(main_button(&repair, false, false), MainButton::InstallNode);
        assert_eq!(
            main_button(&snapshot(true, true, true, false), true, true),
            MainButton::Cancel
        );
        assert_eq!(
            main_button(&snapshot(true, true, true, false), true, false),
            MainButton::Busy
        );
    }

    #[test]
    fn update_keeps_the_current_or_saved_profile_mode() {
        let system = Installation {
            source: Source::System,
            node: PathBuf::from("node.exe"),
            entry: PathBuf::from("bin.js"),
            version: "0.1.2-alpha.4".to_owned(),
            profile: ProfileMode::User,
        };
        let managed = Installation {
            source: Source::Managed,
            profile: ProfileMode::User,
            ..system.clone()
        };

        assert_eq!(
            update_profile_mode(Some(&system), Some(ProfileMode::Portable)),
            ProfileMode::User
        );
        assert_eq!(
            update_profile_mode(Some(&managed), Some(ProfileMode::Portable)),
            ProfileMode::User
        );
        assert_eq!(
            update_profile_mode(None, Some(ProfileMode::User)),
            ProfileMode::User
        );
        assert_eq!(update_profile_mode(None, None), ProfileMode::Portable);
    }

    #[test]
    fn hidden_or_busy_window_does_not_schedule_health_work() {
        assert!(!health_check_allowed(false, false, false));
        assert!(!health_check_allowed(true, true, false));
        assert!(!health_check_allowed(true, false, true));
        assert!(health_check_allowed(true, false, false));
    }

    #[test]
    fn layout_scaling_covers_required_dpi_levels() {
        assert_eq!(scale(100, 96), 100);
        assert_eq!(scale(100, 144), 150);
        assert_eq!(scale(100, 192), 200);
        assert_eq!(desired_client_size(96), (CLIENT_WIDTH, CLIENT_HEIGHT));
        assert_eq!(desired_client_size(144), (726, 526));
        assert_eq!(desired_client_size(192), (968, 702));
    }

    #[test]
    fn semver_orders_prereleases_and_prevents_downgrade() {
        assert!(parse_version("0.1.2-alpha.4").unwrap() > parse_version("0.1.2-alpha.3").unwrap());
        assert!(parse_version("0.1.2").unwrap() > parse_version("0.1.2-alpha.4").unwrap());
        assert!(parse_version("0.1.1-rc.2").unwrap() < parse_version("0.1.2-alpha.4").unwrap());
        assert!(parse_version("01.2.3").is_none());
        assert!(parse_version("1.2").is_none());
        assert!(parse_version("1.2.3+build.4").is_some());
        assert!(parse_version("1.2.3+bad+build").is_none());
    }

    #[test]
    fn all_registry_versions_include_alpha_four() {
        assert_eq!(
            parse_latest_version(r#"["0.1.1-rc.2","0.1.2-alpha.3","0.1.2-alpha.4"]"#),
            Some("0.1.2-alpha.4".to_owned())
        );
    }

    #[test]
    fn health_response_requires_dsh_identity_marker() {
        assert_eq!(
            classify_dsh_response(
                b"HTTP/1.1 200 OK\r\n\r\n<script>window.__DSH_BOOT__={}</script>"
            ),
            ProbeResponse::Ready
        );
        assert_eq!(
            classify_dsh_response(b"HTTP/1.1 200 OK\r\n\r\nordinary local web app"),
            ProbeResponse::NotDsh
        );
        assert_eq!(
            classify_dsh_response(
                b"HTTP/1.1 401 Unauthorized\r\n\r\ndsh web authentication required"
            ),
            ProbeResponse::AuthenticationRequired
        );
        assert_eq!(
            classify_dsh_response(b"HTTP/1.1 500 Error\r\n\r\n__DSH_BOOT__"),
            ProbeResponse::NotDsh
        );
        assert!(valid_web_url("http://127.0.0.1:3080/?token=abc_DEF-123"));
        assert!(!valid_web_url("http://evil.example/?token=abc"));
        assert!(!valid_web_url(
            "http://127.0.0.1:3080/?token=abc%0dInjected"
        ));
    }

    #[test]
    fn authenticated_web_url_must_be_ready_before_enabling_ui() {
        let base = env::temp_dir().join(format!("dsh-launcher-auth-{}", transaction_nonce()));
        let paths = test_paths(&base);
        paths.ensure_layout().unwrap();
        let auth_url = "http://127.0.0.1:3080/?token=auth-token".to_owned();
        fs::write(
            paths.logs.join("dsh.out.log"),
            format!("dsh web: {auth_url}\n"),
        )
        .unwrap();

        let ready = probe_dsh_with(&paths, |url| {
            if url == WEB_URL {
                ProbeResponse::AuthenticationRequired
            } else if url == auth_url {
                ProbeResponse::Ready
            } else {
                ProbeResponse::NotDsh
            }
        });
        assert!(ready.identified);
        assert_eq!(ready.web_url, Some(auth_url.clone()));

        let stale = probe_dsh_with(&paths, |_url| ProbeResponse::AuthenticationRequired);
        assert!(stale.identified);
        assert_eq!(stale.web_url, None);

        fs::remove_file(paths.logs.join("dsh.out.log")).unwrap();
        let missing = probe_dsh_with(&paths, |_url| ProbeResponse::AuthenticationRequired);
        assert!(missing.identified);
        assert_eq!(missing.web_url, None);

        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn schema_two_manifest_is_minimal_and_strict() {
        let manifest = parse_runtime_manifest(MANIFEST_TEXT).unwrap();
        assert_eq!(manifest.schema_version, 2);
        assert_eq!(manifest.package, "@deepseek-ai/dsh");
        assert_eq!(manifest.node_download_page, NODE_DOWNLOAD_URL);
        let mut value: serde_json::Value = serde_json::from_str(MANIFEST_TEXT).unwrap();
        value["schema_version"] = serde_json::json!(1);
        assert!(parse_runtime_manifest(&value.to_string()).is_err());
    }

    #[test]
    fn transaction_round_trip_preserves_profile_and_paths() {
        let base =
            env::temp_dir().join(format!("dsh-launcher-transaction-{}", transaction_nonce()));
        let paths = test_paths(&base);
        paths.ensure_layout().unwrap();
        let target_parent = paths.managed_package().parent().unwrap().to_path_buf();
        fs::create_dir_all(&target_parent).unwrap();
        let transaction = UpdateTransaction {
            phase: "committed".to_owned(),
            target: paths.managed_package(),
            backup: Some(paths.updates.join("rollback")),
            stage: paths.updates.join("stage"),
            profile: ProfileMode::User,
        };
        write_transaction(&paths, &transaction).unwrap();
        let loaded = read_transaction(&paths).unwrap().unwrap();
        assert_eq!(loaded.phase, "committed");
        assert_eq!(loaded.profile, ProfileMode::User);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn atomic_state_write_replaces_existing_content() {
        let base = env::temp_dir().join(format!("dsh-launcher-atomic-{}", transaction_nonce()));
        fs::create_dir_all(&base).unwrap();
        let path = base.join("state.json");
        atomic_write(&path, b"old").unwrap();
        atomic_write(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(fs::read_dir(&base).unwrap().count(), 1);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn staged_package_requires_all_declared_runtime_dependencies() {
        let base = env::temp_dir().join(format!("dsh-launcher-deps-{}", transaction_nonce()));
        let package = base.join("node_modules/@deepseek-ai/dsh");
        fs::create_dir_all(package.join("lib")).unwrap();
        fs::write(package.join("lib/bin.js"), "").unwrap();
        fs::write(
            package.join("package.json"),
            r#"{"name":"@deepseek-ai/dsh","version":"0.1.2-alpha.4","dependencies":{"example-runtime":"1.0.0"}}"#,
        )
        .unwrap();
        assert!(verify_staged_package(&package, "0.1.2-alpha.4").is_err());
        fs::create_dir_all(base.join("node_modules/example-runtime")).unwrap();
        fs::write(
            base.join("node_modules/example-runtime/package.json"),
            r#"{"name":"example-runtime","version":"1.0.0"}"#,
        )
        .unwrap();
        verify_staged_package(&package, "0.1.2-alpha.4").unwrap();
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn interrupted_prefix_swap_keeps_the_newest_prefix() {
        let base = env::temp_dir().join(format!("dsh-launcher-rollback-{}", transaction_nonce()));
        let paths = test_paths(&base);
        paths.ensure_layout().unwrap();
        fs::write(paths.npm_prefix.join("old.txt"), "old").unwrap();
        let stage = paths.updates.join("stage");
        let backup = paths.updates.join("rollback");
        fs::create_dir_all(&stage).unwrap();
        fs::write(stage.join("new.txt"), "new").unwrap();
        write_test_package(&stage, "0.1.2-alpha.4");
        let transaction = UpdateTransaction {
            phase: "old-moved".to_owned(),
            target: paths.npm_prefix.clone(),
            backup: Some(backup.clone()),
            stage: stage.clone(),
            profile: ProfileMode::Portable,
        };
        write_transaction(&paths, &transaction).unwrap();
        fs::rename(&paths.npm_prefix, &backup).unwrap();
        fs::rename(&stage, &paths.npm_prefix).unwrap();

        recover_update_transaction(&paths).unwrap();

        assert_eq!(
            fs::read_to_string(paths.npm_prefix.join("new.txt")).unwrap(),
            "new"
        );
        assert!(!paths.npm_prefix.join("old.txt").exists());
        assert!(!paths.transaction_file().exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn recovery_before_backup_move_promotes_the_newest_prefix() {
        let base = env::temp_dir().join(format!("dsh-launcher-prepared-{}", transaction_nonce()));
        let paths = test_paths(&base);
        paths.ensure_layout().unwrap();
        fs::write(paths.npm_prefix.join("old.txt"), "old").unwrap();
        let stage = paths.updates.join("stage");
        fs::create_dir_all(&stage).unwrap();
        fs::write(stage.join("new.txt"), "new").unwrap();
        write_test_package(&stage, "0.1.2-alpha.4");
        let transaction = UpdateTransaction {
            phase: "prepared".to_owned(),
            target: paths.npm_prefix.clone(),
            backup: Some(paths.updates.join("rollback-not-created")),
            stage,
            profile: ProfileMode::Portable,
        };

        write_transaction(&paths, &transaction).unwrap();
        recover_update_transaction(&paths).unwrap();

        assert_eq!(
            fs::read_to_string(paths.npm_prefix.join("new.txt")).unwrap(),
            "new"
        );
        assert!(!paths.npm_prefix.join("old.txt").exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn committed_candidate_is_kept_and_old_backup_is_removed() {
        let base = env::temp_dir().join(format!(
            "dsh-launcher-committed-candidate-{}",
            transaction_nonce()
        ));
        let paths = test_paths(&base);
        paths.ensure_layout().unwrap();
        fs::write(paths.npm_prefix.join("broken.txt"), "broken").unwrap();
        write_test_package(&paths.npm_prefix, "0.1.2-alpha.4");
        let backup = paths.updates.join("rollback");
        fs::create_dir_all(&backup).unwrap();
        fs::write(backup.join("old.txt"), "old").unwrap();
        let transaction = UpdateTransaction {
            phase: "committed".to_owned(),
            target: paths.npm_prefix.clone(),
            backup: Some(backup),
            stage: paths.updates.join("stage-already-promoted"),
            profile: ProfileMode::Portable,
        };
        write_transaction(&paths, &transaction).unwrap();

        recover_update_transaction(&paths).unwrap();

        assert_eq!(
            fs::read_to_string(paths.npm_prefix.join("broken.txt")).unwrap(),
            "broken"
        );
        assert!(!paths.updates.join("rollback").exists());
        assert!(!paths.transaction_file().exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn corrupt_transaction_promotes_stage_and_never_restores_old_version() {
        let base = env::temp_dir().join(format!("dsh-launcher-corrupt-{}", transaction_nonce()));
        let paths = test_paths(&base);
        paths.ensure_layout().unwrap();
        fs::write(paths.npm_prefix.join("old.txt"), "old").unwrap();
        let stage = paths.updates.join("dsh-stage-1");
        let old = paths.updates.join("dsh-old-1");
        fs::create_dir_all(&stage).unwrap();
        fs::create_dir_all(&old).unwrap();
        fs::write(stage.join("new.txt"), "new").unwrap();
        write_test_package(&stage, "0.1.2-alpha.4");
        fs::write(old.join("old.txt"), "old").unwrap();
        fs::write(paths.transaction_file(), "{").unwrap();

        recover_update_transaction(&paths).unwrap();

        assert_eq!(
            fs::read_to_string(paths.npm_prefix.join("new.txt")).unwrap(),
            "new"
        );
        assert!(!old.exists());
        assert!(!paths.transaction_file().exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn authentication_survives_large_and_multibyte_logs_and_rotation() {
        let base = env::temp_dir().join(format!("dsh-auth-growth-{}", transaction_nonce()));
        let paths = test_paths(&base);
        paths.ensure_layout().unwrap();
        let log = paths.logs.join("dsh.out.log");
        let auth = "http://127.0.0.1:3080/?token=first";
        let probe = || {
            probe_dsh_with(&paths, |url| {
                if url == WEB_URL {
                    ProbeResponse::AuthenticationRequired
                } else if url == auth {
                    ProbeResponse::Ready
                } else {
                    ProbeResponse::NotDsh
                }
            })
        };
        fs::write(&log, format!("dsh web: {auth}\n")).unwrap();
        assert_eq!(probe().web_url.as_deref(), Some(auth));
        let mut file = OpenOptions::new().append(true).open(&log).unwrap();
        file.write_all("中文日志\n".repeat(2000).as_bytes())
            .unwrap();
        drop(file);
        assert_eq!(probe().web_url.as_deref(), Some(auth));
        // A fresh log forces a scan without the previously cached address.
        fs::rename(&log, paths.logs.join("dsh.out.log.1")).unwrap();
        fs::write(&log, format!("{}\ndsh web: {auth}\n", "中".repeat(3000))).unwrap();
        assert_eq!(probe().web_url.as_deref(), Some(auth));
        // Truncation must discard the previous process's URL.
        fs::write(&log, "new process starting\n").unwrap();
        assert_eq!(probe().web_url, None);
        let mut file = OpenOptions::new().append(true).open(&log).unwrap();
        file.write_all(b"dsh web: http://127.0.0.1:3080/?tok")
            .unwrap();
        assert_eq!(logged_web_url(&paths), None);
        file.write_all(b"en=second\n").unwrap();
        drop(file);
        assert_eq!(
            logged_web_url(&paths).as_deref(),
            Some("http://127.0.0.1:3080/?token=second")
        );
        assert_eq!(probe().web_url, None); // stale/invalid URLs are still revalidated
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn authenticated_service_reports_actionable_unavailable_status() {
        let mut value = snapshot(true, true, true, true);
        value.healthy = false;
        value.auth_unavailable = true;
        assert!(status_for_snapshot(&value).contains("认证 URL 不可用"));
        assert_eq!(main_button(&value, false, false), MainButton::Stop);
    }

    #[test]
    fn missing_candidate_enters_repair_instead_of_committing_empty_target() {
        let base = env::temp_dir().join(format!("dsh-missing-candidate-{}", transaction_nonce()));
        let paths = test_paths(&base);
        paths.ensure_layout().unwrap();
        let backup = paths.updates.join("dsh-old-1");
        write_test_package(&backup, "0.1.1");
        write_transaction(
            &paths,
            &UpdateTransaction {
                phase: "old-moved".to_owned(),
                target: paths.npm_prefix.clone(),
                backup: Some(backup.clone()),
                stage: paths.updates.join("dsh-stage-missing"),
                profile: ProfileMode::User,
            },
        )
        .unwrap();
        paths.ensure_layout().unwrap();
        assert!(recover_update_transaction(&paths).is_err());
        assert!(paths.repair_file().is_file());
        assert!(!paths.transaction_file().exists());
        assert!(backup.exists());
        assert_eq!(read_profile_mode(&paths), Some(ProfileMode::User));
        recover_for_use(&paths).unwrap(); // GUI/upgrade must remain accessible
        let mut value = snapshot(false, true, true, false);
        value.repair_needed = paths.repair_file().is_file();
        assert_eq!(main_button(&value, false, false), MainButton::RepairDsh);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn missing_prepared_stage_does_not_accept_the_old_target() {
        let base = env::temp_dir().join(format!("dsh-prepared-missing-{}", transaction_nonce()));
        let paths = test_paths(&base);
        paths.ensure_layout().unwrap();
        write_test_package(&paths.npm_prefix, "0.1.1");
        write_transaction(
            &paths,
            &UpdateTransaction {
                phase: "prepared".to_owned(),
                target: paths.npm_prefix.clone(),
                backup: Some(paths.updates.join("dsh-old-not-moved")),
                stage: paths.updates.join("dsh-stage-missing"),
                profile: ProfileMode::User,
            },
        )
        .unwrap();
        assert!(recover_update_transaction(&paths).is_err());
        assert!(paths.repair_file().exists());
        assert_eq!(
            recovery_version(&paths.npm_prefix).as_deref(),
            Some("0.1.1")
        );
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn invalid_orphan_requires_repair_and_a_valid_retry_preserves_profile() {
        let base = env::temp_dir().join(format!("dsh-invalid-orphan-{}", transaction_nonce()));
        let paths = test_paths(&base);
        paths.ensure_layout().unwrap();
        write_profile_mode(&paths, ProfileMode::User).unwrap();
        write_test_package(&paths.npm_prefix, "0.1.1");
        let stage = paths.updates.join("dsh-stage-incomplete");
        fs::create_dir_all(&stage).unwrap();
        fs::write(stage.join("partial.tmp"), "partial").unwrap();
        fs::write(paths.transaction_file(), "{").unwrap();
        assert!(recover_update_transaction(&paths).is_err());
        assert!(paths.repair_file().exists());
        assert!(!paths.transaction_file().exists());
        assert_eq!(
            recovery_version(&paths.npm_prefix).as_deref(),
            Some("0.1.1")
        );
        assert!(!paths.npm_prefix.join("partial.tmp").exists());
        recover_for_use(&paths).unwrap();
        write_test_package(&stage, "0.1.2-alpha.4");
        let profile = update_profile_mode(None, read_profile_mode(&paths));
        promote_stage(&paths, &stage, profile).unwrap();
        recover_update_transaction(&paths).unwrap();
        assert!(!paths.repair_file().exists());
        assert_eq!(
            recovery_version(&paths.npm_prefix).as_deref(),
            Some("0.1.2-alpha.4")
        );
        assert_eq!(read_profile_mode(&paths), Some(ProfileMode::User));
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn recovery_checks_dependencies_and_keeps_the_repair_version() {
        let base = env::temp_dir().join(format!("dsh-recovery-deps-{}", transaction_nonce()));
        let paths = test_paths(&base);
        paths.ensure_layout().unwrap();
        let stage = paths.updates.join("dsh-stage-incomplete");
        write_test_package(&stage, "0.1.2-alpha.4");
        fs::write(stage.join("node_modules/@deepseek-ai/dsh/package.json"),
            r#"{"name":"@deepseek-ai/dsh","version":"0.1.2-alpha.4","dependencies":{"missing":"1.0.0"}}"#).unwrap();
        write_transaction(
            &paths,
            &UpdateTransaction {
                phase: "prepared".to_owned(),
                target: paths.npm_prefix.clone(),
                backup: None,
                stage,
                profile: ProfileMode::User,
            },
        )
        .unwrap();
        assert!(recover_update_transaction(&paths).is_err());
        assert_eq!(
            read_repair_version(&paths).as_deref(),
            Some("0.1.2-alpha.4")
        );
        assert!(!paths.managed_package().exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn netstat_parser_only_accepts_requested_listening_port() {
        assert_eq!(
            parse_listening_pid("TCP 127.0.0.1:3080 0.0.0.0:0 LISTENING 42", 3080),
            Some(42)
        );
        assert_eq!(
            parse_listening_pid("TCP 127.0.0.1:3081 0.0.0.0:0 LISTENING 42", 3080),
            None
        );
    }

    #[test]
    fn command_ownership_requires_dsh_web_entry() {
        assert!(is_dsh_command(
            r#"node C:\pkg\@deepseek-ai\dsh\lib\bin.js web --port 3080"#,
            3080
        ));
        assert!(!is_dsh_command(
            r#"node C:\other\server.js web --port 3080"#,
            3080
        ));
        assert!(!is_dsh_command(
            r#"node C:\pkg\@deepseek-ai\dsh\lib\bin.js web --port 3081"#,
            3080
        ));
    }

    #[test]
    fn release_smoke_flag_is_exact() {
        assert!(is_release_smoke(&[
            "launcher.exe".to_owned(),
            "--release-smoke".to_owned()
        ]));
        assert!(!is_release_smoke(&["launcher.exe".to_owned()]));
    }

    #[test]
    fn source_is_small_enough_for_lightweight_gate() {
        assert!(include_str!("main.rs").lines().count() <= 6500);
    }
}
