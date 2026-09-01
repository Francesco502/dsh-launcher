#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::VecDeque;
use std::env;
use std::ffi::{c_void, OsStr};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Component, Path, PathBuf, Prefix};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use std::os::windows::fs::MetadataExt;
use std::os::windows::process::CommandExt;

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, GlobalFree, ERROR_ALREADY_EXISTS, FILETIME, HANDLE, HWND, POINT,
    RECT,
};
use windows_sys::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMSBT_MAINWINDOW, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR,
    DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, Ellipse, EndPaint,
    FillRect, GetStockObject, GetSysColor, GradientFill, InvalidateRect, RoundRect, SelectObject,
    SetBkMode, SetDCBrushColor, SetDCPenColor, SetTextColor, UpdateWindow, CLEARTYPE_QUALITY,
    CLIP_DEFAULT_PRECIS, COLOR_GRAYTEXT, COLOR_HIGHLIGHT as SYSTEM_COLOR_HIGHLIGHT, COLOR_WINDOW,
    COLOR_WINDOWTEXT, DC_BRUSH, DC_PEN, DEFAULT_PITCH, DT_CENTER, DT_LEFT, DT_SINGLELINE,
    DT_VCENTER, FF_DONTCARE, FW_NORMAL, FW_SEMIBOLD, GB2312_CHARSET, GRADIENT_FILL_RECT_V,
    GRADIENT_RECT, NULL_BRUSH, OUT_DEFAULT_PRECIS, PAINTSTRUCT, TRANSPARENT, TRIVERTEX,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, MoveFileExW, ReplaceFileW, WriteFile, FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, OPEN_EXISTING,
    REPLACEFILE_WRITE_THROUGH,
};
use windows_sys::Win32::System::Console::{
    AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE,
    STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::SystemServices::{SS_CENTERIMAGE, SS_OWNERDRAW};
use windows_sys::Win32::System::Threading::{
    CreateMutexW, GetExitCodeProcess, GetProcessTimes, OpenProcess, QueryFullProcessImageNameW,
    ReleaseMutex, TerminateProcess, CREATE_NO_WINDOW, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_TERMINATE, PROCESS_VM_READ,
};
use windows_sys::Win32::UI::Accessibility::{NotifyWinEvent, HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows_sys::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_DISABLED, ODS_FOCUS, ODS_SELECTED};
use windows_sys::Win32::UI::HiDpi::{
    GetDpiForSystem, GetDpiForWindow, SetProcessDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, GetFocus, IsWindowEnabled, SetFocus, VK_ESCAPE, VK_RETURN,
};
use windows_sys::Win32::UI::Shell::{
    IsUserAnAdmin, SHFileOperationW, SetCurrentProcessExplicitAppUserModelID, Shell_NotifyIconW,
    FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FO_DELETE, NIF_ICON, NIF_INFO, NIF_MESSAGE,
    NIF_TIP, NIIF_ERROR, NIIF_INFO, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_SELECT,
    NOTIFYICONDATAW, NOTIFYICON_VERSION_4, SHFILEOPSTRUCTW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateAcceleratorTableW, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
    DestroyAcceleratorTable, DestroyMenu, DestroyWindow, DispatchMessageW, DrawIconEx, FindWindowW,
    GetClientRect, GetCursorPos, GetDlgCtrlID, GetDlgItem, GetMessageW, GetParent,
    GetSystemMetrics, GetWindowLongPtrW, GetWindowTextW, IsDialogMessageW, KillTimer, LoadCursorW,
    LoadIconW, MessageBoxW, PostMessageW, PostQuitMessage, RegisterClassExW,
    RegisterWindowMessageW, SendMessageW, SetForegroundWindow, SetTimer, SetWindowLongPtrW,
    SetWindowPos, SetWindowTextW, ShowWindow, SystemParametersInfoW, TrackPopupMenu,
    TranslateAcceleratorW, TranslateMessage, WindowFromPoint, ACCEL, BM_CLICK, BS_OWNERDRAW,
    CREATESTRUCTW, DI_NORMAL, EVENT_OBJECT_NAMECHANGE, FVIRTKEY, GWLP_USERDATA, HICON, ICON_BIG,
    ICON_SMALL, ICON_SMALL2, IDC_ARROW, IDYES, MB_ICONERROR, MB_ICONQUESTION, MB_OK, MB_YESNO,
    MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG, OBJID_CLIENT, SM_CXSCREEN, SM_CYSCREEN,
    SPI_GETHIGHCONTRAST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_RESTORE,
    SW_SHOW, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_CLOSE, WM_COMMAND,
    WM_CONTEXTMENU, WM_CTLCOLORSTATIC, WM_DESTROY, WM_DPICHANGED, WM_DRAWITEM, WM_KEYDOWN,
    WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_NCCREATE, WM_NULL, WM_PAINT, WM_RBUTTONUP, WM_SETFONT,
    WM_SETICON, WM_SETTINGCHANGE, WM_THEMECHANGED, WM_TIMER, WNDCLASSEXW, WS_CAPTION, WS_CHILD,
    WS_EX_APPWINDOW, WS_EX_CONTROLPARENT, WS_EX_TRANSPARENT, WS_MINIMIZEBOX, WS_OVERLAPPED,
    WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

#[link(name = "ntdll")]
extern "system" {
    fn NtQueryInformationProcess(
        process_handle: HANDLE,
        process_information_class: u32,
        process_information: *mut c_void,
        process_information_length: u32,
        return_length: *mut u32,
    ) -> i32;
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ProcessBasicInformation {
    reserved1: *mut c_void,
    peb_base_address: *mut c_void,
    reserved2: [*mut c_void; 2],
    unique_process_id: *mut c_void,
    reserved3: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *mut u16,
}

#[cfg(target_pointer_width = "64")]
const PEB_PROCESS_PARAMETERS_OFFSET: usize = 0x20;
#[cfg(target_pointer_width = "32")]
const PEB_PROCESS_PARAMETERS_OFFSET: usize = 0x10;
#[cfg(target_pointer_width = "64")]
const PROCESS_PARAMETERS_COMMAND_LINE_OFFSET: usize = 0x70;
#[cfg(target_pointer_width = "32")]
const PROCESS_PARAMETERS_COMMAND_LINE_OFFSET: usize = 0x40;

const DSH_PORT: u16 = 3080;
const DATA_DIRECTORY: &str = "data";
const PORTABLE_MARKER_FILE: &str = "portable.flag";
const RUNTIME_MANIFEST_FILE: &str = "runtime-manifest.json";
const RUNTIME_MANIFEST_TEXT: &str = include_str!("../runtime-manifest.json");
const RUNTIME_DIRECTORY: &str = "runtime";
const NPM_GLOBAL_DIRECTORY: &str = "npm-global";
const RUNTIME_READY_FILE: &str = "runtime.ready";
const NATIVE_DSH_PID_FILE: &str = "dsh-launcher-native.pid";
const DSH_PROFILE_DIRECTORY: &str = "profile";
const DSH_WEB_PROFILE_NAME: &str = "web";
const DSH_QUOTA_RUNTIME_NAME: &str = "dsh-quota";
const DSH_STATE_DIRECTORY: &str = "state";
const DSH_CACHE_DIRECTORY: &str = "cache";
const DSH_UPDATE_DIRECTORY: &str = "updates";
const DSH_TEMP_DIRECTORY: &str = "tmp";
const DSH_LOG_DIRECTORY: &str = "logs";
const LEGACY_MIGRATION_MARKER_FILE: &str = "legacy-migration-reviewed";
const LEGACY_MIGRATION_JOURNAL_FILE: &str = "legacy-migration.journal";
const MIGRATION_DIRECTORY: &str = "migration-staging";
const FAILED_STAGING_RETENTION: Duration = Duration::from_secs(24 * 60 * 60);
const LOG_ROTATION_LIMIT_BYTES: u64 = 5 * 1024 * 1024;
const LOG_ROTATION_FILES: usize = 5;
const DSH_LOG_TAIL_BYTES: u64 = 4096;
const DSH_STDOUT_LOG_FILE: &str = "dsh-launcher-native.out.log";
const DSH_STDERR_LOG_FILE: &str = "dsh-launcher-native.err.log";
const DSH_PREFLIGHT_STDOUT_LOG_FILE: &str = "dsh-launcher-preflight.out.log";
const DSH_PREFLIGHT_STDERR_LOG_FILE: &str = "dsh-launcher-preflight.err.log";
const DSH_START_TIMEOUT: Duration = Duration::from_secs(45);
const DSH_STOP_TIMEOUT: Duration = Duration::from_secs(15);
const DSH_UPDATE_COMMAND_TIMEOUT: Duration = Duration::from_secs(60);
const ACTION_STATUS_HOLD: Duration = Duration::from_secs(8);
const DSH_PACKAGE_COMMAND_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const RUNTIME_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);
const UPGRADE_PREFLIGHT_PORT: u16 = 3081;
const PROCESS_STILL_ACTIVE: u32 = 259;
const WEB_URL: &str = "http://127.0.0.1:3080/";
const QUOTA_CONFIG_PATH: &str = "/api/dsh-quota/config";
const DSH_LATEST_VERSION_SCRIPT: &str = "fetch(process.argv[1],{signal:AbortSignal.timeout(10000)}).then(r=>{if(!r.ok)throw new Error('HTTP '+r.status);return r.json()}).then(p=>console.log(JSON.stringify(Object.keys(p.versions||{})))).catch(e=>{console.error(e.message);process.exit(1)})";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_USER_MODEL_ID: &str = "DeepSeek.DSHLauncher";
const LAUNCHER_RELEASE_API_URL: &str =
    "https://api.github.com/repos/Francesco502/dsh-launcher/releases/latest";
const LAUNCHER_ASSET_NAME: &str = "DSH-Launcher.exe";
const LAUNCHER_CHECKSUM_ASSET_NAME: &str = "DSH-Launcher.exe.sha256";
const LAUNCHER_ASSET_URL_PREFIX: &str =
    "https://github.com/Francesco502/dsh-launcher/releases/download/";
const DSH_BUILD_SCRIPT_PACKAGES: [&str; 5] = [
    "@deepseek-ai/dsh-subprocess-local",
    "koffi",
    "node-pty",
    "@google/genai",
    "protobufjs",
];
const SELF_UPDATE_NEW_FILE: &str = "DSH-Launcher.exe.new";
const SELF_UPDATE_CHECKSUM_FILE: &str = "DSH-Launcher.exe.sha256";
const SELF_UPDATE_HELPER_FILE: &str = "DSH-Launcher.exe.helper.exe";
const SELF_UPDATE_TRANSACTION_FILE: &str = "transaction.json";
const SELF_UPDATE_HEALTH_FILE: &str = "health.handshake";
const SELF_UPDATE_WAIT_TIMEOUT: Duration = Duration::from_secs(30);
const SELF_UPDATE_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_HEALTH_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const CREATE_NO_WINDOW_FLAG: u32 = CREATE_NO_WINDOW;

const TRAY_MESSAGE: u32 = WM_APP + 1;
const STATUS_MESSAGE: u32 = WM_APP + 2;
const SHOW_WINDOW_MESSAGE: u32 = WM_APP + 3;
const SELF_UPDATE_EXIT_MESSAGE: u32 = WM_APP + 4;
const DIAGNOSTICS_READY_MESSAGE: u32 = WM_APP + 5;
const NIN_KEYSELECT: u32 = 1025;

const CMD_START: u32 = 1001;
const CMD_RESTART: u32 = 1002;
const CMD_STOP: u32 = 1003;
const CMD_UPGRADE: u32 = 1004;
const CMD_EXIT: u32 = 1005;
const CMD_SHOW: u32 = 1006;
const CMD_OPEN_WEB: u32 = 1007;
const CMD_LAUNCHER_UPDATE: u32 = 1008;
const CMD_OPEN_DATA: u32 = 1009;
const CMD_REPAIR: u32 = 1010;
const CMD_DETAILS: u32 = 1011;
const CMD_CANCEL: u32 = 1012;
const CMD_CLEANUP_LEGACY: u32 = 1013;
const CMD_MIGRATE_LEGACY: u32 = 1014;
const CMD_MORE_TOOLS: u32 = 1015;
const CMD_HOME: u32 = 1016;
const ID_TITLE: u32 = 1101;
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
const HOVER_BUTTON_COUNT: usize = 13;
const WINDOW_CLASS: &str = "DeepSeekHarnessDshControlWindow";
const WINDOW_TITLE: &str = "DSH启动器";
const UI_FONT_FAMILY: &str = "Microsoft YaHei UI";
const ICON_RESOURCE_ID: usize = 1;
const BLACK_ICON_RESOURCE_ID: usize = 2;
const WINDOW_WIDTH: i32 = 620;
const WINDOW_HEIGHT: i32 = 560;
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
    LauncherUpdate,
    Repair,
    Migrate,
    CleanupLegacy,
    OpenWeb,
    OpenData,
}

impl Action {
    fn from_name(value: &str) -> Option<Self> {
        match value {
            "start" => Some(Self::Start),
            "restart" => Some(Self::Restart),
            "stop" | "close" => Some(Self::Stop),
            "upgrade" | "update" => Some(Self::Upgrade),
            "launcher-update" | "self-update" => Some(Self::LauncherUpdate),
            "repair" | "verify-repair" => Some(Self::Repair),
            "migrate" | "migrate-legacy" => Some(Self::Migrate),
            "open" | "web" => Some(Self::OpenWeb),
            "data" | "open-data" => Some(Self::OpenData),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Start => "启动服务",
            Self::Restart => "重启服务",
            Self::Stop => "停止服务",
            Self::Upgrade => "更新 DSH",
            Self::LauncherUpdate => "更新启动器",
            Self::Repair => "验证并修复运行时",
            Self::Migrate => "迁移旧数据",
            Self::CleanupLegacy => "移入旧数据回收站",
            Self::OpenWeb => "打开网页",
            Self::OpenData => "打开数据目录",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RuntimeManifest {
    schema_version: u64,
    architecture: String,
    node_version: String,
    node_archive_name: String,
    node_url: String,
    node_sha256: String,
    dsh_package: String,
    dsh_bootstrap_version: String,
    dsh_registry_url: String,
    dsh_entry: String,
    dsh_peer_dependencies: Vec<String>,
    quota_package: String,
    quota_runtime_name: String,
    quota_version: String,
    quota_archive_name: String,
    quota_url: String,
    quota_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DshVersion {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: Vec<SemverIdentifier>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SemverIdentifier {
    Numeric(u64),
    Text(String),
}

impl std::fmt::Display for DshVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.prerelease.is_empty() {
            write!(formatter, "-")?;
            for (index, identifier) in self.prerelease.iter().enumerate() {
                if index > 0 {
                    write!(formatter, ".")?;
                }
                match identifier {
                    SemverIdentifier::Numeric(value) => write!(formatter, "{value}")?,
                    SemverIdentifier::Text(value) => write!(formatter, "{value}")?,
                }
            }
        }
        Ok(())
    }
}

impl Ord for DshVersion {
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
                    (false, false) => {
                        compare_semver_identifiers(&self.prerelease, &other.prerelease)
                    }
                },
            )
    }
}

impl PartialOrd for DshVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MigrationCandidate {
    id: &'static str,
    label: &'static str,
    source: PathBuf,
    target: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileFingerprint {
    relative_path: String,
    size: u64,
    sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SelfUpdateTransaction {
    parent_pid: u32,
    transaction_id: String,
    source: PathBuf,
    target: PathBuf,
    checksum_file: PathBuf,
    backup: PathBuf,
    health_file: PathBuf,
    expected_version: String,
    expected_sha256: String,
    previous_sha256: String,
}

#[derive(Clone, Debug)]
struct AppPaths {
    data_root: PathBuf,
    runtime_root: PathBuf,
    npm_prefix: PathBuf,
    dsh_home: PathBuf,
    log_root: PathBuf,
    state_root: PathBuf,
    update_root: PathBuf,
    temp_root: PathBuf,
    npm_cache: PathBuf,
}

impl AppPaths {
    fn from_data_root(data_root: PathBuf) -> Self {
        Self {
            runtime_root: data_root.join(RUNTIME_DIRECTORY),
            npm_prefix: data_root.join(NPM_GLOBAL_DIRECTORY),
            dsh_home: data_root.join(DSH_PROFILE_DIRECTORY).join(".dsh"),
            log_root: data_root.join(DSH_LOG_DIRECTORY),
            state_root: data_root.join(DSH_STATE_DIRECTORY),
            update_root: data_root.join(DSH_UPDATE_DIRECTORY),
            temp_root: data_root.join(DSH_TEMP_DIRECTORY),
            npm_cache: data_root.join(DSH_CACHE_DIRECTORY).join("npm"),
            data_root,
        }
    }

    fn pid_path(&self) -> PathBuf {
        self.state_root.join(NATIVE_DSH_PID_FILE)
    }

    fn ensure_layout(&self) -> Result<(), String> {
        for path in [
            &self.data_root,
            &self.runtime_root,
            &self.npm_prefix,
            &self.dsh_home,
            &self.log_root,
            &self.state_root,
            &self.update_root,
            &self.temp_root,
            &self.npm_cache,
        ] {
            ensure_safe_directory(path)?;
        }
        Ok(())
    }
}

static APP_PATHS: OnceLock<Result<AppPaths, String>> = OnceLock::new();
const ACTION_MUTEX_NAME: &str = "Local\\DeepSeekHarness.DshLauncher.Action";

struct AppState {
    hicon: usize,
    black_hicon: usize,
    tray_hicon: AtomicUsize,
    taskbar_created_message: u32,
    data_root: String,
    background_brush: usize,
    title_font: AtomicUsize,
    body_font: AtomicUsize,
    small_font: AtomicUsize,
    button_font: AtomicUsize,
    status_hwnd: AtomicUsize,
    tray_added: AtomicBool,
    busy: AtomicBool,
    cancelable: AtomicBool,
    health_checking: AtomicBool,
    diagnostics_running: AtomicBool,
    ui_page: AtomicUsize,
    hover_levels: [AtomicUsize; HOVER_BUTTON_COUNT],
    messages: Mutex<VecDeque<String>>,
    pending_diagnostic_report: Mutex<Option<String>>,
    last_health: Mutex<String>,
    current_status: Mutex<String>,
    health_publish_after: Mutex<Instant>,
    high_contrast: AtomicBool,
    dark_mode: AtomicBool,
    close_notice_shown: AtomicBool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UiPage {
    Home,
    Tools,
}

impl UiPage {
    fn from_atomic(value: usize) -> Self {
        if value == 1 {
            Self::Tools
        } else {
            Self::Home
        }
    }

    fn as_atomic(self) -> usize {
        match self {
            Self::Home => 0,
            Self::Tools => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NativeDshProcess {
    pid: u32,
    started_at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct LauncherVersion {
    major: u64,
    minor: u64,
    patch: u64,
}

impl std::fmt::Display for LauncherVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Debug, PartialEq, Eq)]
struct LauncherRelease {
    version: LauncherVersion,
    asset_url: String,
    checksum_url: String,
}

#[derive(Debug, PartialEq, Eq)]
enum LauncherUpdateOutcome {
    UpToDate(String),
    Scheduled(String),
}

static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);
static RUNTIME_MANIFEST: OnceLock<Result<RuntimeManifest, String>> = OnceLock::new();

impl LauncherUpdateOutcome {
    fn should_exit(&self) -> bool {
        matches!(self, Self::Scheduled(_))
    }

    fn into_message(self) -> String {
        match self {
            Self::UpToDate(message) | Self::Scheduled(message) => message,
        }
    }
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
    // DPI must be declared before any error dialog, class registration, or window creation.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let args: Vec<String> = env::args().collect();
    if let Some(transaction_path) = parse_self_update(&args) {
        attach_parent_console();
        let code = match apply_self_update(&transaction_path) {
            Ok(message) => {
                write_cli_message(&message, false);
                0
            }
            Err(error) => {
                write_cli_message(&error, true);
                1
            }
        };
        std::process::exit(code);
    }
    if is_release_smoke(&args) {
        attach_parent_console();
        let code = match run_release_smoke() {
            Ok(message) => {
                write_cli_message(&message, false);
                0
            }
            Err(error) => {
                write_cli_message(&error, true);
                1
            }
        };
        std::process::exit(code);
    }
    let health_path = match self_update_health_path(&args) {
        Ok(path) => path,
        Err(error) => {
            attach_parent_console();
            write_cli_message(&error, true);
            std::process::exit(2);
        }
    };
    let action = match parse_action(&args) {
        Ok(action) => action,
        Err(error) => {
            attach_parent_console();
            write_cli_message(&error, true);
            std::process::exit(2);
        }
    };
    if let Some(action) = action {
        attach_parent_console();
        let code = match acquire_action_mutex()
            .ok_or_else(|| "已有启动器操作正在执行，请稍后重试".to_owned())
            .and_then(|_lock| ensure_not_elevated().and_then(|_| execute_action(action)))
        {
            Ok(message) => {
                write_cli_message(&message, false);
                0
            }
            Err(error) => {
                write_cli_message(&error, true);
                1
            }
        };
        std::process::exit(code);
    }

    let _instance = match acquire_single_instance() {
        Some(value) => value,
        None => return,
    };

    if let Err(error) = ensure_not_elevated().and_then(|_| run_app(health_path)) {
        show_error_box(&error);
        std::process::exit(1);
    }
}

fn is_release_smoke(args: &[String]) -> bool {
    args.len() == 2 && args[1] == "--release-smoke"
}

fn run_release_smoke() -> Result<String, String> {
    let paths = app_paths()?;
    let executable = env::current_exe().map_err(|error| format!("无法定位启动器目录：{error}"))?;
    let executable_dir = executable
        .parent()
        .ok_or_else(|| "无法定位启动器目录".to_owned())?;
    verify_portable_manifest(executable_dir)?;
    paths.ensure_layout()?;
    Ok(format!("DSH启动器 v{APP_VERSION} 便携包初始化检查通过"))
}

fn parse_self_update(args: &[String]) -> Option<PathBuf> {
    if args.len() != 3 || args[1] != "--apply-self-update" {
        return None;
    }
    let transaction = PathBuf::from(&args[2]);
    (!transaction.as_os_str().is_empty()).then_some(transaction)
}

fn self_update_health_path(args: &[String]) -> Result<Option<PathBuf>, String> {
    let mut result = None;
    let mut index = 1;
    while index < args.len() {
        if args[index] == "--self-update-health" {
            let value = args
                .get(index + 1)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| "--self-update-health 缺少握手文件路径".to_owned())?;
            if result.is_some() {
                return Err("--self-update-health 只能指定一次".to_owned());
            }
            result = Some(PathBuf::from(value));
            index += 2;
        } else {
            index += 1;
        }
    }
    Ok(result)
}

fn parse_action(args: &[String]) -> Result<Option<Action>, String> {
    let mut action = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--action" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--action 缺少操作名称".to_owned())?;
                if action.is_some() {
                    return Err("--action 只能指定一次".to_owned());
                }
                action = Some(Action::from_name(value).ok_or_else(|| {
                    format!(
                        "未知操作“{value}”；可用操作：start、restart、stop、upgrade、repair、migrate、launcher-update、open、data"
                    )
                })?);
                index += 2;
            }
            "--data-dir" => {
                let value = args
                    .get(index + 1)
                    .filter(|value| !value.trim().is_empty() && !value.starts_with("--"));
                if value.is_none() {
                    return Err("--data-dir 缺少目录路径".to_owned());
                }
                index += 2;
            }
            value if value.starts_with("--data-dir=") => {
                if value.trim_start_matches("--data-dir=").trim().is_empty() {
                    return Err("--data-dir= 缺少目录路径".to_owned());
                }
                index += 1;
            }
            "--self-update-health" => {
                if args
                    .get(index + 1)
                    .filter(|value| !value.trim().is_empty())
                    .is_none()
                {
                    return Err("--self-update-health 缺少握手文件路径".to_owned());
                }
                index += 2;
            }
            value => return Err(format!("未知参数：{value}")),
        }
    }
    Ok(action)
}

fn data_dir_override(args: &[String]) -> Result<Option<PathBuf>, String> {
    let mut result = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--action" => index += 2,
            "--self-update-health" => index += 2,
            "--data-dir" => {
                let value = args
                    .get(index + 1)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "--data-dir 缺少目录路径".to_owned())?;
                if result.is_some() {
                    return Err("--data-dir 只能指定一次".to_owned());
                }
                result = Some(PathBuf::from(value));
                index += 2;
            }
            value if value.starts_with("--data-dir=") => {
                let value = value.trim_start_matches("--data-dir=");
                if value.trim().is_empty() {
                    return Err("--data-dir= 缺少目录路径".to_owned());
                }
                if result.is_some() {
                    return Err("--data-dir 只能指定一次".to_owned());
                }
                result = Some(PathBuf::from(value));
                index += 1;
            }
            _ => index += 1,
        }
    }
    Ok(result)
}

fn acquire_single_instance() -> Option<MutexGuard> {
    let name = single_instance_mutex_name()?;
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

fn single_instance_mutex_name() -> Option<Vec<u16>> {
    let executable = env::current_exe().ok()?;
    let executable = fs::canonicalize(&executable).unwrap_or(executable);
    Some(to_wide(&single_instance_mutex_name_for_path(&executable)))
}

fn single_instance_mutex_name_for_path(executable: &Path) -> String {
    let normalized = executable.to_string_lossy().to_lowercase();
    let mut digest = Sha256::new();
    digest.update(normalized.as_bytes());
    format!("Local\\DeepSeekHarness.DshLauncher.{:x}", digest.finalize())
}

fn acquire_action_mutex() -> Option<MutexGuard> {
    let name = to_wide(ACTION_MUTEX_NAME);
    let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
    if handle.is_null() || unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        if !handle.is_null() {
            unsafe {
                CloseHandle(handle);
            }
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
        Action::LauncherUpdate => update_launcher(),
        Action::Repair => repair_runtime(),
        Action::Migrate => migrate_legacy_data(),
        Action::CleanupLegacy => cleanup_legacy_data_to_recycle_bin(&|_| {}),
        Action::OpenWeb => open_web_ui(),
        Action::OpenData => open_data_directory(),
    }
}

fn update_launcher() -> Result<String, String> {
    update_launcher_with_progress(&|_| {}).map(LauncherUpdateOutcome::into_message)
}

fn update_launcher_with_progress(progress: &dyn Fn(&str)) -> Result<LauncherUpdateOutcome, String> {
    progress("正在检查启动器更新...");
    let paths = app_paths()?;
    let release = latest_launcher_release()?;
    let current_version = parse_launcher_version(APP_VERSION)
        .ok_or_else(|| format!("当前启动器版本号无效：{APP_VERSION}"))?;
    if release.version <= current_version {
        return Ok(LauncherUpdateOutcome::UpToDate(format!(
            "启动器已是最新版本 · v{APP_VERSION}"
        )));
    }

    progress(&format!("发现启动器 v{}，正在下载...", release.version));
    let current_exe = env::current_exe().map_err(|error| format!("无法定位当前启动器：{error}"))?;
    let executable_directory = current_exe
        .parent()
        .ok_or_else(|| "无法定位启动器所在目录".to_owned())?;
    if !ensure_writable_directory(executable_directory) {
        return Err(format!(
            "启动器所在目录不可写，无法安全更新：{}",
            executable_directory.display()
        ));
    }
    cleanup_stale_update_directories(&paths)?;
    let update_directory = paths.update_root.join(format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    ensure_safe_directory(&update_directory)
        .map_err(|error| format!("无法创建启动器更新暂存目录：{error}"))?;

    let result = (|| {
        let source = update_directory.join(SELF_UPDATE_NEW_FILE);
        let checksum_file = update_directory.join(SELF_UPDATE_CHECKSUM_FILE);
        let helper = update_directory.join(SELF_UPDATE_HELPER_FILE);
        download_launcher_file(&release.asset_url, &source, "下载启动器更新")?;
        download_launcher_file(&release.checksum_url, &checksum_file, "下载启动器校验文件")?;

        progress("正在校验启动器更新...");
        verify_launcher_download(&source, &checksum_file)?;
        fs::copy(&current_exe, &helper)
            .map_err(|error| format!("无法准备启动器更新助手：{error}"))?;

        let expected_sha256 = parse_sha256(
            &fs::read_to_string(&checksum_file)
                .map_err(|error| format!("无法读取启动器 SHA-256 校验文件：{error}"))?,
        )
        .ok_or_else(|| "启动器 SHA-256 校验文件格式无效".to_owned())?;
        let previous_sha256 = calculate_sha256(&current_exe)?;
        let transaction_path = update_directory.join(SELF_UPDATE_TRANSACTION_FILE);
        let transaction = SelfUpdateTransaction {
            parent_pid: std::process::id(),
            transaction_id: update_directory
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| "启动器更新事务目录名称无效".to_owned())?
                .to_owned(),
            source: source.clone(),
            target: current_exe.clone(),
            checksum_file: checksum_file.clone(),
            backup: update_directory.join("DSH-Launcher.exe.bak"),
            health_file: update_directory.join(SELF_UPDATE_HEALTH_FILE),
            expected_version: release.version.to_string(),
            expected_sha256,
            previous_sha256,
        };
        write_self_update_transaction(&transaction_path, &transaction)?;

        let mut command = hidden_command(&helper);
        command.arg("--apply-self-update").arg(&transaction_path);
        command
            .spawn()
            .map_err(|error| format!("无法启动启动器更新助手：{error}"))?;

        Ok(LauncherUpdateOutcome::Scheduled(format!(
            "已安排启动器更新到 v{}，程序将自动重启",
            release.version
        )))
    })();

    match result {
        Ok(outcome) => Ok(outcome),
        Err(error) => {
            let cleanup = fs::remove_dir_all(&update_directory);
            match cleanup {
                Ok(()) => Err(error),
                Err(cleanup_error) if cleanup_error.kind() == std::io::ErrorKind::NotFound => {
                    Err(error)
                }
                Err(cleanup_error) => Err(format!(
                    "{error}；启动器更新暂存目录清理失败：{cleanup_error}"
                )),
            }
        }
    }
}

fn latest_launcher_release() -> Result<LauncherRelease, String> {
    let script = format!(
        "$ErrorActionPreference = 'Stop';\n\
         $headers = @{{ 'Accept' = 'application/vnd.github+json'; 'User-Agent' = {user_agent} }};\n\
         $release = Invoke-RestMethod -Headers $headers -Uri {api_url} -TimeoutSec 30;\n\
         $asset = @($release.assets | Where-Object {{ $_.name -eq {asset_name} }}) | Select-Object -First 1;\n\
         $checksum = @($release.assets | Where-Object {{ $_.name -eq {checksum_name} }}) | Select-Object -First 1;\n\
         if ($null -eq $asset -or $null -eq $checksum) {{ throw 'GitHub Release 缺少启动器或 SHA-256 资产' }};\n\
         [Console]::WriteLine(\"$($release.tag_name)`t$($asset.browser_download_url)`t$($checksum.browser_download_url)\")",
        user_agent = powershell_literal(&format!("DSH-Launcher/{APP_VERSION}")),
        api_url = powershell_literal(LAUNCHER_RELEASE_API_URL),
        asset_name = powershell_literal(LAUNCHER_ASSET_NAME),
        checksum_name = powershell_literal(LAUNCHER_CHECKSUM_ASSET_NAME),
    );
    let output = run_powershell_script(&script, "检查启动器更新")?;
    parse_launcher_release(&output)
}

fn parse_launcher_release(value: &str) -> Result<LauncherRelease, String> {
    let line = value
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| line.split('\t').count() == 3)
        .ok_or_else(|| "无法解析 GitHub Release 信息".to_owned())?;
    let parts: Vec<&str> = line.split('\t').collect();
    let version = parse_launcher_version(parts[0])
        .ok_or_else(|| format!("GitHub Release 版本号无效：{}", parts[0]))?;
    if !parts[1].starts_with(LAUNCHER_ASSET_URL_PREFIX)
        || !parts[1].ends_with("/DSH-Launcher.exe")
        || !parts[2].starts_with(LAUNCHER_ASSET_URL_PREFIX)
        || !parts[2].ends_with("/DSH-Launcher.exe.sha256")
    {
        return Err("拒绝使用非本项目 GitHub Release 的更新资产".to_owned());
    }
    Ok(LauncherRelease {
        version,
        asset_url: parts[1].to_owned(),
        checksum_url: parts[2].to_owned(),
    })
}

fn parse_launcher_version(value: &str) -> Option<LauncherVersion> {
    let value = value.trim().strip_prefix('v').unwrap_or(value.trim());
    let mut parts = value.split('.');
    let version = LauncherVersion {
        major: parts.next()?.parse().ok()?,
        minor: parts.next()?.parse().ok()?,
        patch: parts.next()?.parse().ok()?,
    };
    if parts.next().is_some() {
        return None;
    }
    Some(version)
}

fn download_launcher_file(url: &str, destination: &Path, description: &str) -> Result<(), String> {
    if !url.starts_with(LAUNCHER_ASSET_URL_PREFIX) {
        return Err("拒绝下载非本项目 GitHub Release 资产".to_owned());
    }
    ensure_safe_file_destination(destination, "启动器更新下载文件")?;
    let timeout = SELF_UPDATE_DOWNLOAD_TIMEOUT.as_secs();
    let script = format!(
        "$ErrorActionPreference = 'Stop';\n\
         $ProgressPreference = 'SilentlyContinue';\n\
         Invoke-WebRequest -UseBasicParsing -Headers @{{ 'Accept' = 'application/octet-stream'; 'User-Agent' = {user_agent} }} -Uri {url} -OutFile {destination} -TimeoutSec {timeout} | Out-Null",
        user_agent = powershell_literal(&format!("DSH-Launcher/{APP_VERSION}")),
        url = powershell_literal(url),
        destination = powershell_literal(&destination.to_string_lossy()),
    );
    run_powershell_script_with_timeout(&script, description, SELF_UPDATE_DOWNLOAD_TIMEOUT)?;
    let metadata = fs::metadata(destination)
        .map_err(|error| format!("{description}后无法读取文件：{error}"))?;
    if metadata.len() == 0 {
        return Err(format!("{description}得到空文件"));
    }
    Ok(())
}

fn verify_launcher_download(source: &Path, checksum_file: &Path) -> Result<(), String> {
    let expected_text = fs::read_to_string(checksum_file)
        .map_err(|error| format!("无法读取启动器 SHA-256 校验文件：{error}"))?;
    let expected =
        parse_sha256(&expected_text).ok_or_else(|| "启动器 SHA-256 校验文件格式无效".to_owned())?;
    let actual = calculate_sha256(source)?;
    if actual != expected {
        return Err(format!(
            "启动器更新校验失败：期望 {expected}，实际 {actual}"
        ));
    }
    Ok(())
}

fn parse_sha256(value: &str) -> Option<String> {
    let value = value.split_whitespace().next()?;
    if value.len() != 64 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
        return None;
    }
    Some(value.to_ascii_lowercase())
}

fn calculate_sha256(path: &Path) -> Result<String, String> {
    let mut file =
        fs::File::open(path).map_err(|error| format!("无法读取文件以计算 SHA-256：{error}"))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 128 * 1024];
    loop {
        let length = file
            .read(&mut buffer)
            .map_err(|error| format!("无法读取文件以计算 SHA-256：{error}"))?;
        if length == 0 {
            break;
        }
        digest.update(&buffer[..length]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn run_powershell_script(script: &str, description: &str) -> Result<String, String> {
    run_powershell_script_with_timeout(script, description, DSH_UPDATE_COMMAND_TIMEOUT)
}

fn run_powershell_script_with_timeout(
    script: &str,
    description: &str,
    timeout: Duration,
) -> Result<String, String> {
    // Windows 10/11 already provide Windows PowerShell 5.1. DSH resolves
    // that executable when PowerShell 7 is not installed, so the launcher
    // does not ship a second 245 MiB runtime just for downloads and archive
    // extraction.
    let mut command = hidden_command("powershell.exe");
    command.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        script,
    ]);
    if let Ok(paths) = app_paths() {
        command
            .env("TEMP", &paths.temp_root)
            .env("TMP", &paths.temp_root);
    }
    run_native_update_command_with_timeout(&mut command, description, timeout)
}

fn powershell_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn apply_self_update(transaction_path: &Path) -> Result<String, String> {
    let transaction = read_self_update_transaction(transaction_path)?;
    validate_self_update_transaction(transaction_path, &transaction)?;
    if transaction.parent_pid == std::process::id() {
        return Err("拒绝由自身进程执行启动器替换".to_owned());
    }
    let parent_command_line = process_command_line(transaction.parent_pid)?
        .ok_or_else(|| "无法验证启动器更新父进程".to_owned())?;
    let parent_image = process_image_path(transaction.parent_pid)?
        .ok_or_else(|| "无法读取启动器更新父进程映像路径".to_owned())?;
    if !same_windows_path(&transaction.target, &parent_image) {
        return Err("启动器更新目标与父进程映像不一致，已拒绝替换".to_owned());
    }
    let normalized_parent = parent_command_line.to_ascii_lowercase().replace('\\', "/");
    if !normalized_parent.contains("dsh-launcher.exe")
        || normalized_parent.contains("--apply-self-update")
    {
        return Err("启动器更新父进程校验失败，已拒绝替换".to_owned());
    }
    wait_for_process_exit(transaction.parent_pid)?;
    let source_volume = path_drive_letter(&transaction.source);
    let target_volume = path_drive_letter(&transaction.target);
    if source_volume.is_none() || source_volume != target_volume {
        return Err("拒绝执行跨磁盘启动器更新；更新暂存文件必须与 EXE 位于同一磁盘".to_owned());
    }
    let source_wide = to_wide(&transaction.source.to_string_lossy());
    let target_wide = to_wide(&transaction.target.to_string_lossy());
    let backup_wide = to_wide(&transaction.backup.to_string_lossy());
    let replaced = unsafe {
        ReplaceFileW(
            target_wide.as_ptr(),
            source_wide.as_ptr(),
            backup_wide.as_ptr(),
            REPLACEFILE_WRITE_THROUGH,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if replaced == 0 {
        return Err(format!("替换启动器失败：{}", unsafe {
            GetLastError()
        }));
    }

    let mut command = hidden_command(&transaction.target);
    command
        .arg("--self-update-health")
        .arg(&transaction.health_file);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let restore = restore_self_update_backup(&transaction);
            return Err(format!(
                "启动更新后的启动器失败：{error}；{}",
                restore
                    .map(|()| "旧版本已恢复".to_owned())
                    .unwrap_or_else(|restore_error| format!("旧版本恢复失败：{restore_error}"))
            ));
        }
    };
    let deadline = Instant::now() + SELF_UPDATE_WAIT_TIMEOUT;
    while Instant::now() < deadline {
        if transaction.health_file.is_file() {
            let healthy = fs::read_to_string(&transaction.health_file)
                .map(|value| {
                    health_handshake_matches(&value, &transaction.expected_version, child.id())
                })
                .unwrap_or(false);
            if healthy
                && child
                    .try_wait()
                    .map_err(|error| error.to_string())?
                    .is_none()
            {
                fs::remove_file(&transaction.backup).map_err(|error| {
                    format!("新启动器已完成健康握手，但删除旧版本备份失败：{error}")
                })?;
                for file in [
                    &transaction.source,
                    &transaction.checksum_file,
                    &transaction.health_file,
                    transaction_path,
                ] {
                    if let Err(error) = fs::remove_file(file) {
                        if error.kind() != std::io::ErrorKind::NotFound {
                            return Err(format!("启动器更新完成但清理事务文件失败：{error}"));
                        }
                    }
                }
                return Ok("启动器更新完成，健康握手已确认".to_owned());
            }
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("无法确认更新后的启动器状态：{error}"))?
        {
            let restore = restore_self_update_backup(&transaction);
            return Err(format!(
                "更新后的启动器提前退出（{status}）；{}",
                restore
                    .map(|()| "旧版本已恢复".to_owned())
                    .unwrap_or_else(|restore_error| format!("旧版本恢复失败：{restore_error}"))
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
    let termination = terminate_process_tree(child.id(), "更新后的启动器")
        .and_then(|_| wait_for_process_exit(child.id()));
    let restore = restore_self_update_backup(&transaction);
    let termination_detail = termination
        .err()
        .map(|error| format!("更新后进程清理失败：{error}；"))
        .unwrap_or_default();
    Err(format!(
        "更新后的启动器未在 {} 秒内完成健康握手；{termination_detail}{}",
        SELF_UPDATE_WAIT_TIMEOUT.as_secs(),
        restore
            .map(|()| "旧版本已恢复".to_owned())
            .unwrap_or_else(|restore_error| format!("旧版本恢复失败：{restore_error}"))
    ))
}

fn restore_self_update_backup(transaction: &SelfUpdateTransaction) -> Result<(), String> {
    if !transaction.backup.is_file() {
        return Err("启动器更新备份不存在".to_owned());
    }
    let backup_wide = to_wide(&transaction.backup.to_string_lossy());
    let target_wide = to_wide(&transaction.target.to_string_lossy());
    let result = unsafe {
        MoveFileExW(
            backup_wide.as_ptr(),
            target_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(format!("恢复启动器备份失败：{}", unsafe {
            GetLastError()
        }));
    }
    let actual = calculate_sha256(&transaction.target)?;
    if actual != transaction.previous_sha256 {
        return Err(format!(
            "恢复启动器后哈希不一致：期望 {}，实际 {actual}",
            transaction.previous_sha256
        ));
    }
    Ok(())
}

fn write_self_update_transaction(
    path: &Path,
    transaction: &SelfUpdateTransaction,
) -> Result<(), String> {
    let value = serde_json::json!({
        "parent_pid": transaction.parent_pid,
        "transaction_id": transaction.transaction_id,
        "source": transaction.source,
        "target": transaction.target,
        "checksum_file": transaction.checksum_file,
        "backup": transaction.backup,
        "health_file": transaction.health_file,
        "expected_version": transaction.expected_version,
        "expected_sha256": transaction.expected_sha256,
        "previous_sha256": transaction.previous_sha256,
    });
    let encoded = serde_json::to_vec_pretty(&value)
        .map_err(|error| format!("无法序列化启动器更新事务：{error}"))?;
    let partial = path.with_extension("json.partial");
    ensure_safe_file_destination(&partial, "启动器更新事务暂存文件")?;
    ensure_safe_file_destination(path, "启动器更新事务")?;
    fs::write(&partial, encoded).map_err(|error| format!("无法写入启动器更新事务：{error}"))?;
    fs::rename(&partial, path).map_err(|error| format!("无法提交启动器更新事务：{error}"))
}

fn read_self_update_transaction(path: &Path) -> Result<SelfUpdateTransaction, String> {
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(path).map_err(|error| format!("无法读取启动器更新事务：{error}"))?,
    )
    .map_err(|error| format!("启动器更新事务格式无效：{error}"))?;
    let get_string = |name: &str| {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| format!("启动器更新事务缺少 {name}"))
    };
    let parent_pid = value
        .get("parent_pid")
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value != 0)
        .ok_or_else(|| "启动器更新事务的父进程无效".to_owned())?;
    let transaction_id = value
        .get("transaction_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "启动器更新事务缺少 transaction_id".to_owned())?;
    let expected_sha256 = value
        .get("expected_sha256")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_sha256)
        .ok_or_else(|| "启动器更新事务的目标哈希无效".to_owned())?;
    let previous_sha256 = value
        .get("previous_sha256")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_sha256)
        .ok_or_else(|| "启动器更新事务的旧版本哈希无效".to_owned())?;
    let expected_version = value
        .get("expected_version")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_launcher_version)
        .map(|version| version.to_string())
        .ok_or_else(|| "启动器更新事务的目标版本无效".to_owned())?;
    Ok(SelfUpdateTransaction {
        parent_pid,
        transaction_id,
        source: get_string("source")?,
        target: get_string("target")?,
        checksum_file: get_string("checksum_file")?,
        backup: get_string("backup")?,
        health_file: get_string("health_file")?,
        expected_version,
        expected_sha256,
        previous_sha256,
    })
}

fn validate_self_update_transaction(
    transaction_path: &Path,
    transaction: &SelfUpdateTransaction,
) -> Result<(), String> {
    let directory = transaction_path
        .parent()
        .ok_or_else(|| "启动器更新事务目录无效".to_owned())?;
    if transaction_path.file_name().and_then(|name| name.to_str())
        != Some(SELF_UPDATE_TRANSACTION_FILE)
    {
        return Err("启动器更新事务路径无效".to_owned());
    }
    let transaction_metadata = fs::symlink_metadata(transaction_path)
        .map_err(|error| format!("无法读取启动器更新事务：{error}"))?;
    if is_reparse_point(&transaction_metadata) || !transaction_metadata.is_file() {
        return Err("启动器更新事务不是普通文件".to_owned());
    }
    if !transaction_id_matches_parent(&transaction.transaction_id, transaction.parent_pid) {
        return Err("启动器更新事务标识无效".to_owned());
    }
    if directory.file_name().and_then(|name| name.to_str())
        != Some(transaction.transaction_id.as_str())
    {
        return Err("启动器更新事务标识与目录不一致".to_owned());
    }
    ensure_existing_directory(directory, "启动器更新事务目录")?;
    let target_directory = transaction
        .target
        .parent()
        .ok_or_else(|| "启动器更新目标目录无效".to_owned())?;
    let update_root = target_directory
        .join(DATA_DIRECTORY)
        .join(DSH_UPDATE_DIRECTORY);
    if !path_is_same_or_below(directory, &update_root) || same_windows_path(directory, &update_root)
    {
        return Err("启动器更新事务不在目标便携包的 data\\updates 下".to_owned());
    }
    if parse_launcher_version(&transaction.expected_version).is_none() {
        return Err("启动器更新事务的目标版本无效".to_owned());
    }
    for (label, path) in [
        ("更新文件", &transaction.source),
        ("校验文件", &transaction.checksum_file),
        ("备份文件", &transaction.backup),
        ("健康握手文件", &transaction.health_file),
    ] {
        if path.parent() != Some(directory) {
            return Err(format!("拒绝使用事务目录外的{label}"));
        }
        if let Ok(metadata) = fs::symlink_metadata(path) {
            if is_reparse_point(&metadata) {
                return Err(format!("拒绝使用符号链接或重解析点{label}"));
            }
            if label != "备份文件" && label != "健康握手文件" && !metadata.is_file() {
                return Err(format!("{label}不是普通文件"));
            }
        } else if label == "备份文件" || label == "健康握手文件" {
            // These files are intentionally absent before ReplaceFileW or before
            // the new launcher writes its first health handshake.
        } else {
            return Err(format!("{label}不存在"));
        }
    }
    let target_name = transaction
        .target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let target_metadata = fs::symlink_metadata(&transaction.target)
        .map_err(|error| format!("无法读取启动器更新目标：{error}"))?;
    if !target_name.eq_ignore_ascii_case("DSH-Launcher.exe")
        || is_reparse_point(&target_metadata)
        || !target_metadata.is_file()
    {
        return Err("启动器更新目标路径无效".to_owned());
    }
    if !transaction.source.is_file() || !transaction.checksum_file.is_file() {
        return Err("启动器更新文件或校验文件不存在".to_owned());
    }
    let checksum = parse_sha256(
        &fs::read_to_string(&transaction.checksum_file)
            .map_err(|error| format!("无法读取启动器校验文件：{error}"))?,
    )
    .ok_or_else(|| "启动器校验文件格式无效".to_owned())?;
    if checksum != transaction.expected_sha256 {
        return Err("启动器事务哈希与校验文件不一致".to_owned());
    }
    let actual = calculate_sha256(&transaction.source)?;
    if actual != transaction.expected_sha256 {
        return Err(format!(
            "启动器更新哈希不一致：期望 {}，实际 {actual}",
            transaction.expected_sha256
        ));
    }
    let previous = calculate_sha256(&transaction.target)?;
    if previous != transaction.previous_sha256 {
        return Err("启动器目标已在更新前发生变化，已拒绝替换".to_owned());
    }
    Ok(())
}

fn transaction_id_matches_parent(transaction_id: &str, parent_pid: u32) -> bool {
    let prefix = format!("{parent_pid}-");
    transaction_id.strip_prefix(&prefix).is_some_and(|nonce| {
        !nonce.is_empty() && nonce.chars().all(|character| character.is_ascii_digit())
    })
}

fn wait_for_process_exit(pid: u32) -> Result<(), String> {
    let deadline = Instant::now() + SELF_UPDATE_WAIT_TIMEOUT;
    while Instant::now() < deadline {
        if !is_process_running(pid)? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err("等待旧版启动器退出超时，更新未执行".to_owned())
}

fn is_process_running(pid: u32) -> Result<bool, String> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return Ok(false);
    }
    let mut exit_code = 0;
    let result = unsafe { GetExitCodeProcess(handle, &mut exit_code) };
    unsafe {
        CloseHandle(handle);
    }
    if result == 0 {
        return Err(format!("无法读取旧版启动器状态：{}", unsafe {
            GetLastError()
        }));
    }
    Ok(exit_code == PROCESS_STILL_ACTIVE)
}

fn process_image_path(pid: u32) -> Result<Option<PathBuf>, String> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return Ok(None);
    }
    let mut buffer = vec![0u16; 32_768];
    let mut length = buffer.len() as u32;
    let result = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut length) };
    let error = if result == 0 {
        Some(unsafe { GetLastError() })
    } else {
        None
    };
    unsafe {
        CloseHandle(handle);
    }
    if let Some(error) = error {
        return Err(format!("无法读取进程映像路径：{error}"));
    }
    buffer.truncate(length as usize);
    Ok(Some(PathBuf::from(String::from_utf16_lossy(&buffer))))
}

fn same_windows_path(left: &Path, right: &Path) -> bool {
    let normalize = |path: &Path| {
        fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_ascii_lowercase()
    };
    normalize(left) == normalize(right)
}

fn listening_process_ids(port: u16) -> Result<Vec<u32>, String> {
    let mut command = hidden_command("netstat.exe");
    command.args(["-ano", "-p", "tcp"]);
    let output = run_native_command(&mut command, "读取端口占用")?;
    let mut process_ids = Vec::new();
    for line in output.lines() {
        if let Some(pid) = parse_listening_pid(line, port) {
            if !process_ids.contains(&pid) {
                process_ids.push(pid);
            }
        }
    }
    Ok(process_ids)
}

fn parse_listening_pid(line: &str, port: u16) -> Option<u32> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 5
        || !fields[0].eq_ignore_ascii_case("TCP")
        || !fields[3].eq_ignore_ascii_case("LISTENING")
    {
        return None;
    }
    let local_port = fields[1].rsplit(':').next()?.parse::<u16>().ok()?;
    if local_port != port {
        return None;
    }
    fields.last()?.parse::<u32>().ok().filter(|pid| *pid != 0)
}

fn process_command_line(pid: u32) -> Result<Option<String>, String> {
    let handle =
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid) };
    if handle.is_null() {
        return Err(format!("无法读取 DSH 进程命令行：{}", unsafe {
            GetLastError()
        }));
    }
    let result = unsafe { read_process_command_line(handle) };
    unsafe {
        CloseHandle(handle);
    }
    result
}

unsafe fn read_process_command_line(handle: HANDLE) -> Result<Option<String>, String> {
    let mut basic_information = std::mem::MaybeUninit::<ProcessBasicInformation>::uninit();
    let mut return_length = 0;
    let status = NtQueryInformationProcess(
        handle,
        0,
        basic_information.as_mut_ptr().cast::<c_void>(),
        std::mem::size_of::<ProcessBasicInformation>() as u32,
        &mut return_length,
    );
    if status != 0 {
        return Err(format!("无法读取 DSH 进程信息：NTSTATUS 0x{status:08x}"));
    }
    let basic_information = basic_information.assume_init();
    if basic_information.peb_base_address.is_null() {
        return Ok(None);
    }

    let parameters_address = (basic_information.peb_base_address as usize
        + PEB_PROCESS_PARAMETERS_OFFSET) as *const c_void;
    let parameters: *mut c_void = read_process_value(handle, parameters_address)?;
    if parameters.is_null() {
        return Ok(None);
    }

    let command_line_address =
        (parameters as usize + PROCESS_PARAMETERS_COMMAND_LINE_OFFSET) as *const c_void;
    let command_line: UnicodeString = read_process_value(handle, command_line_address)?;
    if command_line.length == 0 || command_line.buffer.is_null() {
        return Ok(None);
    }

    let character_count = usize::from(command_line.length) / std::mem::size_of::<u16>();
    let mut buffer = vec![0u16; character_count];
    let mut bytes_read = 0;
    let result = ReadProcessMemory(
        handle,
        command_line.buffer.cast::<c_void>(),
        buffer.as_mut_ptr().cast::<c_void>(),
        usize::from(command_line.length),
        &mut bytes_read,
    );
    if result == 0 || bytes_read < usize::from(command_line.length) {
        return Err(format!("无法读取 DSH 进程命令行内容：{}", GetLastError()));
    }
    String::from_utf16(&buffer)
        .map(Some)
        .map_err(|error| format!("无法解析 DSH 进程命令行：{error}"))
}

unsafe fn read_process_value<T: Copy>(handle: HANDLE, address: *const c_void) -> Result<T, String> {
    let mut value = std::mem::MaybeUninit::<T>::uninit();
    let mut bytes_read = 0;
    let result = ReadProcessMemory(
        handle,
        address,
        value.as_mut_ptr().cast::<c_void>(),
        std::mem::size_of::<T>(),
        &mut bytes_read,
    );
    if result == 0 || bytes_read < std::mem::size_of::<T>() {
        return Err(format!("无法读取 DSH 进程内存：{}", GetLastError()));
    }
    Ok(value.assume_init())
}

fn is_verified_dsh_command(command_line: &str, port: u16) -> bool {
    let normalized = command_line.to_ascii_lowercase().replace('\\', "/");
    if !(normalized.contains("@deepseek-ai/dsh/") && normalized.contains("/lib/bin.js")) {
        return false;
    }

    let tokens: Vec<&str> = normalized
        .split_whitespace()
        .map(|token| token.trim_matches('"'))
        .collect();
    let has_web_command = tokens.contains(&"web");
    let port_argument = port.to_string();
    let port_equals_argument = format!("--port={port_argument}");
    let has_explicit_port_argument = tokens
        .iter()
        .any(|token| *token == "--port" || token.starts_with("--port="));
    let port_matches = if !has_explicit_port_argument {
        true
    } else {
        tokens.iter().any(|token| *token == port_equals_argument)
            || tokens
                .windows(2)
                .any(|window| window[0] == "--port" && window[1] == port_argument)
    };
    has_web_command && port_matches
}

fn attach_parent_console() {
    unsafe {
        let valid = |handle: HANDLE| !handle.is_null() && handle as isize != -1;
        if valid(GetStdHandle(STD_OUTPUT_HANDLE)) && valid(GetStdHandle(STD_ERROR_HANDLE)) {
            return;
        }
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        let open_console = |name: &str| {
            let name = to_wide(name);
            CreateFileW(
                name.as_ptr(),
                FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if !valid(GetStdHandle(STD_INPUT_HANDLE)) {
            let input = open_console("\\\\.\\CONIN$");
            if valid(input) {
                let _ = SetStdHandle(STD_INPUT_HANDLE, input);
            }
        }
        let output = GetStdHandle(STD_OUTPUT_HANDLE);
        let error = GetStdHandle(STD_ERROR_HANDLE);
        if !valid(output) && valid(error) {
            let _ = SetStdHandle(STD_OUTPUT_HANDLE, error);
        } else if !valid(output) && !valid(error) {
            let console = open_console("\\\\.\\CONOUT$");
            if valid(console) {
                let _ = SetStdHandle(STD_OUTPUT_HANDLE, console);
                let _ = SetStdHandle(STD_ERROR_HANDLE, console);
            }
        } else if !valid(error) {
            let _ = SetStdHandle(STD_ERROR_HANDLE, output);
        }
    }
}

fn write_cli_message(message: &str, error: bool) {
    let mut bytes = message.as_bytes().to_vec();
    bytes.extend_from_slice(b"\r\n");
    unsafe {
        let standard_handle = GetStdHandle(if error {
            STD_ERROR_HANDLE
        } else {
            STD_OUTPUT_HANDLE
        });
        if write_cli_bytes(standard_handle, &bytes) {
            return;
        }

        let console_name = to_wide("\\\\.\\CONOUT$");
        let console = CreateFileW(
            console_name.as_ptr(),
            FILE_GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        );
        if !console.is_null() && console as isize != -1 {
            let _ = write_cli_bytes(console, &bytes);
            CloseHandle(console);
        }
    }
}

unsafe fn write_cli_bytes(handle: HANDLE, bytes: &[u8]) -> bool {
    if handle.is_null() || handle as isize == -1 || bytes.is_empty() {
        return false;
    }
    let mut written = 0u32;
    WriteFile(
        handle,
        bytes.as_ptr(),
        bytes.len().min(u32::MAX as usize) as u32,
        &mut written,
        std::ptr::null_mut(),
    ) != 0
        && written == bytes.len().min(u32::MAX as usize) as u32
}

fn ensure_not_elevated() -> Result<(), String> {
    if unsafe { IsUserAnAdmin() } != 0 {
        Err(
            "请不要以管理员身份运行 DSH启动器；它必须使用当前 Windows 用户的 .dsh 配置与插件。"
                .to_owned(),
        )
    } else {
        Ok(())
    }
}

fn ensure_dsh_ready(progress: &dyn Fn(&str)) -> Result<(), String> {
    progress("正在执行运行时完整性检查...");
    let paths = app_paths()?;
    verify_runtime_integrity(&paths).map_err(|error| format!("运行时需要修复：{error}"))?;
    verify_dsh_integrity(&paths).map_err(|error| format!("DSH 需要修复：{error}"))
}

fn ensure_dsh_upgrade_ready(progress: &dyn Fn(&str)) -> Result<(), String> {
    progress("正在验证可升级的 DSH 运行时...");
    let paths = app_paths()?;
    verify_runtime_integrity(&paths).map_err(|error| format!("运行时需要修复：{error}"))?;
    verify_dsh_upgrade_source_integrity(&paths)
        .map_err(|error| format!("当前 DSH 无法安全更新：{error}"))
}

fn runtime_components_present(paths: &AppPaths) -> bool {
    [
        paths.runtime_root.join("node").join("node.exe"),
        paths.runtime_root.join("node").join("npm.cmd"),
    ]
    .iter()
    .all(|path| {
        fs::symlink_metadata(path)
            .map(|metadata| metadata.is_file() && !is_reparse_point(&metadata))
            .unwrap_or(false)
    })
}

fn verify_runtime_integrity(paths: &AppPaths) -> Result<(), String> {
    let manifest = runtime_manifest()?;
    let executable = env::current_exe().map_err(|error| format!("无法定位启动器目录：{error}"))?;
    let executable_dir = executable
        .parent()
        .ok_or_else(|| "无法定位启动器目录".to_owned())?;
    verify_portable_manifest(executable_dir)?;
    if !runtime_components_present(paths) {
        return Err("Node.js 运行时文件缺失；请执行“验证并修复”完成首次安装".to_owned());
    }
    let node_version = executable_version(
        &paths.runtime_root.join("node").join("node.exe"),
        &["--version"],
        "Node.js",
    )?;
    if node_version.trim().trim_start_matches('v') != manifest.node_version {
        return Err(format!(
            "Node.js 版本不一致：清单要求 {}，实际 {}",
            manifest.node_version, node_version
        ));
    }
    let marker = paths.runtime_root.join(RUNTIME_READY_FILE);
    let marker_text =
        fs::read_to_string(&marker).map_err(|error| format!("运行时状态清单缺失：{error}"))?;
    let marker_node = marker_value(&marker_text, "node")
        .ok_or_else(|| "运行时状态清单缺少 node 版本".to_owned())?;
    let marker_dsh = marker_value(&marker_text, "dsh")
        .ok_or_else(|| "运行时状态清单缺少 dsh 版本".to_owned())?;
    let actual_dsh = native_dsh_entry()
        .and_then(|entry| native_dsh_version_for_entry(&entry))
        .map_err(|error| format!("无法验证运行时状态清单中的 DSH：{error}"))?;
    if marker_node != manifest.node_version || marker_dsh != actual_dsh {
        return Err("运行时状态清单与运行时 manifest 不一致".to_owned());
    }
    Ok(())
}

fn verify_dsh_integrity(paths: &AppPaths) -> Result<(), String> {
    let manifest = runtime_manifest()?;
    let entry = required_file(
        paths
            .npm_prefix
            .join("node_modules")
            .join("@deepseek-ai")
            .join("dsh")
            .join(&manifest.dsh_entry),
        "DSH",
    )?;
    let actual = native_dsh_version_for_entry(&entry)?;
    let actual_semver =
        parse_dsh_semver(&actual).ok_or_else(|| format!("DSH 版本号无效：{actual}"))?;
    let minimum = parse_dsh_semver(&manifest.dsh_bootstrap_version)
        .ok_or_else(|| "运行时清单中的 DSH 版本无效".to_owned())?;
    if actual_semver < minimum {
        return Err(format!(
            "DSH 版本低于清单要求：至少 {}，实际 {}",
            manifest.dsh_bootstrap_version, actual
        ));
    }
    verify_quota_plugin_integrity(paths)?;
    Ok(())
}

fn verify_dsh_upgrade_source_integrity(paths: &AppPaths) -> Result<(), String> {
    let manifest = runtime_manifest()?;
    let entry = required_file(
        paths
            .npm_prefix
            .join("node_modules")
            .join("@deepseek-ai")
            .join("dsh")
            .join(&manifest.dsh_entry),
        "DSH",
    )?;
    let actual = native_dsh_version_for_entry(&entry)?;
    parse_dsh_upgrade_source_version(&actual)?;
    verify_quota_plugin_integrity(paths)
}

fn parse_dsh_upgrade_source_version(actual: &str) -> Result<DshVersion, String> {
    parse_dsh_semver(actual).ok_or_else(|| format!("DSH 版本号无效：{actual}"))
}

fn verify_quota_plugin_integrity(paths: &AppPaths) -> Result<(), String> {
    let manifest = runtime_manifest()?;
    let profile = paths.dsh_home.join("profiles").join("web");
    let profile_manifest = required_file(profile.join("package.json"), "DSH Web 配置")?;
    let profile_json: serde_json::Value = serde_json::from_slice(
        &fs::read(&profile_manifest).map_err(|error| format!("无法读取 DSH Web 配置：{error}"))?,
    )
    .map_err(|error| format!("DSH Web 配置格式无效：{error}"))?;
    let bundles = profile_json
        .get("dsh")
        .and_then(|value| value.get("profile"))
        .and_then(|value| value.get("bundles"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "DSH Web 配置缺少 bundles 清单".to_owned())?;
    if !bundles
        .iter()
        .any(|value| value.as_str() == Some(manifest.quota_runtime_name.as_str()))
    {
        return Err(format!(
            "DSH Web 配置未启用 {} 插件",
            manifest.quota_runtime_name
        ));
    }

    let plugin_root = profile
        .join("node_modules")
        .join(&manifest.quota_runtime_name);
    let plugin_manifest = required_file(plugin_root.join("package.json"), "DSH 额度插件")?;
    let plugin_json: serde_json::Value = serde_json::from_slice(
        &fs::read(&plugin_manifest).map_err(|error| format!("无法读取 DSH 额度插件：{error}"))?,
    )
    .map_err(|error| format!("DSH 额度插件清单格式无效：{error}"))?;
    let name = plugin_json
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let version = plugin_json
        .get("version")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if name != manifest.quota_package || version != manifest.quota_version {
        return Err(format!(
            "DSH 额度插件版本不一致：清单要求 {}@{}，实际 {}@{}",
            manifest.quota_package, manifest.quota_version, name, version
        ));
    }
    required_file(plugin_root.join("lib").join("index.js"), "DSH 额度插件入口")?;
    required_file(plugin_root.join("cordis.patch.yml"), "DSH 额度插件 patch")?;
    Ok(())
}

fn executable_version(path: &Path, arguments: &[&str], label: &str) -> Result<String, String> {
    let mut command = hidden_command(path);
    command.args(arguments);
    run_native_command(&mut command, &format!("读取 {label} 版本"))
}

fn marker_value(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let (name, value) = line.split_once('=')?;
        (name.trim() == key).then(|| value.trim().to_owned())
    })
}

fn repair_runtime() -> Result<String, String> {
    repair_runtime_with_progress(&|_| {})
}

fn repair_runtime_with_progress(progress: &dyn Fn(&str)) -> Result<String, String> {
    let paths = app_paths()?;
    let manifest = runtime_manifest()?;
    let runtime_needs_repair = verify_runtime_integrity(&paths).is_err();
    let dsh_needs_repair = verify_dsh_integrity(&paths).is_err();
    if !runtime_needs_repair && !dsh_needs_repair {
        return Ok("运行时验证通过，未做修改".to_owned());
    }
    let was_running = verified_dsh_state(DSH_PORT)?;
    if was_running {
        progress("正在停止服务以安全修复运行时...");
        stop_dsh()?;
    }

    let mut repair_backup = None;
    let mut runtime_backup = None;
    let mut runtime_candidate = None;
    let mut runtime_archives = Vec::new();
    let result = (|| {
        if runtime_needs_repair {
            progress("正在重新验证 Node.js 运行时...");
            let node_archive = ensure_runtime_archive(
                &paths,
                &manifest.node_url,
                &manifest.node_archive_name,
                &manifest.node_sha256,
                "下载 Node.js 运行时",
            )?;
            runtime_archives.push(node_archive.clone());

            progress("正在准备运行时原子提交...");
            let candidate = stage_runtime_candidate(&paths, &node_archive)?;
            runtime_candidate = Some(candidate.clone());
            // DSH preflight needs Node.js. Promote the verified Node candidate
            // before repairing a missing DSH tree, then roll both back together
            // if package preflight or post-commit verification fails.
            progress("正在提交已验证的运行时目录...");
            runtime_backup = Some(promote_runtime_candidate(&paths, &candidate)?);
            runtime_candidate = None;
        }
        if dsh_needs_repair {
            progress("正在暂存并预检 DSH 运行包...");
            repair_backup = Some(repair_dsh_package(progress)?);
        }
        write_runtime_ready_marker(&paths)?;
        verify_runtime_integrity(&paths)?;
        verify_dsh_integrity(&paths)
            .map_err(|error| format!("运行时已修复，但 DSH 仍需修复：{error}"))
    })();

    match result {
        Ok(()) => {
            if was_running {
                if let Err(error) = start_dsh_with_progress(progress) {
                    let rollback_detail =
                        rollback_runtime_and_dsh(&mut runtime_backup, &mut repair_backup);
                    let recovery = match rollback_detail {
                        Ok(()) => match start_dsh_with_progress(progress) {
                            Ok(_) => "；已回滚并恢复原 DSH 服务".to_owned(),
                            Err(restart_error) => {
                                format!("；已回滚，但原 DSH 服务恢复失败：{restart_error}")
                            }
                        },
                        Err(rollback_error) => format!("；回滚失败：{rollback_error}"),
                    };
                    return Err(format!(
                        "运行时已修复，但恢复 DSH 服务失败：{error}{recovery}"
                    ));
                }
            }
            let cleanup = cleanup_runtime_and_dsh_backups(
                &paths,
                runtime_backup.take(),
                repair_backup.take(),
            );
            if let Err(error) = cleanup {
                return Err(format!("运行时验证并修复完成，但清理回滚备份失败：{error}"));
            }
            clear_npm_cache(&paths)
                .map_err(|error| format!("运行时已修复，但删除 npm 缓存失败：{error}"))?;
            for archive in runtime_archives {
                if let Err(error) = fs::remove_file(&archive) {
                    if error.kind() != std::io::ErrorKind::NotFound {
                        return Err(format!("运行时已修复，但清理下载文件失败：{error}"));
                    }
                }
            }
            Ok("运行时验证并修复完成".to_owned())
        }
        Err(error) => {
            let staging_cleanup = runtime_candidate
                .take()
                .map(fs::remove_dir_all)
                .transpose()
                .map_err(|cleanup_error| format!("运行时暂存目录清理失败：{cleanup_error}"));
            let rollback_detail =
                match rollback_runtime_and_dsh(&mut runtime_backup, &mut repair_backup) {
                    Ok(()) => "；已回滚运行时和 DSH".to_owned(),
                    Err(rollback_error) => format!("；运行时或 DSH 回滚失败：{rollback_error}"),
                };
            let staging_detail = match staging_cleanup {
                Ok(_) => String::new(),
                Err(cleanup_error) => format!("；{cleanup_error}"),
            };
            let recovery = if was_running {
                match start_dsh_with_progress(progress) {
                    Ok(_) => "；原 DSH 服务已恢复".to_owned(),
                    Err(restart_error) => format!("；原 DSH 服务恢复失败：{restart_error}"),
                }
            } else {
                String::new()
            };
            Err(format!(
                "运行时修复失败：{error}{rollback_detail}{staging_detail}{recovery}"
            ))
        }
    }
}

fn repair_dsh_package(progress: &dyn Fn(&str)) -> Result<PathBuf, String> {
    let paths = app_paths()?;
    let manifest = runtime_manifest()?;
    let stage = create_upgrade_stage()?;
    let mut created_web_profile = false;
    let prepared = (|| {
        let (entry, version) = stage_dsh(&stage, &manifest.dsh_bootstrap_version)?;
        if version != manifest.dsh_bootstrap_version {
            return Err(format!(
                "修复包版本不一致：清单要求 {}，实际 {version}",
                manifest.dsh_bootstrap_version
            ));
        }
        created_web_profile = ensure_web_profile_for_repair(&paths)?;
        progress("正在预检修复包...");
        preflight_staged_dsh(&entry)?;
        Ok::<(), String>(())
    })();
    if let Err(error) = prepared {
        let cleanup = cleanup_upgrade_stage(&stage).err();
        let profile_cleanup = if created_web_profile {
            remove_created_web_profile(&paths).err()
        } else {
            None
        };
        let mut details = match cleanup {
            Some(cleanup_error) => format!("DSH 修复预检失败：{error}；清理失败：{cleanup_error}"),
            None => format!("DSH 修复预检失败：{error}"),
        };
        if let Some(cleanup_error) = profile_cleanup {
            details.push_str(&format!("；清理新建 DSH Web 配置失败：{cleanup_error}"));
        }
        return Err(details);
    }

    let backup = promote_dsh_candidate(&stage)?;
    let promoted = (|| {
        verify_dsh_integrity(&app_paths()?)?;
        write_runtime_ready_marker(&app_paths()?)?;
        Ok::<(), String>(())
    })();
    if let Err(error) = promoted {
        let rollback = rollback_dsh_candidate(&backup);
        let profile_cleanup = if created_web_profile {
            remove_created_web_profile(&paths).err()
        } else {
            None
        };
        let mut details = match rollback {
            Ok(()) => format!("DSH 修复提交后验证失败：{error}；已恢复旧版本"),
            Err(rollback_error) => {
                format!("DSH 修复提交后验证失败：{error}；旧版本恢复失败：{rollback_error}")
            }
        };
        if let Some(cleanup_error) = profile_cleanup {
            details.push_str(&format!("；清理新建 DSH Web 配置失败：{cleanup_error}"));
        }
        return Err(details);
    }
    Ok(backup)
}

fn ensure_web_profile_for_repair(paths: &AppPaths) -> Result<bool, String> {
    let profile = paths.dsh_home.join("profiles").join(DSH_WEB_PROFILE_NAME);
    let manifest_path = profile.join("package.json");
    match fs::symlink_metadata(&manifest_path) {
        Ok(metadata) if is_reparse_point(&metadata) || !metadata.is_file() => {
            return Err(format!(
                "DSH Web 配置不是普通文件，拒绝覆盖：{}",
                manifest_path.display()
            ));
        }
        Ok(_) => {
            validate_web_profile_for_repair(&profile)?;
            return Ok(false);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "无法检查 DSH Web 配置 {}：{error}",
                manifest_path.display()
            ));
        }
    }

    if directory_has_entries(&profile) {
        return Err(format!(
            "DSH Web 配置目录缺少 package.json，拒绝覆盖：{}",
            profile.display()
        ));
    }
    ensure_safe_directory(&profile)
        .map_err(|error| format!("无法创建 DSH Web 配置目录：{error}"))?;
    let result = (|| {
        let manifest = serde_json::json!({
            "name": "dsh-profile-web",
            "private": true,
            "dependencies": {
                DSH_QUOTA_RUNTIME_NAME: format!(
                    "npm:{}@{}",
                    runtime_manifest()?.quota_package,
                    runtime_manifest()?.quota_version
                )
            },
            "dsh": {
                "profile": {
                    "bundles": [
                        "@deepseek-ai/dsh-base",
                        "@deepseek-ai/dsh-web-app",
                        DSH_QUOTA_RUNTIME_NAME
                    ]
                }
            }
        });
        write_safe_text_file(
            &manifest_path,
            &format!(
                "{}\n",
                serde_json::to_string_pretty(&manifest)
                    .map_err(|error| format!("无法生成 DSH Web 配置：{error}"))?
            ),
            "DSH Web 配置",
        )?;
        write_safe_text_file(
            &profile.join("cordis.patch.yml"),
            "[]\n",
            "DSH Web 配置补丁",
        )?;
        write_safe_text_file(
            &profile.join("pnpm-workspace.yaml"),
            "packages:\n  - .\n\nnodeLinker: hoisted\nautoInstallPeers: false\n",
            "DSH Web 工作区配置",
        )?;

        let profile_text = profile.to_string_lossy().into_owned();
        let manifest = runtime_manifest()?;
        let mut install = native_npm_command()?;
        install
            .env_remove("NPM_CONFIG_PREFIX")
            .current_dir(&profile)
            .args([
                "install",
                "--prefix",
                &profile_text,
                "--package-lock=true",
                "--ignore-scripts",
                "--no-audit",
                "--no-fund",
                "--fetch-timeout=30000",
                "--fetch-retries=0",
                &format!("--registry={}", npm_registry_base_url()?),
                &format!("{}@{}", manifest.quota_package, manifest.quota_version),
            ]);
        run_native_update_command_with_timeout(
            &mut install,
            "安装 DSH 额度插件",
            DSH_PACKAGE_COMMAND_TIMEOUT,
        )?;
        validate_web_profile_for_repair(&profile)
    })();
    if let Err(error) = result {
        let cleanup = fs::remove_dir_all(&profile).err();
        return Err(match cleanup {
            Some(cleanup_error) => {
                format!("初始化 DSH Web 配置失败：{error}；清理失败：{cleanup_error}")
            }
            None => format!("初始化 DSH Web 配置失败：{error}"),
        });
    }
    Ok(true)
}

fn validate_web_profile_for_repair(profile: &Path) -> Result<(), String> {
    let manifest_path = required_file(profile.join("package.json"), "DSH Web 配置")?;
    let manifest: serde_json::Value = serde_json::from_slice(
        &fs::read(&manifest_path).map_err(|error| format!("无法读取 DSH Web 配置：{error}"))?,
    )
    .map_err(|error| format!("DSH Web 配置格式无效：{error}"))?;
    let bundles = manifest
        .get("dsh")
        .and_then(|value| value.get("profile"))
        .and_then(|value| value.get("bundles"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "DSH Web 配置缺少 bundles 清单".to_owned())?;
    if !bundles
        .iter()
        .any(|value| value.as_str() == Some(DSH_QUOTA_RUNTIME_NAME))
    {
        return Err(format!(
            "DSH Web 配置未启用 {} 插件",
            DSH_QUOTA_RUNTIME_NAME
        ));
    }
    let runtime_manifest = runtime_manifest()?;
    let plugin_root = profile.join("node_modules").join(DSH_QUOTA_RUNTIME_NAME);
    let plugin_manifest = required_file(plugin_root.join("package.json"), "DSH 额度插件")?;
    let plugin: serde_json::Value = serde_json::from_slice(
        &fs::read(&plugin_manifest).map_err(|error| format!("无法读取 DSH 额度插件：{error}"))?,
    )
    .map_err(|error| format!("DSH 额度插件清单格式无效：{error}"))?;
    let name = plugin
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let version = plugin
        .get("version")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if name != runtime_manifest.quota_package || version != runtime_manifest.quota_version {
        return Err(format!(
            "DSH 额度插件版本不一致：清单要求 {}@{}，实际 {}@{}",
            runtime_manifest.quota_package, runtime_manifest.quota_version, name, version
        ));
    }
    required_file(plugin_root.join("lib").join("index.js"), "DSH 额度插件入口")?;
    required_file(plugin_root.join("cordis.patch.yml"), "DSH 额度插件 patch")?;
    Ok(())
}

fn remove_created_web_profile(paths: &AppPaths) -> Result<(), String> {
    let profile = paths.dsh_home.join("profiles").join(DSH_WEB_PROFILE_NAME);
    match fs::symlink_metadata(&profile) {
        Ok(metadata) if is_reparse_point(&metadata) || !metadata.is_dir() => Err(format!(
            "拒绝清理无效 DSH Web 配置目录：{}",
            profile.display()
        )),
        Ok(_) => fs::remove_dir_all(&profile)
            .map_err(|error| format!("无法清理新建 DSH Web 配置目录：{error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法检查 DSH Web 配置目录：{error}")),
    }
}

fn write_safe_text_file(path: &Path, contents: &str, label: &str) -> Result<(), String> {
    ensure_safe_file_destination(path, label)?;
    fs::write(path, contents).map_err(|error| format!("无法写入{label}：{error}"))
}

fn write_runtime_ready_marker(paths: &AppPaths) -> Result<(), String> {
    let manifest = runtime_manifest()?;
    let entry = native_dsh_entry()?;
    let dsh_version = native_dsh_version_for_entry(&entry)?;
    let marker = paths.runtime_root.join(RUNTIME_READY_FILE);
    let contents = format!("node={}\ndsh={}\n", manifest.node_version, dsh_version);
    ensure_safe_file_destination(&marker, "运行时状态")?;
    fs::write(marker, contents).map_err(|error| format!("无法写入运行时状态：{error}"))
}

fn ensure_runtime_archive(
    paths: &AppPaths,
    url: &str,
    archive_name: &str,
    expected_sha256: &str,
    description: &str,
) -> Result<PathBuf, String> {
    let destination = paths.update_root.join(archive_name);
    ensure_safe_file_destination(&destination, description)?;
    if destination.is_file() {
        let actual = calculate_sha256(&destination)
            .map_err(|error| format!("无法验证已有{description}：{error}"))?;
        if actual.eq_ignore_ascii_case(expected_sha256) {
            return Ok(destination);
        }
        fs::remove_file(&destination)
            .map_err(|error| format!("无法移除损坏的{description}：{error}"))?;
    }

    let partial = paths.update_root.join(format!("{archive_name}.download"));
    ensure_safe_file_destination(&partial, "运行时下载暂存文件")?;
    if partial.exists() {
        fs::remove_file(&partial)
            .map_err(|error| format!("无法清理未完成的{description}：{error}"))?;
    }
    download_runtime_file(url, &partial, description)?;
    let actual =
        calculate_sha256(&partial).map_err(|error| format!("无法验证{description}：{error}"))?;
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        fs::remove_file(&partial)
            .map_err(|error| format!("{description}校验失败后清理暂存文件失败：{error}"))?;
        return Err(format!(
            "{description}校验失败：期望 {expected_sha256}，实际 {actual}"
        ));
    }
    fs::rename(&partial, &destination)
        .map_err(|error| format!("无法保存{description}：{error}"))?;
    Ok(destination)
}

fn download_runtime_file(url: &str, destination: &Path, description: &str) -> Result<(), String> {
    let manifest = runtime_manifest()?;
    if url != manifest.node_url {
        return Err("拒绝下载未固定来源的运行时文件".to_owned());
    }
    ensure_safe_file_destination(destination, "运行时下载文件")?;
    let timeout = RUNTIME_DOWNLOAD_TIMEOUT.as_secs();
    let script = format!(
        "$ErrorActionPreference = 'Stop';\n\
         $ProgressPreference = 'SilentlyContinue';\n\
         Invoke-WebRequest -UseBasicParsing -Headers @{{ 'User-Agent' = {user_agent} }} -Uri {url} -OutFile {destination} -TimeoutSec {timeout} | Out-Null",
        user_agent = powershell_literal(&format!("DSH-Launcher/{APP_VERSION}")),
        url = powershell_literal(url),
        destination = powershell_literal(&destination.to_string_lossy()),
    );
    run_powershell_script_with_timeout(&script, description, RUNTIME_DOWNLOAD_TIMEOUT)?;
    let metadata = fs::metadata(destination)
        .map_err(|error| format!("{description}后无法读取文件：{error}"))?;
    if metadata.len() == 0 {
        return Err(format!("{description}得到空文件"));
    }
    Ok(())
}

fn stage_runtime_candidate(paths: &AppPaths, node_archive: &Path) -> Result<PathBuf, String> {
    let candidate = paths
        .update_root
        .join(format!("runtime-candidate-{}", transaction_nonce()));
    ensure_safe_directory(&candidate)
        .map_err(|error| format!("无法创建运行时提交暂存目录：{error}"))?;
    let result = (|| {
        stage_runtime_component(
            paths,
            node_archive,
            &candidate,
            "node",
            &["node.exe", "npm.cmd"],
        )?;
        Ok::<(), String>(())
    })();
    if let Err(error) = result {
        let cleanup = fs::remove_dir_all(&candidate).err();
        return Err(match cleanup {
            Some(cleanup_error) => {
                format!("运行时暂存失败：{error}；暂存目录清理失败：{cleanup_error}")
            }
            None => format!("运行时暂存失败：{error}"),
        });
    }
    Ok(candidate)
}

fn stage_runtime_component(
    paths: &AppPaths,
    archive: &Path,
    candidate: &Path,
    component: &str,
    required_files: &[&str],
) -> Result<(), String> {
    let extract_root = paths.temp_root.join(format!(
        "runtime-extract-{component}-{}-{}",
        std::process::id(),
        transaction_nonce()
    ));
    let result = (|| {
        ensure_safe_directory(&extract_root)
            .map_err(|error| format!("无法创建运行时解压目录：{error}"))?;
        expand_runtime_archive(archive, &extract_root)?;
        let source = find_archive_root(&extract_root, required_files[0])?;
        let staging = candidate.join(component);
        ensure_safe_directory(&staging)
            .map_err(|error| format!("无法创建运行时安装暂存目录：{error}"))?;
        copy_directory_contents(&source, &staging)?;
        for file in required_files {
            let path = staging.join(file);
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("运行时压缩包缺少 {component} 文件 {file}：{error}"))?;
            if is_reparse_point(&metadata) || !metadata.is_file() {
                return Err(format!(
                    "运行时压缩包中的 {component} 文件无效：{}",
                    path.display()
                ));
            }
        }
        Ok::<(), String>(())
    })();
    let cleanup = fs::remove_dir_all(&extract_root);
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Err(error), Err(cleanup_error)) => {
            Err(format!("{error}；解压目录清理失败：{cleanup_error}"))
        }
        (Ok(()), Err(error)) => Err(format!("运行时解压目录清理失败：{error}")),
    }
}

fn expand_runtime_archive(archive: &Path, destination: &Path) -> Result<(), String> {
    let script = format!(
        "$ErrorActionPreference = 'Stop';\n\
         Add-Type -AssemblyName System.IO.Compression.FileSystem;\n\
         $archivePath = {archive};\n\
         $destinationPath = {destination};\n\
         $root = [System.IO.Path]::GetFullPath($destinationPath);\n\
         if (-not $root.EndsWith([System.IO.Path]::DirectorySeparatorChar)) {{ $root += [System.IO.Path]::DirectorySeparatorChar }};\n\
         $zip = [System.IO.Compression.ZipFile]::OpenRead($archivePath);\n\
         try {{\n\
           foreach ($entry in $zip.Entries) {{\n\
             $name = $entry.FullName.Replace('/', '\\');\n\
             if ([String]::IsNullOrWhiteSpace($name) -or $name.StartsWith('\\\\') -or $name.Contains(':')) {{ throw \"运行时压缩包包含无效路径：$($entry.FullName)\" }};\n\
             $target = [System.IO.Path]::GetFullPath([System.IO.Path]::Combine($root, $name));\n\
             if (-not $target.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) {{ throw \"运行时压缩包路径越界：$($entry.FullName)\" }};\n\
             $attributes = [uint32]$entry.ExternalAttributes;\n\
             if ((($attributes -shr 16) -band 0xF000) -eq 0xA000) {{ throw \"运行时压缩包包含符号链接：$($entry.FullName)\" }};\n\
           }}\n\
         }} finally {{ $zip.Dispose() }};\n\
         $tar = Join-Path $env:WINDIR 'System32\\tar.exe';\n\
         if (-not (Test-Path -LiteralPath $tar -PathType Leaf)) {{ throw '系统缺少 tar.exe，无法安全解压运行时文件' }};\n\
         & $tar -xf $archivePath -C $destinationPath;\n\
         if ($LASTEXITCODE -ne 0) {{ throw \"tar 解压运行时文件失败，退出码：$LASTEXITCODE\" }}",
        archive = powershell_literal(&archive.to_string_lossy()),
        destination = powershell_literal(&destination.to_string_lossy()),
    );
    run_powershell_script(&script, "解压运行时文件")?;
    Ok(())
}

fn find_archive_root(extract_root: &Path, required_file: &str) -> Result<PathBuf, String> {
    let direct = extract_root.join(required_file);
    if let Ok(metadata) = fs::symlink_metadata(&direct) {
        if is_reparse_point(&metadata) {
            return Err(format!("运行时压缩包包含符号链接：{}", direct.display()));
        }
    }
    if direct.is_file() {
        return Ok(extract_root.to_owned());
    }
    let mut matches = Vec::new();
    let entries =
        fs::read_dir(extract_root).map_err(|error| format!("无法读取运行时解压目录：{error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("无法读取运行时解压目录项：{error}"))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("无法读取运行时目录项类型：{error}"))?;
        if entry
            .file_type()
            .map_err(|error| format!("无法读取运行时目录项类型：{error}"))?
            .is_dir()
            && !is_reparse_point(&metadata)
            && path.join(required_file).is_file()
        {
            let required = path.join(required_file);
            if let Ok(metadata) = fs::symlink_metadata(&required) {
                if is_reparse_point(&metadata) {
                    return Err(format!("运行时压缩包包含符号链接：{}", required.display()));
                }
            }
            matches.push(path);
        }
    }
    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(format!("运行时压缩包中未找到 {required_file}")),
        _ => Err(format!(
            "运行时压缩包包含多个候选目录，无法安全安装 {required_file}"
        )),
    }
}

fn promote_runtime_candidate(paths: &AppPaths, candidate: &Path) -> Result<PathBuf, String> {
    if candidate.parent() != Some(paths.update_root.as_path())
        || !candidate.is_dir()
        || fs::symlink_metadata(candidate)
            .ok()
            .as_ref()
            .is_some_and(is_reparse_point)
    {
        return Err("拒绝提交非启动器创建的运行时暂存目录".to_owned());
    }
    let target = &paths.runtime_root;
    let target_metadata =
        fs::symlink_metadata(target).map_err(|error| format!("无法检查当前运行时目录：{error}"))?;
    if is_reparse_point(&target_metadata) || !target_metadata.is_dir() {
        return Err(format!("拒绝覆盖无效运行时目录：{}", target.display()));
    }
    let backup = paths
        .update_root
        .join(format!("runtime-rollback-{}", transaction_nonce()));
    fs::rename(target, &backup).map_err(|error| format!("无法暂存旧运行时目录：{error}"))?;
    if let Err(error) = fs::rename(candidate, target) {
        let restore = fs::rename(&backup, target);
        return match restore {
            Ok(()) => Err(format!("无法提交运行时暂存目录：{error}")),
            Err(restore_error) => Err(format!(
                "无法提交运行时暂存目录：{error}；旧运行时恢复失败：{restore_error}"
            )),
        };
    }
    Ok(backup)
}

fn rollback_runtime_candidate(backup: &Path) -> Result<(), String> {
    let paths = app_paths()?;
    if backup.parent() != Some(paths.update_root.as_path())
        || !backup.is_dir()
        || fs::symlink_metadata(backup)
            .ok()
            .as_ref()
            .is_some_and(is_reparse_point)
    {
        return Err("运行时回滚目录无效".to_owned());
    }
    let target = &paths.runtime_root;
    let target_metadata = fs::symlink_metadata(target)
        .map_err(|error| format!("无法检查待回滚运行时目录：{error}"))?;
    if is_reparse_point(&target_metadata) || !target_metadata.is_dir() {
        return Err(format!("拒绝删除无效运行时目录：{}", target.display()));
    }
    let failed = paths
        .update_root
        .join(format!("runtime-failed-{}", transaction_nonce()));
    fs::rename(target, &failed).map_err(|error| format!("无法暂存失败的运行时目录：{error}"))?;
    if let Err(error) = fs::rename(backup, target) {
        let restore = fs::rename(&failed, target);
        return match restore {
            Ok(()) => Err(format!("无法恢复旧运行时目录：{error}")),
            Err(restore_error) => Err(format!(
                "无法恢复旧运行时目录：{error}；失败运行时恢复失败：{restore_error}"
            )),
        };
    }
    fs::remove_dir_all(&failed)
        .map_err(|error| format!("运行时已恢复，但失败目录清理失败：{error}"))?;
    Ok(())
}

fn rollback_runtime_and_dsh(
    runtime_backup: &mut Option<PathBuf>,
    dsh_backup: &mut Option<PathBuf>,
) -> Result<(), String> {
    ensure_service_stopped_for_rollback()?;
    let mut failures = Vec::new();
    if let Some(backup) = runtime_backup.take() {
        if let Err(error) = rollback_runtime_candidate(&backup) {
            failures.push(format!("运行时：{error}"));
        }
    }
    if let Some(backup) = dsh_backup.take() {
        if let Err(error) = rollback_dsh_candidate(&backup) {
            failures.push(format!("DSH：{error}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("；"))
    }
}

fn ensure_service_stopped_for_rollback() -> Result<(), String> {
    if find_verified_dsh_pid(DSH_PORT)?.is_some() {
        stop_dsh()?;
    }
    if is_port_listening(DSH_PORT) {
        return Err(format!(
            "端口 {DSH_PORT} 仍被占用，拒绝在服务未停止时执行离线回滚"
        ));
    }
    Ok(())
}

fn cleanup_runtime_and_dsh_backups(
    paths: &AppPaths,
    runtime_backup: Option<PathBuf>,
    dsh_backup: Option<PathBuf>,
) -> Result<(), String> {
    let mut failures = Vec::new();
    if let Some(backup) = runtime_backup {
        match fs::symlink_metadata(&backup) {
            Ok(metadata) if is_reparse_point(&metadata) || !metadata.is_dir() => {
                failures.push(format!("运行时回滚目录无效：{}", backup.display()));
            }
            Ok(_) => {
                if let Err(error) = fs::remove_dir_all(&backup) {
                    failures.push(format!("运行时回滚目录：{error}"));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => failures.push(format!("运行时回滚目录：{error}")),
        }
    }
    if let Some(backup) = dsh_backup {
        if let Err(error) = cleanup_old_dsh_rollbacks(paths, &backup) {
            failures.push(format!("DSH 回滚目录：{error}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("；"))
    }
}

fn start_dsh() -> Result<String, String> {
    start_dsh_with_progress(&|_| {})
}

fn start_dsh_with_progress(progress: &dyn Fn(&str)) -> Result<String, String> {
    if is_native_dsh_running()? {
        return Ok("服务进程已在运行 · http://127.0.0.1:3080".to_owned());
    }
    if find_verified_dsh_pid(DSH_PORT)?.is_some() {
        return Ok("服务已在运行 · http://127.0.0.1:3080".to_owned());
    }

    if is_port_listening(DSH_PORT) {
        return Err("端口 3080 正被其他服务占用。请先关闭该服务后再启动。".to_owned());
    }

    ensure_dsh_ready(progress)?;

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
            let termination = terminate_native_dsh_process(pid);
            let detail = termination
                .err()
                .map(|termination_error| format!("；启动进程清理失败：{termination_error}"))
                .unwrap_or_default();
            return Err(format!("{error}{detail}"));
        }
    };

    let deadline = Instant::now() + DSH_START_TIMEOUT;
    while Instant::now() < deadline {
        if CANCEL_REQUESTED.load(Ordering::Acquire) {
            let termination = terminate_native_dsh_process(pid);
            let clear = if termination.is_ok() {
                clear_native_dsh_pid().err()
            } else {
                None
            };
            let detail = termination
                .err()
                .map(|error| format!("；关闭启动进程失败：{error}"))
                .or_else(|| clear.map(|error| format!("；清理服务进程记录失败：{error}")))
                .unwrap_or_default();
            return Err(format!("启动服务已取消{detail}"));
        }
        if native_dsh_process_matches(process)? && is_http_success(DSH_PORT, "/") {
            return Ok("服务已启动 · http://127.0.0.1:3080".to_owned());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("读取服务启动状态失败：{error}"))?
        {
            let cleanup_detail = clear_native_dsh_pid()
                .err()
                .map(|error| format!("；清理服务进程记录失败：{error}"))
                .unwrap_or_default();
            return Err(format!(
                "服务启动进程提前退出：{status}{}{cleanup_detail}",
                dsh_start_log_diagnostic()
            ));
        }
        thread::sleep(Duration::from_millis(500));
    }

    let termination = terminate_native_dsh_process(pid);
    let clear = if termination.is_ok() {
        clear_native_dsh_pid().err()
    } else {
        None
    };
    let cleanup_detail = termination
        .err()
        .map(|error| format!("；关闭启动进程失败：{error}"))
        .or_else(|| clear.map(|error| format!("；清理服务进程记录失败：{error}")))
        .unwrap_or_default();
    Err(format!(
        "服务在 {} 秒内未启动{}{}",
        DSH_START_TIMEOUT.as_secs(),
        dsh_start_log_diagnostic(),
        cleanup_detail
    ))
}

fn restart_dsh() -> Result<String, String> {
    restart_dsh_with_progress(&|_| {})
}

fn restart_dsh_with_progress(progress: &dyn Fn(&str)) -> Result<String, String> {
    if find_verified_dsh_pid(DSH_PORT)?.is_some() {
        progress("正在停止服务...");
        stop_dsh()?;
    }
    start_dsh_with_progress(progress)
}

fn stop_dsh() -> Result<String, String> {
    let verified_pid = find_verified_dsh_pid(DSH_PORT)?;
    let Some(pid) = verified_pid else {
        return if is_port_listening(DSH_PORT) {
            Err("端口 3080 上的进程无法验证为 DSH，已拒绝关闭。".to_owned())
        } else {
            clear_native_dsh_pid()?;
            Ok("服务当前未运行".to_owned())
        };
    };

    let forced = terminate_native_dsh_process(pid)?;
    let deadline = Instant::now() + DSH_STOP_TIMEOUT;
    while Instant::now() < deadline {
        if CANCEL_REQUESTED.load(Ordering::Acquire) {
            return Err("停止服务已取消；服务可能仍在退出，请稍后查看状态".to_owned());
        }
        let process_running = is_process_running(pid)?;
        if !process_running && !is_port_listening(DSH_PORT) {
            clear_native_dsh_pid()?;
            return Ok(if forced {
                "服务已停止（已强制终止）".to_owned()
            } else {
                "服务已停止".to_owned()
            });
        }
        // A listener can outlive the process briefly while Windows releases the
        // socket. Keep waiting for that normal shutdown race; only report a
        // conflict after the bounded timeout below.
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
    progress("正在准备 DSH 运行时...");
    ensure_dsh_upgrade_ready(progress)?;
    progress("正在检查当前版本...");
    let before = native_dsh_version()?;
    progress("正在查询最新版本...");
    let latest = latest_dsh_version()?;
    let before_version =
        parse_dsh_semver(&before).ok_or_else(|| format!("当前 DSH 版本号无效：{before}"))?;
    let latest_version =
        parse_dsh_semver(&latest).ok_or_else(|| format!("远端 DSH 版本号无效：{latest}"))?;
    match latest_version.cmp(&before_version) {
        std::cmp::Ordering::Equal => return Ok(format!("已是最新版本 {before}")),
        std::cmp::Ordering::Less => {
            return Ok(format!("远端版本 {latest} 低于当前版本 {before}，不会降级"));
        }
        std::cmp::Ordering::Greater => {}
    }

    let stage = create_upgrade_stage()?;
    let preflight: Result<String, String> = (|| {
        progress("正在下载更新...");
        let (entry, candidate) = stage_dsh(&stage, &latest)?;
        if candidate != latest {
            return Err(format!(
                "检测到版本已变化（{latest} → {candidate}），请重新检查更新"
            ));
        }
        progress("正在验证更新...");
        preflight_staged_dsh(&entry)?;
        Ok(candidate)
    })();

    let candidate = match preflight {
        Ok(candidate) => candidate,
        Err(error) => {
            let cleanup_detail = cleanup_upgrade_stage(&stage)
                .err()
                .map(|cleanup_error| format!("；暂存目录清理失败：{cleanup_error}"))
                .unwrap_or_default();
            return Err(format!(
                "更新验证失败，当前服务未被停止或替换：{error}{cleanup_detail}"
            ));
        }
    };

    let was_running = verified_dsh_state(DSH_PORT)?;
    if was_running {
        progress("正在停止服务...");
        if let Err(error) = stop_dsh() {
            let cleanup_detail = cleanup_upgrade_stage(&stage)
                .err()
                .map(|cleanup_error| format!("；暂存目录清理失败：{cleanup_error}"))
                .unwrap_or_default();
            return Err(format!(
                "无法停止当前服务，更新未提交：{error}{cleanup_detail}"
            ));
        }
    }

    progress("正在提交已验证的更新目录...");
    let backup = match promote_dsh_candidate(&stage) {
        Ok(backup) => backup,
        Err(error) => {
            let cleanup_detail = cleanup_upgrade_stage(&stage)
                .err()
                .map(|cleanup_error| format!("；暂存目录清理失败：{cleanup_error}"))
                .unwrap_or_default();
            let restart_detail = if was_running {
                start_dsh()
                    .err()
                    .map(|restart_error| format!("；原 DSH 服务恢复失败：{restart_error}"))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            return Err(format!(
                "无法提交已验证的 DSH 更新：{error}{cleanup_detail}{restart_detail}"
            ));
        }
    };
    let upgrade = (|| {
        #[cfg(feature = "test-hooks")]
        if env::var_os("DSH_LAUNCHER_TEST_FAIL_AFTER_DSH_PROMOTE").is_some() {
            return Err("故障注入：已提交候选目录，强制验证回滚".to_owned());
        }
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
        write_runtime_ready_marker(&app_paths()?)?;
        Ok(after)
    })();

    match upgrade {
        Ok(after) => {
            let paths = app_paths()?;
            let cache_cleanup = clear_npm_cache(&paths);
            let rollback_cleanup = cleanup_old_dsh_rollbacks(&paths, &backup);
            if let Err(error) = rollback_cleanup {
                return Err(format!(
                    "更新完成：{before} → {after}，但清理旧回滚版本失败：{error}"
                ));
            }
            if let Err(error) = cache_cleanup {
                return Err(format!(
                    "更新完成：{before} → {after}，但删除 npm 缓存失败：{error}"
                ));
            }
            Ok(format!("更新完成：{before} → {after}"))
        }
        Err(error) => {
            progress("正在恢复原版本...");
            let rollback = ensure_service_stopped_for_rollback()
                .and_then(|()| rollback_dsh_candidate(&backup));
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
    let manifest = runtime_manifest()?;
    required_file(
        native_npm_prefix()?
            .join("node_modules")
            .join("@deepseek-ai")
            .join("dsh")
            .join(&manifest.dsh_entry),
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
    #[cfg(feature = "test-hooks")]
    if let Some(version) = env::var_os("DSH_LAUNCHER_TEST_LATEST_DSH_VERSION") {
        let version = version.to_string_lossy().into_owned();
        if parse_dsh_semver(&version).is_none() {
            return Err(format!("故障注入的 DSH 版本无效：{version}"));
        }
        return Ok(version);
    }
    latest_dsh_version_from_registry().or_else(|_| latest_dsh_version_from_npm())
}

fn latest_dsh_version_from_registry() -> Result<String, String> {
    let registry_url = runtime_manifest()?.dsh_registry_url;
    let mut command = hidden_command(native_node_executable()?);
    configure_native_environment(&mut command)?;
    command.args(["-e", DSH_LATEST_VERSION_SCRIPT, &registry_url]);
    let output = run_native_command(&mut command, "查询最新版本")?;
    parse_latest_dsh_version(&output).ok_or_else(|| "最新版本查询未返回有效版本号".to_owned())
}

fn latest_dsh_version_from_npm() -> Result<String, String> {
    let package_name = runtime_manifest()?.dsh_package;
    let mut command = native_npm_command()?;
    command.args([
        "view",
        &package_name,
        "versions",
        "--json",
        "--fetch-timeout=10000",
        "--fetch-retries=0",
        "--loglevel=error",
        &format!("--registry={}", npm_registry_base_url()?),
    ]);
    let output = run_native_command(&mut command, "查询最新版本")?;
    parse_latest_dsh_version(&output).ok_or_else(|| "最新版本查询未返回有效版本号".to_owned())
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

fn parse_latest_dsh_version(output: &str) -> Option<String> {
    let versions = serde_json::from_str::<Vec<String>>(output.trim()).unwrap_or_else(|_| {
        output
            .lines()
            .map(str::trim)
            .filter(|line| is_safe_dsh_version(line))
            .map(str::to_owned)
            .collect()
    });
    versions
        .into_iter()
        .filter_map(|version| parse_dsh_semver(&version).map(|parsed| (parsed, version)))
        .max_by(|(left, _), (right, _)| left.cmp(right))
        .map(|(_, version)| version)
}

fn parse_dsh_semver(value: &str) -> Option<DshVersion> {
    let value = value.trim();
    if value.is_empty() || value.starts_with('v') {
        return None;
    }
    let (without_build, build) = value.split_once('+').unwrap_or((value, ""));
    if !build.is_empty()
        && !build.split('.').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
    {
        return None;
    }
    let (core, prerelease_text) = without_build
        .split_once('-')
        .map_or((without_build, ""), |(core, pre)| (core, pre));
    let core_parts: Vec<&str> = core.split('.').collect();
    if core_parts.len() != 3 {
        return None;
    }
    let mut numeric = [0u64; 3];
    for (index, part) in core_parts.iter().enumerate() {
        if part.is_empty() || (part.len() > 1 && part.starts_with('0')) {
            return None;
        }
        numeric[index] = part.parse().ok()?;
    }
    let prerelease = if prerelease_text.is_empty() {
        Vec::new()
    } else {
        prerelease_text
            .split('.')
            .map(|part| {
                if part.is_empty()
                    || !part
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric() || character == '-')
                    || (part.len() > 1
                        && part.starts_with('0')
                        && part.chars().all(|character| character.is_ascii_digit()))
                {
                    return None;
                }
                if part.chars().all(|character| character.is_ascii_digit()) {
                    Some(SemverIdentifier::Numeric(part.parse().ok()?))
                } else {
                    Some(SemverIdentifier::Text(part.to_owned()))
                }
            })
            .collect::<Option<Vec<_>>>()?
    };
    Some(DshVersion {
        major: numeric[0],
        minor: numeric[1],
        patch: numeric[2],
        prerelease,
    })
}

fn compare_semver_identifiers(
    left: &[SemverIdentifier],
    right: &[SemverIdentifier],
) -> std::cmp::Ordering {
    for (left, right) in left.iter().zip(right.iter()) {
        let ordering = match (left, right) {
            (SemverIdentifier::Numeric(left), SemverIdentifier::Numeric(right)) => left.cmp(right),
            (SemverIdentifier::Numeric(_), SemverIdentifier::Text(_)) => std::cmp::Ordering::Less,
            (SemverIdentifier::Text(_), SemverIdentifier::Numeric(_)) => {
                std::cmp::Ordering::Greater
            }
            (SemverIdentifier::Text(left), SemverIdentifier::Text(right)) => left.cmp(right),
        };
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
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
    let root = app_paths()?.update_root.join("dsh-staging");
    ensure_safe_directory(&root).map_err(|error| format!("无法创建 DSH 升级暂存目录：{error}"))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("无法生成 DSH 升级暂存目录名称：{error}"))?
        .as_millis();
    let stage = root.join(format!("dsh-{nonce}-{}", std::process::id()));
    ensure_safe_directory(&stage).map_err(|error| format!("无法创建 DSH 升级暂存目录：{error}"))?;
    Ok(stage)
}

fn npm_registry_base_url() -> Result<String, String> {
    let registry_url = runtime_manifest()?.dsh_registry_url;
    let (scheme, remainder) = registry_url
        .split_once("://")
        .ok_or_else(|| "运行时清单中的 npm registry URL 无效".to_owned())?;
    let authority = remainder
        .split('/')
        .next()
        .filter(|authority| !authority.is_empty())
        .ok_or_else(|| "运行时清单中的 npm registry 主机无效".to_owned())?;
    Ok(format!("{scheme}://{authority}/"))
}

fn cleanup_upgrade_stage(stage: &Path) -> Result<(), String> {
    let root = app_paths()?.update_root.join("dsh-staging");
    if stage.parent() != Some(root.as_path()) {
        return Err("拒绝清理未由启动器创建的 DSH 升级暂存目录".to_owned());
    }
    fs::remove_dir_all(stage).map_err(|error| format!("无法清理 DSH 升级暂存目录：{error}"))
}

fn stage_dsh(stage: &Path, version: &str) -> Result<(PathBuf, String), String> {
    let stage_text = stage.to_string_lossy().into_owned();
    let manifest = runtime_manifest()?;
    let package_name = manifest.dsh_package.clone();
    if parse_dsh_semver(version).is_none() {
        return Err(format!("拒绝暂存无效的 DSH 版本：{version}"));
    }
    let package = format!("{package_name}@{version}");

    let mut install = native_npm_command()?;
    install
        .env_remove("NPM_CONFIG_PREFIX")
        .env("NPM_CONFIG_CACHE", stage.join("npm-cache"))
        .current_dir(stage)
        .args([
            "install",
            "--prefix",
            &stage_text,
            "--package-lock=true",
            "--ignore-scripts",
            "--legacy-peer-deps",
            "--no-audit",
            "--no-fund",
            "--fetch-timeout=30000",
            "--fetch-retries=0",
            &format!("--registry={}", npm_registry_base_url()?),
            &package,
        ])
        .args(manifest.dsh_peer_dependencies.iter().map(String::as_str));
    run_native_update_command(&mut install, "下载更新")?;

    let mut rebuild = native_npm_command()?;
    rebuild
        .env_remove("NPM_CONFIG_PREFIX")
        .env("NPM_CONFIG_CACHE", stage.join("npm-cache"))
        .current_dir(stage)
        .args([
            "rebuild",
            "--prefix",
            &stage_text,
            "--ignore-scripts=false",
            "--no-audit",
            "--no-fund",
        ])
        .args(DSH_BUILD_SCRIPT_PACKAGES);
    run_native_update_command(&mut rebuild, "准备更新")?;
    run_npm_audit(stage)?;
    let stage_cache = stage.join("npm-cache");
    if stage_cache.exists() {
        fs::remove_dir_all(&stage_cache)
            .map_err(|error| format!("无法清理 DSH 暂存 npm 缓存：{error}"))?;
    }

    required_file(stage.join("package-lock.json"), "DSH 锁文件")?;

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
    verify_dsh_package_lock(stage, &version)?;
    Ok((entry, version))
}

fn verify_dsh_package_lock(stage: &Path, expected_version: &str) -> Result<(), String> {
    let lock_path = required_file(stage.join("package-lock.json"), "DSH 锁文件")?;
    let lock: serde_json::Value = serde_json::from_slice(
        &fs::read(&lock_path).map_err(|error| format!("无法读取 DSH 锁文件：{error}"))?,
    )
    .map_err(|error| format!("DSH 锁文件格式无效：{error}"))?;
    let locked = lock
        .get("packages")
        .and_then(|packages| packages.get("node_modules/@deepseek-ai/dsh"))
        .and_then(|package| package.get("version"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "DSH 锁文件缺少精确的 @deepseek-ai/dsh 版本".to_owned())?;
    if locked != expected_version {
        return Err(format!(
            "DSH 锁文件版本不一致：期望 {expected_version}，实际 {locked}"
        ));
    }
    Ok(())
}

fn run_npm_audit(stage: &Path) -> Result<(), String> {
    let stage_text = stage.to_string_lossy().into_owned();
    let registry = npm_registry_base_url()?;
    let mut audit = native_npm_command()?;
    audit.env_remove("NPM_CONFIG_PREFIX").args([
        "audit",
        "--prefix",
        &stage_text,
        "--omit=dev",
        "--audit-level=high",
        "--fetch-timeout=30000",
        "--fetch-retries=0",
        &format!("--registry={registry}"),
        "--json",
    ]);
    run_native_update_command(&mut audit, "检查 npm 高危漏洞")?;
    Ok(())
}

fn promote_dsh_candidate(stage: &Path) -> Result<PathBuf, String> {
    let paths = app_paths()?;
    let expected_parent = paths.update_root.join("dsh-staging");
    let stage_metadata = fs::symlink_metadata(stage).ok();
    if stage.parent() != Some(expected_parent.as_path())
        || stage_metadata
            .as_ref()
            .is_none_or(|metadata| is_reparse_point(metadata) || !metadata.is_dir())
    {
        return Err("拒绝提交非启动器创建的 DSH 暂存目录".to_owned());
    }
    let target = &paths.npm_prefix;
    let target_metadata =
        fs::symlink_metadata(target).map_err(|error| format!("无法检查当前 DSH 目录：{error}"))?;
    if is_reparse_point(&target_metadata) || !target_metadata.is_dir() {
        return Err(format!("拒绝覆盖 DSH 符号链接目录：{}", target.display()));
    }
    let backup = paths
        .update_root
        .join(format!("dsh-rollback-{}", transaction_nonce()));
    fs::rename(target, &backup).map_err(|error| format!("无法保留当前 DSH 回滚版本：{error}"))?;
    if let Err(error) = fs::rename(stage, target) {
        let restore = fs::rename(&backup, target);
        return match restore {
            Ok(()) => Err(format!("无法提交 DSH 暂存目录：{error}")),
            Err(restore_error) => Err(format!(
                "无法提交 DSH 暂存目录：{error}；当前版本恢复失败：{restore_error}"
            )),
        };
    }
    Ok(backup)
}

fn rollback_dsh_candidate(backup: &Path) -> Result<(), String> {
    let paths = app_paths()?;
    let backup_metadata = fs::symlink_metadata(backup).ok();
    if backup.parent() != Some(paths.update_root.as_path())
        || backup_metadata
            .as_ref()
            .is_none_or(|metadata| is_reparse_point(metadata) || !metadata.is_dir())
    {
        return Err("DSH 回滚目录无效".to_owned());
    }
    let failed = paths
        .update_root
        .join(format!("dsh-failed-{}", transaction_nonce()));
    ensure_safe_directory(&paths.update_root)?;
    fs::rename(&paths.npm_prefix, &failed)
        .map_err(|error| format!("无法暂存失败的 DSH 目录：{error}"))?;
    if let Err(error) = fs::rename(backup, &paths.npm_prefix) {
        let restore = fs::rename(&failed, &paths.npm_prefix);
        return match restore {
            Ok(()) => Err(format!("无法恢复 DSH 回滚目录：{error}")),
            Err(restore_error) => Err(format!(
                "无法恢复 DSH 回滚目录：{error}；失败目录恢复失败：{restore_error}"
            )),
        };
    }
    fs::remove_dir_all(&failed)
        .map_err(|error| format!("DSH 已恢复，但失败目录清理失败：{error}"))?;
    let restored = native_dsh_version()?;
    if parse_dsh_semver(&restored).is_none() {
        return Err(format!("恢复后的 DSH 版本无效：{restored}"));
    }
    write_runtime_ready_marker(&paths)
}

fn cleanup_old_dsh_rollbacks(paths: &AppPaths, keep: &Path) -> Result<(), String> {
    let entries = fs::read_dir(&paths.update_root)
        .map_err(|error| format!("无法读取 DSH 回滚目录：{error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("无法读取 DSH 回滚目录项：{error}"))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if path != keep && name.starts_with("dsh-rollback-") {
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("无法检查旧 DSH 回滚目录：{error}"))?;
            if is_reparse_point(&metadata) || !metadata.is_dir() {
                return Err(format!("拒绝清理无效 DSH 回滚目录：{}", path.display()));
            }
            fs::remove_dir_all(&path)
                .map_err(|error| format!("无法清理旧 DSH 回滚目录：{error}"))?;
        }
    }
    Ok(())
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
        if CANCEL_REQUESTED.load(Ordering::Acquire) {
            return Err("更新预检已取消".to_owned());
        }
        let root_ok = is_http_success(UPGRADE_PREFLIGHT_PORT, "/");
        let config_ok = http_json_status(UPGRADE_PREFLIGHT_PORT, QUOTA_CONFIG_PATH)
            .map(|(status, value)| {
                is_successful_http_status(status) && (value.is_object() || value.is_array())
            })
            .unwrap_or(false);
        if root_ok && config_ok {
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

fn is_safe_dsh_version(version: &str) -> bool {
    parse_dsh_semver(version).is_some()
}

fn runtime_manifest() -> Result<RuntimeManifest, String> {
    RUNTIME_MANIFEST
        .get_or_init(|| parse_runtime_manifest(RUNTIME_MANIFEST_TEXT))
        .clone()
}

fn parse_runtime_manifest(value: &str) -> Result<RuntimeManifest, String> {
    let root: serde_json::Value =
        serde_json::from_str(value).map_err(|error| format!("运行时清单 JSON 无效：{error}"))?;
    let get_string = |object: &serde_json::Value, name: &str| {
        object
            .get(name)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("运行时清单缺少 {name}"))
    };
    let schema_version = root
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "运行时清单 schema_version 无效".to_owned())?;
    let architecture = get_string(&root, "architecture")?;
    let node = root
        .get("node")
        .ok_or_else(|| "运行时清单缺少 node".to_owned())?;
    let dsh = root
        .get("dsh")
        .ok_or_else(|| "运行时清单缺少 dsh".to_owned())?;
    let quota = root
        .get("quota")
        .ok_or_else(|| "运行时清单缺少 quota".to_owned())?;
    let dsh_peer_dependencies = dsh
        .get("peer_dependencies")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "运行时清单缺少 dsh.peer_dependencies".to_owned())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| "运行时清单 dsh.peer_dependencies 包含无效项目".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let manifest = RuntimeManifest {
        schema_version,
        architecture,
        node_version: get_string(node, "version")?,
        node_archive_name: get_string(node, "archive_name")?,
        node_url: get_string(node, "url")?,
        node_sha256: get_string(node, "sha256")?,
        dsh_package: get_string(dsh, "package")?,
        dsh_bootstrap_version: get_string(dsh, "bootstrap_version")?,
        dsh_registry_url: get_string(dsh, "registry_url")?,
        dsh_entry: get_string(dsh, "entry")?,
        dsh_peer_dependencies,
        quota_package: get_string(quota, "package")?,
        quota_runtime_name: get_string(quota, "runtime_name")?,
        quota_version: get_string(quota, "version")?,
        quota_archive_name: get_string(quota, "archive_name")?,
        quota_url: get_string(quota, "url")?,
        quota_sha256: get_string(quota, "sha256")?,
    };
    if manifest.schema_version != 1
        || manifest.architecture != "x86_64-pc-windows-gnu"
        || !manifest
            .node_url
            .starts_with("https://nodejs.org/download/release/")
        || !manifest.node_url.ends_with(&manifest.node_archive_name)
        || parse_sha256(&manifest.node_sha256).is_none()
        || !is_safe_dsh_version(&manifest.node_version)
        || !is_safe_dsh_version(&manifest.dsh_bootstrap_version)
        || manifest.dsh_package != "@deepseek-ai/dsh"
        || manifest.dsh_registry_url != "https://registry.npmjs.org/@deepseek-ai%2fdsh"
        || manifest.dsh_entry != "lib/bin.js"
        || manifest.dsh_peer_dependencies.iter().any(|spec| {
            let Some((package, version)) = spec.rsplit_once('@') else {
                return true;
            };
            package.is_empty() || parse_dsh_semver(version).is_none()
        })
        || {
            let mut dependencies = manifest.dsh_peer_dependencies.clone();
            dependencies.sort();
            dependencies.dedup();
            dependencies.len() != manifest.dsh_peer_dependencies.len()
        }
        || manifest.quota_package != "@francescoli/dsh-quota"
        || manifest.quota_runtime_name != "dsh-quota"
        || !is_safe_dsh_version(&manifest.quota_version)
        || !manifest
            .quota_url
            .starts_with("https://registry.npmjs.org/")
        || !manifest.quota_url.ends_with(&manifest.quota_archive_name)
        || parse_sha256(&manifest.quota_sha256).is_none()
    {
        return Err("运行时清单字段不符合 Windows x64 便携发布契约".to_owned());
    }
    Ok(manifest)
}

fn portable_manifest_path(executable_dir: &Path) -> PathBuf {
    executable_dir.join(RUNTIME_MANIFEST_FILE)
}

fn verify_portable_manifest(executable_dir: &Path) -> Result<(), String> {
    let path = portable_manifest_path(executable_dir);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("便携包缺少运行时清单 {}：{error}", path.display()))?;
    if is_reparse_point(&metadata) || !metadata.is_file() {
        return Err(format!("便携包运行时清单不是普通文件：{}", path.display()));
    }
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("便携包缺少运行时清单 {}：{error}", path.display()))?;
    let external = parse_runtime_manifest(&contents)?;
    let embedded = runtime_manifest()?;
    if external != embedded {
        return Err(format!(
            "便携包运行时清单与启动器内置清单不一致：{}",
            path.display()
        ));
    }
    Ok(())
}

fn app_paths() -> Result<AppPaths, String> {
    APP_PATHS
        .get_or_init(|| {
            let args: Vec<String> = env::args().collect();
            let executable =
                env::current_exe().map_err(|error| format!("无法定位启动器目录：{error}"))?;
            let executable_dir = executable
                .parent()
                .ok_or_else(|| "无法定位启动器目录".to_owned())?;
            let data_override = data_dir_override(&args)?;
            let data_root = resolve_data_root(executable_dir, data_override)?;
            verify_portable_manifest(executable_dir)?;
            let paths = AppPaths::from_data_root(data_root);
            paths.ensure_layout()?;
            recover_migration_journal(&paths)?;
            cleanup_stale_update_directories(&paths)?;
            Ok(paths)
        })
        .clone()
}

fn resolve_data_root(
    executable_dir: &Path,
    override_root: Option<PathBuf>,
) -> Result<PathBuf, String> {
    let marker = executable_dir.join(PORTABLE_MARKER_FILE);
    let marker_is_valid = fs::symlink_metadata(&marker)
        .map(|metadata| metadata.is_file() && !is_reparse_point(&metadata))
        .unwrap_or(false);
    if !marker_is_valid {
        return Err(format!(
            "请下载并完整解压便携包；启动器目录缺少 {}：{}",
            PORTABLE_MARKER_FILE,
            executable_dir.display()
        ));
    }
    let colocated = executable_dir.join(DATA_DIRECTORY);
    if let Some(root) = override_root {
        let requested = if root.is_absolute() {
            root
        } else {
            executable_dir.join(root)
        };
        let requested = fs::canonicalize(&requested).unwrap_or(requested);
        let expected = fs::canonicalize(&colocated).unwrap_or(colocated.clone());
        if requested != expected {
            return Err(format!(
                "严格便携模式只允许 EXE 同目录 data；拒绝使用：{}",
                requested.display()
            ));
        }
    }
    if !ensure_writable_directory(&colocated) {
        return Err(format!(
            "便携模式要求启动器目录可写：{}",
            executable_dir.display()
        ));
    }
    Ok(colocated)
}

fn recover_migration_journal(paths: &AppPaths) -> Result<(), String> {
    let journal = paths.state_root.join(LEGACY_MIGRATION_JOURNAL_FILE);
    let metadata = match fs::symlink_metadata(&journal) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("无法读取迁移事务日志：{error}")),
    };
    if is_reparse_point(&metadata) || !metadata.is_file() {
        return Err(format!("迁移事务日志不是普通文件：{}", journal.display()));
    }
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(&journal).map_err(|error| format!("无法读取迁移事务日志：{error}"))?,
    )
    .map_err(|error| format!("迁移事务日志格式无效：{error}"))?;
    let state = value
        .get("state")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "迁移事务日志缺少状态".to_owned())?;
    let root = value
        .get("root")
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "迁移事务日志缺少事务根目录".to_owned())?;
    let migration_root = paths.update_root.join(MIGRATION_DIRECTORY);
    if root.parent() != Some(migration_root.as_path())
        || !root
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("migration-"))
    {
        return Err("迁移事务根目录不在当前便携包的 data\\updates 下".to_owned());
    }
    let committed = value
        .get("committed")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "迁移事务日志缺少 committed 列表".to_owned())?;
    let committed_ids = committed
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|id| matches!(*id, "runtime" | "npm-global" | "dsh-profile"))
                .map(str::to_owned)
                .ok_or_else(|| "迁移事务日志包含未知 committed ID".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut unique_ids = committed_ids.clone();
    unique_ids.sort();
    unique_ids.dedup();
    if unique_ids.len() != committed_ids.len() {
        return Err("迁移事务日志包含重复 committed ID".to_owned());
    }
    let journal_candidates = value
        .get("candidates")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "迁移事务日志缺少 candidates 列表".to_owned())?;
    let candidate_ids = journal_candidates
        .iter()
        .map(|candidate| {
            candidate
                .get("id")
                .and_then(serde_json::Value::as_str)
                .filter(|id| matches!(*id, "runtime" | "npm-global" | "dsh-profile"))
                .map(str::to_owned)
                .ok_or_else(|| "迁移事务日志包含未知 candidate ID".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut unique_candidate_ids = candidate_ids.clone();
    unique_candidate_ids.sort();
    unique_candidate_ids.dedup();
    if unique_candidate_ids.len() != candidate_ids.len() {
        return Err("迁移事务日志包含重复 candidate ID".to_owned());
    }
    if !committed_ids
        .iter()
        .all(|id| candidate_ids.iter().any(|candidate_id| candidate_id == id))
    {
        return Err("迁移事务日志的 committed ID 不在 candidates 列表中".to_owned());
    }
    let candidates = legacy_data_sources(paths)
        .into_iter()
        .filter(|candidate| candidate_ids.iter().any(|id| id == candidate.id))
        .collect::<Vec<_>>();
    for candidate in &candidates {
        if committed_ids.iter().any(|id| id == candidate.id)
            && (paths_overlap(&candidate.source, &paths.data_root)
                || paths_overlap(&candidate.source, &candidate.target))
        {
            return Err(format!(
                "迁移事务源目录与当前便携目录重叠，拒绝恢复：{}",
                candidate.source.display()
            ));
        }
    }
    let root_exists = match fs::symlink_metadata(&root) {
        Ok(metadata) => {
            if is_reparse_point(&metadata) || !metadata.is_dir() {
                return Err(format!("迁移事务根目录不是普通目录：{}", root.display()));
            }
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(format!("无法检查迁移事务根目录：{error}")),
    };
    let mut recovery_committed_ids = committed_ids;
    if root_exists && state == "committing" {
        let backup_root = root.join("backups");
        let stage_root = root.join("stage");
        for candidate in &candidates {
            if recovery_committed_ids.iter().any(|id| id == candidate.id) {
                continue;
            }
            let backup = backup_root.join(candidate.id);
            let staged = stage_root.join(candidate.id);
            if fs::symlink_metadata(&backup)
                .ok()
                .is_some_and(|metadata| !is_reparse_point(&metadata) && metadata.is_dir())
                && fs::symlink_metadata(&staged).is_err()
                && candidate.target.is_dir()
            {
                recovery_committed_ids.push(candidate.id.to_owned());
            }
        }
    }
    let committed_refs = recovery_committed_ids
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    match state {
        "completed" | "cleanup_failed" => {
            let marker = paths.state_root.join(LEGACY_MIGRATION_MARKER_FILE);
            ensure_safe_file_destination(&marker, "旧数据迁移标记")?;
            fs::write(&marker, "completed\n")
                .map_err(|error| format!("无法补写迁移完成标记：{error}"))?;
            if root_exists {
                fs::remove_dir_all(&root)
                    .map_err(|error| format!("无法清理已完成迁移事务：{error}"))?;
            }
        }
        "pending" | "validated" | "committing" | "failed" => {
            if root_exists {
                rollback_migration(&root, &candidates, &committed_refs)?;
            }
        }
        other => return Err(format!("迁移事务日志状态无效：{other}")),
    }
    fs::remove_file(&journal).map_err(|error| format!("无法清理迁移事务日志：{error}"))
}

fn ensure_writable_directory(path: &Path) -> bool {
    if ensure_safe_directory(path).is_err() {
        return false;
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let probe = path.join(format!(
        ".dsh-launcher-write-test-{}-{nonce}",
        std::process::id()
    ));
    match OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(file) => {
            drop(file);
            let _ = fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

fn ensure_safe_directory(path: &Path) -> Result<(), String> {
    validate_existing_directory_chain(path)?;
    fs::create_dir_all(path)
        .map_err(|error| format!("无法创建应用数据目录 {}：{error}", path.display()))?;
    validate_existing_directory_chain(path)
}

fn validate_existing_directory_chain(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        let metadata = match fs::symlink_metadata(ancestor) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "无法检查应用数据目录 {}：{error}",
                    ancestor.display()
                ));
            }
        };
        if is_reparse_point(&metadata) {
            return Err(format!(
                "拒绝使用符号链接或重解析点目录：{}",
                ancestor.display()
            ));
        }
        if !metadata.is_dir() {
            return Err(format!("应用数据路径不是目录：{}", ancestor.display()));
        }
    }
    Ok(())
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

fn ensure_existing_directory(path: &Path, label: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("无法读取{label} {}：{error}", path.display()))?;
    if is_reparse_point(&metadata) {
        return Err(format!(
            "拒绝使用符号链接或重解析点{label}：{}",
            path.display()
        ));
    }
    if !metadata.is_dir() {
        return Err(format!("{label}不是目录：{}", path.display()));
    }
    validate_existing_directory_chain(path)
}

fn ensure_safe_file_destination(path: &Path, label: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{label}路径没有父目录"))?;
    ensure_safe_directory(parent)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_reparse_point(&metadata) => Err(format!(
            "拒绝写入符号链接或重解析点{label}：{}",
            path.display()
        )),
        Ok(metadata) if !metadata.is_file() => {
            Err(format!("{label}不是普通文件：{}", path.display()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法检查{label} {}：{error}", path.display())),
    }
}

fn path_drive_letter(path: &Path) -> Option<u8> {
    match path.components().next()? {
        Component::Prefix(prefix) => match prefix.kind() {
            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => Some(drive),
            _ => None,
        },
        _ => None,
    }
}

fn is_system_drive(path: &Path) -> bool {
    let current = path_drive_letter(path);
    let system = env::var_os("SystemDrive")
        .and_then(|value| value.to_string_lossy().as_bytes().first().copied());
    current
        .zip(system)
        .is_some_and(|(current, system)| current.eq_ignore_ascii_case(&system))
}

fn legacy_data_sources(paths: &AppPaths) -> Vec<MigrationCandidate> {
    let Some(local_app_data) = env::var_os("LOCALAPPDATA").map(PathBuf::from) else {
        return Vec::new();
    };
    let Some(user_profile) = env::var_os("USERPROFILE").map(PathBuf::from) else {
        return Vec::new();
    };
    [
        MigrationCandidate {
            id: "runtime",
            label: "Node.js 运行时",
            source: local_app_data.join("DSH-Runtime"),
            target: paths.runtime_root.clone(),
        },
        MigrationCandidate {
            id: "npm-global",
            label: "DSH npm 全局包",
            source: local_app_data.join("npm-global"),
            target: paths.npm_prefix.clone(),
        },
        MigrationCandidate {
            id: "dsh-profile",
            label: "DSH 配置与插件",
            source: user_profile.join(".dsh"),
            target: paths.dsh_home.clone(),
        },
    ]
    .into_iter()
    .collect()
}

fn migration_candidates(paths: &AppPaths) -> Vec<MigrationCandidate> {
    legacy_data_sources(paths)
        .into_iter()
        .filter(|candidate| {
            candidate.source != candidate.target
                && !paths_overlap(&candidate.source, &paths.data_root)
                && !paths_overlap(&candidate.source, &candidate.target)
                && directory_has_entries(&candidate.source)
                && !directory_has_entries(&candidate.target)
        })
        .collect()
}

fn legacy_cleanup_candidates(paths: &AppPaths) -> Result<Vec<(MigrationCandidate, u64)>, String> {
    let mut candidates = Vec::new();
    for candidate in legacy_data_sources(paths) {
        let metadata = match fs::symlink_metadata(&candidate.source) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "无法读取旧数据目录 {}：{error}",
                    candidate.source.display()
                ));
            }
        };
        if is_reparse_point(&metadata) || !metadata.is_dir() {
            return Err(format!(
                "拒绝处理非普通旧数据目录：{}",
                candidate.source.display()
            ));
        }
        if !directory_has_entries(&candidate.source) {
            continue;
        }
        if paths_overlap(&candidate.source, &paths.data_root)
            || paths_overlap(&candidate.source, &candidate.target)
        {
            return Err(format!(
                "拒绝把便携数据目录当作旧数据处理：{}",
                candidate.source.display()
            ));
        }
        let size = directory_size(&candidate.source)?;
        candidates.push((candidate, size));
    }
    Ok(candidates)
}

fn normalized_path_key(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

fn path_is_same_or_below(path: &Path, root: &Path) -> bool {
    let path = normalized_path_key(path);
    let root = normalized_path_key(root);
    path == root
        || path
            .strip_prefix(&root)
            .is_some_and(|suffix| suffix.starts_with('\\'))
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    path_is_same_or_below(left, right) || path_is_same_or_below(right, left)
}

fn directory_size(root: &Path) -> Result<u64, String> {
    fn visit(path: &Path) -> Result<u64, String> {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| format!("无法读取 {}：{error}", path.display()))?;
        if is_reparse_point(&metadata) {
            return Err(format!("拒绝处理符号链接：{}", path.display()));
        }
        if metadata.is_file() {
            return Ok(metadata.len());
        }
        if !metadata.is_dir() {
            return Err(format!("旧数据目录包含未知文件类型：{}", path.display()));
        }
        let mut total = 0u64;
        for entry in
            fs::read_dir(path).map_err(|error| format!("无法读取 {}：{error}", path.display()))?
        {
            let child = entry
                .map_err(|error| format!("无法读取旧数据目录项：{error}"))?
                .path();
            total = total.saturating_add(visit(&child)?);
        }
        Ok(total)
    }

    visit(root)
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut index = 0usize;
    while value >= 1024.0 && index < UNITS.len() - 1 {
        value /= 1024.0;
        index += 1;
    }
    if index == 0 {
        format!("{} {}", bytes, UNITS[index])
    } else {
        format!("{value:.1} {}", UNITS[index])
    }
}

fn legacy_cleanup_prompt() -> Result<(Vec<(MigrationCandidate, u64)>, String), String> {
    let paths = app_paths()?;
    if find_verified_dsh_pid(DSH_PORT)?.is_none() {
        return Err("请先启动并验证 DSH 正常运行，再移入旧数据回收站".to_owned());
    }
    let candidates = legacy_cleanup_candidates(&paths)?;
    if candidates.is_empty() {
        return Err("未发现可移入回收站的旧数据".to_owned());
    }
    let details = candidates
        .iter()
        .map(|(candidate, size)| {
            format!(
                "{}：{}（{}）",
                candidate.label,
                candidate.source.display(),
                format_bytes(*size)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let prompt = format!(
        "确认将以下旧数据移入 Windows 回收站？\n\n{details}\n\n这是不可逆操作前的第二次确认；文件进入回收站后可由你手动恢复。当前便携目录不会被删除。"
    );
    Ok((candidates, prompt))
}

fn cleanup_legacy_data_to_recycle_bin(progress: &dyn Fn(&str)) -> Result<String, String> {
    let paths = app_paths()?;
    if find_verified_dsh_pid(DSH_PORT)?.is_none() {
        return Err("DSH 未通过运行验证，已拒绝移入旧数据回收站".to_owned());
    }
    let candidates = legacy_cleanup_candidates(&paths)?;
    if candidates.is_empty() {
        return Ok("未发现可移入回收站的旧数据".to_owned());
    }
    let mut moved = 0usize;
    for (candidate, size) in candidates {
        if CANCEL_REQUESTED.load(Ordering::Acquire) {
            return Err("旧数据回收操作已取消".to_owned());
        }
        progress(&format!(
            "正在移入回收站：{}（{}）...",
            candidate.source.display(),
            format_bytes(size)
        ));
        let mut from = to_wide(&candidate.source.to_string_lossy());
        from.push(0);
        let mut operation = SHFILEOPSTRUCTW {
            hwnd: std::ptr::null_mut(),
            wFunc: FO_DELETE,
            pFrom: from.as_ptr(),
            pTo: std::ptr::null(),
            fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI) as u16,
            fAnyOperationsAborted: 0,
            hNameMappings: std::ptr::null_mut(),
            lpszProgressTitle: std::ptr::null(),
        };
        let result = unsafe { SHFileOperationW(&mut operation) };
        if result != 0 || operation.fAnyOperationsAborted != 0 {
            return Err(format!(
                "移入回收站失败：{}（错误码 {result}，已完成 {moved} 项）",
                candidate.source.display()
            ));
        }
        moved += 1;
    }
    Ok(format!(
        "已将 {moved} 项旧数据移入 Windows 回收站；当前便携数据未删除"
    ))
}

fn request_legacy_cleanup(hwnd: HWND, state: Arc<AppState>) {
    if state.busy.load(Ordering::Acquire) {
        push_status(
            hwnd,
            &state,
            "正在处理，请完成当前操作后再清理旧数据".to_owned(),
        );
        return;
    }
    let prompt = match legacy_cleanup_prompt() {
        Ok((_, prompt)) => prompt,
        Err(error) => {
            push_status(hwnd, &state, error);
            return;
        }
    };
    let prompt = to_wide(&prompt);
    let title = to_wide("移入旧数据回收站");
    let confirmed = unsafe {
        MessageBoxW(
            hwnd,
            prompt.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONQUESTION,
        ) == IDYES
    };
    if confirmed {
        spawn_action(hwnd, state, Action::CleanupLegacy);
    }
}

fn request_legacy_migration(hwnd: HWND, state: Arc<AppState>, mark_reviewed_on_decline: bool) {
    if state.busy.load(Ordering::Acquire) {
        push_status(
            hwnd,
            &state,
            "正在处理，请完成当前操作后再迁移旧数据".to_owned(),
        );
        return;
    }
    let paths = match app_paths() {
        Ok(paths) => paths,
        Err(error) => {
            push_status(hwnd, &state, error);
            return;
        }
    };
    let candidates = migration_candidates(&paths);
    if candidates.is_empty() {
        if mark_reviewed_on_decline {
            let marker = paths.state_root.join(LEGACY_MIGRATION_MARKER_FILE);
            if let Err(error) = ensure_safe_file_destination(&marker, "旧数据迁移标记")
                .and_then(|()| fs::write(&marker, "completed\n").map_err(|error| error.to_string()))
            {
                push_status(hwnd, &state, format!("无法记录旧数据检查结果：{error}"));
            }
        } else {
            push_status(hwnd, &state, "未发现可迁移的旧版数据".to_owned());
        }
        return;
    }
    let prompt = format!(
        "检测到旧版 DSH 数据。是否复制到当前启动器的数据目录？\n\n{}\n\n复制前会逐文件校验大小和 SHA-256；旧数据不会自动删除。",
        migration_details(&candidates)
    );
    let prompt = to_wide(&prompt);
    let title = to_wide("迁移 DSH 数据");
    let confirmed = unsafe {
        MessageBoxW(
            hwnd,
            prompt.as_ptr(),
            title.as_ptr(),
            MB_YESNO | MB_ICONQUESTION,
        ) == IDYES
    };
    if confirmed {
        spawn_action(hwnd, state, Action::Migrate);
    } else if mark_reviewed_on_decline {
        let marker = paths.state_root.join(LEGACY_MIGRATION_MARKER_FILE);
        if let Err(error) = ensure_safe_file_destination(&marker, "旧数据迁移标记")
            .and_then(|()| fs::write(&marker, "declined\n").map_err(|error| error.to_string()))
        {
            push_status(hwnd, &state, format!("无法记录迁移选择：{error}"));
        }
    }
}

fn migration_details(candidates: &[MigrationCandidate]) -> String {
    candidates
        .iter()
        .map(|candidate| {
            format!(
                "{}：{} → {}",
                candidate.label,
                candidate.source.display(),
                candidate.target.display()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn maybe_offer_legacy_migration(hwnd: HWND, state: Arc<AppState>) {
    let Ok(paths) = app_paths() else {
        return;
    };
    let migration_marker = paths.state_root.join(LEGACY_MIGRATION_MARKER_FILE);
    if migration_marker.is_file() {
        return;
    }
    request_legacy_migration(hwnd, state, true);
}

fn migrate_legacy_data() -> Result<String, String> {
    migrate_legacy_data_with_progress(&|_| {})
}

fn migrate_legacy_data_with_progress(progress: &dyn Fn(&str)) -> Result<String, String> {
    let paths = app_paths()?;
    if verified_dsh_state(DSH_PORT)? {
        return Err("请先停止 DSH，再迁移旧数据以避免替换正在使用的运行时或配置".to_owned());
    }
    let candidates = migration_candidates(&paths);
    if candidates.is_empty() {
        return Ok("未发现可迁移的旧版数据".to_owned());
    }
    progress("正在创建迁移事务...");
    let nonce = transaction_nonce();
    let root = paths
        .update_root
        .join(MIGRATION_DIRECTORY)
        .join(format!("migration-{nonce}-{}", std::process::id()));
    let stage_root = root.join("stage");
    let backup_root = root.join("backups");
    ensure_safe_directory(&stage_root)
        .and_then(|_| ensure_safe_directory(&backup_root))
        .map_err(|error| format!("无法创建迁移暂存目录：{error}"))?;
    let journal = paths.state_root.join(LEGACY_MIGRATION_JOURNAL_FILE);
    write_migration_journal(&journal, "pending", &root, &candidates, &[], &[])?;

    let mut validated = Vec::new();
    let mut committed = Vec::new();
    let mut migration_finalized = false;
    let result = (|| {
        for candidate in &candidates {
            if CANCEL_REQUESTED.load(Ordering::Acquire) {
                return Err("迁移已取消".to_owned());
            }
            progress(&format!("正在校验并复制 {}...", candidate.label));
            let staged = stage_root.join(candidate.id);
            let source_before = collect_file_fingerprints(&candidate.source)?;
            copy_directory_contents(&candidate.source, &staged)?;
            let source_after = collect_file_fingerprints(&candidate.source)?;
            let staged_manifest = collect_file_fingerprints(&staged)?;
            if source_before != source_after {
                return Err(format!(
                    "{} 迁移校验失败，源目录在复制期间发生变化",
                    candidate.label
                ));
            }
            if source_after != staged_manifest {
                return Err(format!("{} 迁移校验失败，源目录未修改", candidate.label));
            }
            validated.push(candidate.id);
            write_migration_journal(
                &journal,
                "validated",
                &root,
                &candidates,
                &validated,
                &committed,
            )?;
        }

        for candidate in &candidates {
            if CANCEL_REQUESTED.load(Ordering::Acquire) {
                return Err("迁移已取消".to_owned());
            }
            let backup = backup_root.join(candidate.id);
            if candidate.target.exists() {
                fs::rename(&candidate.target, &backup).map_err(|error| {
                    format!("无法暂存现有 {} 目标目录：{error}", candidate.label)
                })?;
            }
            let staged = stage_root.join(candidate.id);
            if let Err(error) = fs::rename(&staged, &candidate.target) {
                return Err(format!("无法提交 {} 迁移目录：{error}", candidate.label));
            }
            committed.push(candidate.id);
            write_migration_journal(
                &journal,
                "committing",
                &root,
                &candidates,
                &validated,
                &committed,
            )?;
        }
        write_migration_journal(
            &journal,
            "completed",
            &root,
            &candidates,
            &validated,
            &committed,
        )?;
        let marker = paths.state_root.join(LEGACY_MIGRATION_MARKER_FILE);
        ensure_safe_file_destination(&marker, "旧数据迁移标记")?;
        fs::write(&marker, "completed\n")
            .map_err(|error| format!("无法写入迁移完成标记：{error}"))?;
        migration_finalized = true;
        let mut cleanup_errors = Vec::new();
        if let Err(error) = fs::remove_dir_all(&root) {
            if error.kind() != std::io::ErrorKind::NotFound {
                cleanup_errors.push(format!("事务目录：{error}"));
            }
        }
        if let Err(error) = fs::remove_dir(paths.update_root.join(MIGRATION_DIRECTORY)) {
            if error.kind() != std::io::ErrorKind::NotFound {
                cleanup_errors.push(format!("迁移根目录：{error}"));
            }
        }
        if !cleanup_errors.is_empty() {
            return Err(format!(
                "迁移已提交，但暂存清理失败：{}",
                cleanup_errors.join("；")
            ));
        }
        Ok::<(), String>(())
    })();

    if let Err(error) = result {
        if migration_finalized {
            let _ = write_migration_journal(
                &journal,
                "cleanup_failed",
                &root,
                &candidates,
                &validated,
                &committed,
            );
            return Err(format!(
                "迁移已完成，但事务目录清理失败：{error}；当前数据和旧目录均未删除"
            ));
        }
        let rollback = rollback_migration(&root, &candidates, &committed);
        let detail = match rollback {
            Ok(()) => "目标目录已回滚，旧数据仍保留".to_owned(),
            Err(rollback_error) => format!("目标目录回滚失败：{rollback_error}"),
        };
        let _ = write_migration_journal(
            &journal,
            "failed",
            &root,
            &candidates,
            &validated,
            &committed,
        );
        return Err(format!("迁移失败：{error}；{detail}；可重新执行迁移"));
    }
    Ok(format!(
        "旧数据迁移完成：{}；旧目录仍保留",
        migration_details(&candidates)
    ))
}

fn write_migration_journal(
    path: &Path,
    state: &str,
    root: &Path,
    candidates: &[MigrationCandidate],
    validated: &[&str],
    committed: &[&str],
) -> Result<(), String> {
    let value = serde_json::json!({
        "schema_version": 1,
        "state": state,
        "root": root,
        "validated": validated,
        "committed": committed,
        "candidates": candidates.iter().map(|candidate| serde_json::json!({
            "id": candidate.id,
            "label": candidate.label,
            "source": candidate.source,
            "target": candidate.target,
        })).collect::<Vec<_>>(),
    });
    let encoded = serde_json::to_vec_pretty(&value)
        .map_err(|error| format!("无法序列化迁移事务日志：{error}"))?;
    let partial = path.with_extension("journal.partial");
    ensure_safe_file_destination(&partial, "迁移事务暂存文件")?;
    ensure_safe_file_destination(path, "迁移事务日志")?;
    fs::write(&partial, encoded).map_err(|error| format!("无法写入迁移事务日志：{error}"))?;
    fs::rename(&partial, path).map_err(|error| format!("无法提交迁移事务日志：{error}"))
}

fn rollback_migration(
    root: &Path,
    candidates: &[MigrationCandidate],
    committed: &[&str],
) -> Result<(), String> {
    let backup_root = root.join("backups");
    let stage_root = root.join("stage");
    let mut failures = Vec::new();
    for candidate in candidates.iter().rev() {
        let backup = backup_root.join(candidate.id);
        let staged = stage_root.join(candidate.id);
        let was_committed = committed.contains(&candidate.id);
        let backup_metadata = fs::symlink_metadata(&backup);
        let backup_missing = matches!(
            &backup_metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        );
        let backup_exists = match &backup_metadata {
            Ok(metadata) => {
                if is_reparse_point(metadata) || !metadata.is_dir() {
                    failures.push(format!("{} 备份目录无效，拒绝恢复", candidate.label));
                    false
                } else {
                    true
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => {
                failures.push(format!("{} 备份目录检查失败：{error}", candidate.label));
                false
            }
        };
        if was_committed && backup_missing {
            failures.push(format!(
                "{} 缺少有效备份，拒绝删除已提交目标",
                candidate.label
            ));
        }
        let should_remove_target = backup_exists;
        if should_remove_target {
            match fs::symlink_metadata(&candidate.target) {
                Ok(metadata) if is_reparse_point(&metadata) || !metadata.is_dir() => {
                    failures.push(format!(
                        "{} 目标不是可回滚的普通目录，拒绝清理",
                        candidate.label
                    ));
                    continue;
                }
                Ok(_) => {
                    let target_empty = match directory_is_empty(&candidate.target) {
                        Ok(empty) => empty,
                        Err(error) => {
                            failures.push(format!("{} 目标内容检查失败：{error}", candidate.label));
                            continue;
                        }
                    };
                    let target_is_migrated_copy = if target_empty {
                        true
                    } else if was_committed {
                        match migrated_target_matches_source(candidate) {
                            Ok(matches) => matches,
                            Err(error) => {
                                failures.push(format!(
                                    "{} 目标内容无法验证，拒绝清理：{error}",
                                    candidate.label
                                ));
                                continue;
                            }
                        }
                    } else {
                        false
                    };
                    if !target_is_migrated_copy {
                        failures.push(format!(
                            "{} 目标目录包含未验证内容，拒绝覆盖或删除",
                            candidate.label
                        ));
                        continue;
                    }
                    if let Err(error) = fs::remove_dir_all(&candidate.target) {
                        failures.push(format!("{} 目标清理失败：{error}", candidate.label));
                        continue;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    failures.push(format!("{} 目标检查失败：{error}", candidate.label));
                    continue;
                }
            }
        }
        if backup_exists {
            match backup_metadata {
                Ok(_) => {
                    if let Err(error) = fs::rename(&backup, &candidate.target) {
                        failures.push(format!("{} 目标恢复失败：{error}", candidate.label));
                    }
                }
                Err(error) => {
                    failures.push(format!("{} 备份目录检查失败：{error}", candidate.label));
                }
            }
        }
        match fs::symlink_metadata(&staged) {
            Ok(metadata) if is_reparse_point(&metadata) || !metadata.is_dir() => {
                failures.push(format!("{} 暂存目录无效，拒绝清理", candidate.label));
            }
            Ok(_) => {
                if let Err(error) = fs::remove_dir_all(&staged) {
                    failures.push(format!("{} 暂存目录清理失败：{error}", candidate.label));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => failures.push(format!("{} 暂存目录检查失败：{error}", candidate.label)),
        }
    }
    if failures.is_empty() {
        if let Err(error) = fs::remove_dir_all(root) {
            if error.kind() != std::io::ErrorKind::NotFound {
                failures.push(format!("迁移事务目录清理失败：{error}"));
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("；"))
    }
}

fn directory_is_empty(path: &Path) -> Result<bool, String> {
    let mut entries =
        fs::read_dir(path).map_err(|error| format!("无法读取目录 {}：{error}", path.display()))?;
    Ok(entries.next().is_none())
}

fn migrated_target_matches_source(candidate: &MigrationCandidate) -> Result<bool, String> {
    Ok(collect_file_fingerprints(&candidate.source)?
        == collect_file_fingerprints(&candidate.target)?)
}

fn directory_has_entries(path: &Path) -> bool {
    fs::read_dir(path)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

fn copy_directory_contents(source: &Path, target: &Path) -> Result<(), String> {
    ensure_existing_directory(source, "源目录")?;
    ensure_safe_directory(target)
        .map_err(|error| format!("无法创建目标目录 {}：{error}", target.display()))?;
    let entries = fs::read_dir(source)
        .map_err(|error| format!("无法读取旧目录 {}：{error}", source.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("无法读取旧目录项：{error}"))?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)
            .map_err(|error| format!("无法读取 {} 的类型：{error}", source_path.display()))?;
        if is_reparse_point(&metadata) {
            return Err(format!("拒绝迁移符号链接：{}", source_path.display()));
        }
        if metadata.is_dir() {
            copy_directory_contents(&source_path, &target_path)?;
        } else if metadata.is_file() {
            if target_path.exists() {
                return Err(format!("目标文件已存在：{}", target_path.display()));
            }
            fs::copy(&source_path, &target_path).map_err(|error| {
                format!(
                    "无法复制 {} 到 {}：{error}",
                    source_path.display(),
                    target_path.display()
                )
            })?;
        } else {
            return Err(format!(
                "目录包含无法迁移的文件类型：{}",
                source_path.display()
            ));
        }
    }
    Ok(())
}

fn collect_file_fingerprints(root: &Path) -> Result<Vec<FileFingerprint>, String> {
    ensure_existing_directory(root, "校验目录")?;
    fn visit(root: &Path, current: &Path, result: &mut Vec<FileFingerprint>) -> Result<(), String> {
        let entries = fs::read_dir(current)
            .map_err(|error| format!("无法读取目录 {}：{error}", current.display()))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("无法读取目录项：{error}"))?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| format!("无法读取 {} 的类型：{error}", path.display()))?;
            if is_reparse_point(&metadata) {
                return Err(format!("拒绝校验符号链接：{}", path.display()));
            }
            if metadata.is_dir() {
                visit(root, &path, result)?;
            } else if metadata.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|error| format!("无法计算迁移相对路径：{error}"))?
                    .to_string_lossy()
                    .replace('\\', "/");
                let size = fs::metadata(&path)
                    .map_err(|error| format!("无法读取 {} 大小：{error}", path.display()))?
                    .len();
                result.push(FileFingerprint {
                    relative_path: relative,
                    size,
                    sha256: calculate_sha256(&path)?,
                });
            }
        }
        Ok(())
    }

    let mut result = Vec::new();
    visit(root, root, &mut result)?;
    result.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(result)
}

fn transaction_nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn cleanup_stale_update_directories(paths: &AppPaths) -> Result<(), String> {
    let now = SystemTime::now();
    let entries = fs::read_dir(&paths.update_root)
        .map_err(|error| format!("无法读取更新清理目录：{error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("无法读取更新清理目录项：{error}"))?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("无法检查更新清理项 {}：{error}", path.display()))?;
        if is_reparse_point(&metadata) {
            return Err(format!(
                "更新清理目录包含符号链接或重解析点，已拒绝处理：{}",
                path.display()
            ));
        }
        if metadata.is_file() {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name.ends_with(".download") && is_stale(&path, now) {
                fs::remove_file(&path)
                    .map_err(|error| format!("无法清理过期下载暂存文件：{error}"))?;
            }
            continue;
        }
        if !metadata.is_dir() {
            return Err(format!("更新清理项不是普通文件或目录：{}", path.display()));
        }
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name.starts_with("dsh-rollback-") {
            // The last verified DSH directory is the offline rollback boundary.
            // Successful updates prune older rollback directories explicitly.
            continue;
        }
        let candidate_roots = if name == MIGRATION_DIRECTORY {
            fs::read_dir(&path)
                .ok()
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|nested| nested.path())
                .collect::<Vec<_>>()
        } else {
            vec![path.clone()]
        };
        for candidate in candidate_roots {
            let metadata = fs::symlink_metadata(&candidate).map_err(|error| {
                format!("无法检查过期更新目录 {}：{error}", candidate.display())
            })?;
            if is_reparse_point(&metadata) || !metadata.is_dir() {
                return Err(format!("过期更新目录不是普通目录：{}", candidate.display()));
            }
            if is_stale(&candidate, now) {
                fs::remove_dir_all(&candidate).map_err(|error| {
                    format!("无法清理过期更新目录 {}：{error}", candidate.display())
                })?;
            }
        }
    }
    Ok(())
}

fn is_stale(path: &Path, now: SystemTime) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| now.duration_since(modified).ok())
        .is_some_and(|age| age > FAILED_STAGING_RETENTION)
}

fn native_runtime_root() -> Result<PathBuf, String> {
    Ok(app_paths()?.runtime_root)
}

fn native_npm_prefix() -> Result<PathBuf, String> {
    Ok(app_paths()?.npm_prefix)
}

fn native_dsh_log_path(file_name: &str) -> Result<PathBuf, String> {
    Ok(app_paths()?.log_root.join(file_name))
}

fn open_native_dsh_log(file_name: &str) -> Result<Stdio, String> {
    let path = native_dsh_log_path(file_name)?;
    let parent = path
        .parent()
        .ok_or_else(|| "无法定位 DSH 启动日志目录".to_owned())?;
    ensure_safe_directory(parent).map_err(|error| format!("无法创建 DSH 启动日志目录：{error}"))?;
    ensure_safe_file_destination(&path, "DSH 启动日志")?;
    rotate_log_file(&path)?;
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

fn rotate_log_file(path: &Path) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) if is_reparse_point(&metadata) || !metadata.is_file() => {
            return Err(format!("日志路径不是普通文件：{}", path.display()));
        }
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("无法检查日志 {}：{error}", path.display())),
    };
    if metadata.len() < LOG_ROTATION_LIMIT_BYTES {
        return Ok(());
    }
    let oldest = path.with_extension(format!("log.{}", LOG_ROTATION_FILES - 1));
    match fs::symlink_metadata(&oldest) {
        Ok(metadata) if is_reparse_point(&metadata) || !metadata.is_file() => {
            return Err(format!("拒绝覆盖无效旧日志：{}", oldest.display()));
        }
        Ok(_) => fs::remove_file(&oldest)
            .map_err(|error| format!("无法删除最旧日志 {}：{error}", oldest.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("无法检查最旧日志 {}：{error}", oldest.display())),
    }
    for index in (1..LOG_ROTATION_FILES - 1).rev() {
        let source = path.with_extension(format!("log.{index}"));
        let target = path.with_extension(format!("log.{}", index + 1));
        match fs::symlink_metadata(&source) {
            Ok(metadata) if is_reparse_point(&metadata) || !metadata.is_file() => {
                return Err(format!("拒绝轮转无效日志：{}", source.display()));
            }
            Ok(_) => {
                fs::rename(&source, &target)
                    .map_err(|error| format!("无法轮转日志 {}：{error}", path.display()))?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("无法检查日志 {}：{error}", source.display())),
        }
    }
    let first = path.with_extension("log.1");
    fs::rename(path, &first).map_err(|error| format!("无法轮转日志 {}：{error}", path.display()))
}

fn clear_npm_cache(paths: &AppPaths) -> Result<(), String> {
    match fs::symlink_metadata(&paths.npm_cache) {
        Ok(metadata) if is_reparse_point(&metadata) || !metadata.is_dir() => {
            return Err(format!(
                "npm 缓存路径不是普通目录：{}",
                paths.npm_cache.display()
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("无法读取 npm 缓存目录：{error}")),
    }
    for entry in
        fs::read_dir(&paths.npm_cache).map_err(|error| format!("无法读取 npm 缓存目录：{error}"))?
    {
        let path = entry
            .map_err(|error| format!("无法读取 npm 缓存项：{error}"))?
            .path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| format!("无法读取 npm 缓存项：{error}"))?;
        if is_reparse_point(&metadata) {
            return Err(format!("拒绝删除 npm 缓存符号链接：{}", path.display()));
        }
        if metadata.is_dir() {
            fs::remove_dir_all(&path).map_err(|error| format!("无法删除 npm 缓存目录：{error}"))?;
        } else {
            fs::remove_file(&path).map_err(|error| format!("无法删除 npm 缓存文件：{error}"))?;
        }
    }
    Ok(())
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
    match fs::symlink_metadata(&path) {
        Ok(metadata) if is_reparse_point(&metadata) => Err(format!(
            "拒绝使用符号链接或重解析点{label}：{}",
            path.display()
        )),
        Ok(metadata) if metadata.is_file() => Ok(path),
        Ok(_) => Err(format!("{label}不是普通文件：{}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(format!("未找到{label}：{}", path.display()))
        }
        Err(error) => Err(format!("无法读取{label} {}：{error}", path.display())),
    }
}

fn configure_native_environment(command: &mut Command) -> Result<(), String> {
    let paths = app_paths()?;
    let runtime_root = paths.runtime_root.clone();
    let prefix = paths.npm_prefix.clone();
    let dsh_home = paths.dsh_home.clone();
    let user_profile = dsh_home
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "无法定位 DSH 配置所在的用户目录".to_owned())?;
    let existing_path = env::var_os("PATH").unwrap_or_default();
    let mut search_paths = vec![runtime_root.join("node"), prefix.clone()];
    search_paths.extend(env::split_paths(&existing_path));
    let path = env::join_paths(search_paths)
        .map_err(|error| format!("无法准备 DSH 的运行环境：{error}"))?;

    command
        .env("PATH", path)
        .env("NPM_CONFIG_PREFIX", &prefix)
        .env("NPM_CONFIG_CACHE", &paths.npm_cache)
        .env("NPM_CONFIG_USERCONFIG", paths.dsh_home.join("npmrc"))
        .env("DSH_HOME", &dsh_home)
        .env("TEMP", &paths.temp_root)
        .env("TMP", &paths.temp_root)
        .current_dir(user_profile);
    Ok(())
}

fn native_dsh_pid_path() -> Result<PathBuf, String> {
    Ok(app_paths()?.pid_path())
}

fn write_native_dsh_process(pid: u32) -> Result<NativeDshProcess, String> {
    let Some(started_at) = native_process_started_at(pid)? else {
        return Err("服务启动进程在记录前已退出".to_owned());
    };
    let process = NativeDshProcess { pid, started_at };
    let path = native_dsh_pid_path()?;
    ensure_safe_file_destination(&path, "服务进程记录")?;
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
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if is_reparse_point(&metadata) || !metadata.is_file() {
            return Err(format!("服务进程记录不是普通文件：{}", path.display()));
        }
    }
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
    if !native_dsh_process_matches(process)? {
        return Ok(false);
    }
    let Some(command_line) = process_command_line(process.pid)? else {
        return Ok(false);
    };
    Ok(is_verified_dsh_command(&command_line, DSH_PORT))
}

fn native_dsh_process_matches(process: NativeDshProcess) -> Result<bool, String> {
    Ok(native_process_started_at(process.pid)? == Some(process.started_at))
}

fn find_verified_dsh_pid(port: u16) -> Result<Option<u32>, String> {
    let tracked_process = read_native_dsh_process()?;
    let mut candidate_pids = listening_process_ids(port)?;
    if let Some(process) = tracked_process {
        if is_process_running(process.pid)? && !candidate_pids.contains(&process.pid) {
            candidate_pids.push(process.pid);
        }
    }

    for pid in candidate_pids {
        let Some(command_line) = process_command_line(pid)? else {
            continue;
        };
        if is_verified_dsh_command(&command_line, port) {
            return Ok(Some(pid));
        }
    }
    Ok(None)
}

fn verified_dsh_state(port: u16) -> Result<bool, String> {
    let verified = find_verified_dsh_pid(port)?;
    if verified.is_none() && is_port_listening(port) {
        return Err(format!(
            "端口 {port} 正被无法验证为 DSH 的进程占用；为避免修改正在使用的运行时，已停止操作"
        ));
    }
    Ok(verified.is_some())
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

fn terminate_native_dsh_process(pid: u32) -> Result<bool, String> {
    terminate_process_tree(pid, "服务进程")
}

fn terminate_process_direct(pid: u32, label: &str) -> Result<(), String> {
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
            0,
            pid,
        )
    };
    if handle.is_null() {
        return Err(format!("无法打开{label}的强制终止句柄：{}", unsafe {
            GetLastError()
        }));
    }
    let result = unsafe { TerminateProcess(handle, 1) };
    let error = if result == 0 {
        Some(unsafe { GetLastError() })
    } else {
        None
    };
    unsafe {
        CloseHandle(handle);
    }
    if let Some(error) = error {
        return Err(format!("直接终止{label}失败：{error}"));
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if !is_process_running(pid)? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }
    Err(format!("{label}已发送强制终止请求，但进程仍在运行"))
}

fn terminate_process_tree(pid: u32, label: &str) -> Result<bool, String> {
    let pid_text = pid.to_string();
    let graceful = hidden_command("taskkill.exe")
        .args(["/PID", &pid_text, "/T"])
        .output()
        .map_err(|error| format!("无法向{label}发送优雅退出信号：{error}"))?;
    if graceful.status.success() {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if !is_process_running(pid)? {
                record_process_termination(label, pid, false);
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(100));
        }
    } else if !is_process_running(pid)? {
        record_process_termination(label, pid, false);
        return Ok(false);
    }

    let forced = hidden_command("taskkill.exe")
        .args(["/PID", &pid_text, "/T", "/F"])
        .output()
        .map_err(|error| format!("无法强制关闭{label}：{error}"))?;
    if forced.status.success() {
        record_process_termination(label, pid, true);
        Ok(true)
    } else {
        let stderr = String::from_utf8_lossy(&forced.stderr);
        let stdout = String::from_utf8_lossy(&forced.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        match terminate_process_direct(pid, label) {
            Ok(()) => {
                record_process_termination(label, pid, true);
                Ok(true)
            }
            Err(direct_error) => Err(format!(
                "关闭{label}失败：{}；直接终止兜底失败：{direct_error}",
                truncate(detail, 300)
            )),
        }
    }
}

fn record_process_termination(label: &str, pid: u32, forced: bool) {
    let Ok(path) = native_dsh_log_path("process-supervision.log") else {
        return;
    };
    if ensure_safe_file_destination(&path, "进程监督日志").is_err() {
        return;
    }
    let _ = rotate_log_file(&path);
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let _ = writeln!(
        file,
        "timestamp={timestamp} pid={pid} label={label} forced={forced}"
    );
}

fn run_native_command(command: &mut Command, description: &str) -> Result<String, String> {
    let output = command
        .output()
        .map_err(|error| format!("{description}失败：{error}"))?;
    command_output_result(output, description)
}

fn run_native_update_command(command: &mut Command, description: &str) -> Result<String, String> {
    run_native_update_command_with_timeout(command, description, DSH_PACKAGE_COMMAND_TIMEOUT)
}

fn run_native_update_command_with_timeout(
    command: &mut Command,
    description: &str,
    timeout: Duration,
) -> Result<String, String> {
    let capture_root = update_command_capture_root()?;
    ensure_safe_directory(&capture_root)
        .map_err(|error| format!("无法创建{description}输出暂存目录：{error}"))?;
    let capture_stem = capture_root.join(format!(".dsh-launcher-command-{}", transaction_nonce()));
    let stdout_path = capture_stem.with_extension("stdout");
    let stderr_path = capture_stem.with_extension("stderr");
    let stdout_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stdout_path)
        .map_err(|error| format!("无法创建{description}标准输出暂存文件：{error}"))?;
    let stderr_file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stderr_path)
    {
        Ok(file) => file,
        Err(error) => {
            let _ = fs::remove_file(&stdout_path);
            return Err(format!("无法创建{description}标准错误暂存文件：{error}"));
        }
    };
    command
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));
    let mut child = command.spawn().map_err(|error| {
        let _ = fs::remove_file(&stdout_path);
        let _ = fs::remove_file(&stderr_path);
        format!("{description}失败：{error}")
    })?;
    let process_id = child.id();

    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let mut cancelled = false;
    let mut termination_error = None;
    let mut status = None;
    loop {
        match child.try_wait() {
            Ok(Some(value)) => {
                status = Some(value);
                break;
            }
            Ok(None) => {}
            Err(error) => {
                termination_error = Some(format!("无法读取{description}进程状态：{error}"));
                break;
            }
        }
        if CANCEL_REQUESTED.load(Ordering::Acquire) {
            cancelled = true;
            if let Err(error) = terminate_process_tree(process_id, description) {
                termination_error = Some(error);
            }
            break;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            if let Err(error) = terminate_process_tree(process_id, description) {
                termination_error = Some(error);
            }
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }

    if status.is_none() {
        let wait_deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < wait_deadline {
            match child.try_wait() {
                Ok(Some(value)) => {
                    status = Some(value);
                    break;
                }
                Ok(None) => thread::sleep(Duration::from_millis(100)),
                Err(error) => {
                    termination_error.get_or_insert_with(|| {
                        format!("无法读取{description}进程终止状态：{error}")
                    });
                    break;
                }
            }
        }
    }
    if status.is_none() {
        if let Err(error) = child.kill() {
            termination_error
                .get_or_insert_with(|| format!("无法结束{description}子进程：{error}"));
        }
        status = Some(
            child
                .wait()
                .map_err(|error| format!("读取{description}结果失败：{error}"))?,
        );
    }

    let stdout = fs::read(&stdout_path)
        .map_err(|error| format!("读取{description}标准输出失败：{error}"))?;
    let stderr = fs::read(&stderr_path)
        .map_err(|error| format!("读取{description}标准错误失败：{error}"))?;
    let stdout_cleanup = fs::remove_file(&stdout_path);
    let stderr_cleanup = fs::remove_file(&stderr_path);
    if let Err(error) = stdout_cleanup {
        return Err(format!(
            "{description}完成但清理标准输出暂存文件失败：{error}"
        ));
    }
    if let Err(error) = stderr_cleanup {
        return Err(format!(
            "{description}完成但清理标准错误暂存文件失败：{error}"
        ));
    }

    let termination_detail = termination_error
        .map(|error| format!("；进程清理失败：{error}"))
        .unwrap_or_default();
    if timed_out {
        return Err(format!(
            "{description}超过 {} 秒未完成，已停止{termination_detail}。请检查网络或代理后重试。",
            timeout.as_secs()
        ));
    }
    if cancelled {
        return Err(format!("{description}已取消{termination_detail}"));
    }
    if let Some(error) = termination_detail.strip_prefix("；") {
        return Err(format!("{description}失败：{error}"));
    }
    let status = status.expect("child status must be available after wait");
    if status.success() {
        Ok(String::from_utf8_lossy(&stdout).trim().to_owned())
    } else {
        let stderr = String::from_utf8_lossy(&stderr);
        let stdout = String::from_utf8_lossy(&stdout);
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        if detail.is_empty() {
            Err(format!("{description}失败，退出代码：{status}"))
        } else {
            Err(format!("{description}失败：{}", truncate(detail, 500)))
        }
    }
}

fn update_command_capture_root() -> Result<PathBuf, String> {
    match app_paths() {
        Ok(paths) => Ok(paths.temp_root),
        Err(error) => {
            #[cfg(test)]
            {
                let _ = error;
                Ok(env::temp_dir())
            }
            #[cfg(not(test))]
            {
                Err(error)
            }
        }
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

struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

fn http_get(port: u16, path: &str) -> Result<HttpResponse, String> {
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
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| format!("无法读取 DSH 健康检查响应：{error}"))?;
    if response.len() > MAX_HEALTH_RESPONSE_BYTES {
        return Err("DSH 健康检查响应超过 8 MiB，已拒绝读取".to_owned());
    }
    let separator = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| "DSH 健康检查未返回完整 HTTP 响应".to_owned())?;
    let header_text = String::from_utf8_lossy(&response[..separator]);
    let status_line = header_text
        .lines()
        .next()
        .ok_or_else(|| "DSH 健康检查未返回 HTTP 状态行".to_owned())?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| format!("无法解析 DSH 健康检查状态：{status_line}"))?
        .parse::<u16>()
        .map_err(|error| format!("无法解析 DSH 健康检查状态：{error}"))?;
    let encoded_body = &response[separator + 4..];
    let chunked = header_text.lines().any(|line| {
        let Some((name, value)) = line.split_once(':') else {
            return false;
        };
        name.trim().eq_ignore_ascii_case("transfer-encoding")
            && value
                .split(',')
                .any(|encoding| encoding.trim().eq_ignore_ascii_case("chunked"))
    });
    let body = if chunked {
        decode_chunked_body(encoded_body)?
    } else {
        encoded_body.to_vec()
    };
    Ok(HttpResponse { status, body })
}

fn decode_chunked_body(encoded: &[u8]) -> Result<Vec<u8>, String> {
    let mut cursor = 0usize;
    let mut body = Vec::new();
    loop {
        let line_end = encoded[cursor..]
            .windows(2)
            .position(|window| window == b"\r\n")
            .map(|offset| cursor + offset)
            .ok_or_else(|| "DSH 健康检查的 chunked 响应不完整".to_owned())?;
        let size_text = std::str::from_utf8(&encoded[cursor..line_end])
            .map_err(|error| format!("DSH 健康检查 chunk 大小无效：{error}"))?;
        let size_text = size_text.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|error| format!("DSH 健康检查 chunk 大小无效：{error}"))?;
        cursor = line_end + 2;
        if size == 0 {
            return Ok(body);
        }
        let remaining = encoded.len().saturating_sub(cursor);
        if size > MAX_HEALTH_RESPONSE_BYTES.saturating_sub(body.len())
            || remaining < size
            || remaining.saturating_sub(size) < 2
        {
            return Err("DSH 健康检查的 chunked 响应超过限制或不完整".to_owned());
        }
        body.extend_from_slice(&encoded[cursor..cursor + size]);
        cursor += size;
        if &encoded[cursor..cursor + 2] != b"\r\n" {
            return Err("DSH 健康检查 chunk 缺少结束符".to_owned());
        }
        cursor += 2;
    }
}

fn http_status(port: u16, path: &str) -> Result<u16, String> {
    Ok(http_get(port, path)?.status)
}

fn http_json_status(port: u16, path: &str) -> Result<(u16, serde_json::Value), String> {
    let response = http_get(port, path)?;
    let value = serde_json::from_slice(&response.body)
        .map_err(|error| format!("DSH {path} 返回的 JSON 结构无效：{error}"))?;
    Ok((response.status, value))
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

fn dsh_web_health_status() -> String {
    match find_verified_dsh_pid(DSH_PORT) {
        Ok(Some(_)) => match http_status(DSH_PORT, "/") {
            Ok(status) if is_successful_http_status(status) => {
                "服务运行中 · http://127.0.0.1:3080".to_owned()
            }
            Ok(status) => format!("服务响应异常 · HTTP {status}"),
            Err(_) => "服务进程无响应".to_owned(),
        },
        Ok(None) if is_port_listening(DSH_PORT) => "端口 3080 被其他服务占用".to_owned(),
        Ok(None) => "服务未启动".to_owned(),
        Err(_) => "服务状态未知 · 无法验证进程".to_owned(),
    }
}

fn tray_icon_running_state(status: &str) -> Option<bool> {
    if status.starts_with("服务运行中")
        || status.starts_with("服务已启动")
        || status.starts_with("服务已在运行")
    {
        Some(true)
    } else if status.starts_with("服务响应异常")
        || status.starts_with("服务进程无响应")
        || status.starts_with("端口 3080 被其他服务占用")
        || status.starts_with("服务未启动")
        || status.starts_with("服务已停止")
        || status.starts_with("服务当前未运行")
        || status.starts_with("服务状态未知")
        || status.starts_with("运行时需要修复")
        || status.starts_with("DSH 需要修复")
    {
        Some(false)
    } else {
        if status.contains("失败")
            || status.contains("错误")
            || status.contains("拒绝")
            || status.contains("无效")
        {
            Some(false)
        } else {
            None
        }
    }
}

fn open_web_ui() -> Result<String, String> {
    if find_verified_dsh_pid(DSH_PORT)?.is_none() {
        return Err("无法验证端口 3080 上运行的是 DSH，已拒绝打开未知服务".to_owned());
    }
    let status = http_status(DSH_PORT, "/")?;
    if !is_successful_http_status(status) {
        return Err(format!("服务页面当前不可用（HTTP {status}）"));
    }
    spawn_explorer_target(WEB_URL, "服务页面")?;
    Ok("已打开服务页面".to_owned())
}

fn open_data_directory() -> Result<String, String> {
    let path = app_paths()?.data_root;
    let target = path.to_string_lossy().into_owned();
    spawn_explorer_target(&target, "数据目录")?;
    Ok(format!("已打开数据目录：{}", path.display()))
}

fn spawn_explorer_target(target: &str, label: &str) -> Result<(), String> {
    hidden_command("explorer.exe")
        .arg(target)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开{label}：{error}"))
}

fn diagnostic_report(state: &AppState) -> String {
    let mut lines = vec![format!("DSH启动器 v{APP_VERSION}"), "".to_owned()];
    if let Ok(status) = state.current_status.lock() {
        lines.push(format!("最近状态：{status}"));
    }
    if let Ok(messages) = state.messages.lock() {
        if !messages.is_empty() {
            lines.push("最近操作阶段：".to_owned());
            lines.extend(messages.iter().map(|message| format!("- {message}")));
        }
    }
    lines.push("".to_owned());
    match app_paths() {
        Ok(paths) => {
            lines.push(format!("数据目录：{}", paths.data_root.display()));
            lines.push(format!("运行时目录：{}", paths.runtime_root.display()));
            lines.push(format!("DSH 目录：{}", paths.npm_prefix.display()));
            lines.push(format!("日志目录：{}", paths.log_root.display()));
            match verify_runtime_integrity(&paths) {
                Ok(()) => lines.push("运行时完整性：通过".to_owned()),
                Err(error) => lines.push(format!("运行时完整性：失败：{error}")),
            }
            match native_dsh_version() {
                Ok(version) => lines.push(format!("DSH 版本：{version}")),
                Err(error) => lines.push(format!("DSH 版本：无法读取：{error}")),
            }
            lines.push(format!("服务状态：{}", dsh_web_health_status()));
            if let Ok(Some(process)) = read_native_dsh_process() {
                lines.push(format!("记录的服务 PID：{}", process.pid));
            }
            for (file, label) in [
                (DSH_STDOUT_LOG_FILE, "服务 stdout"),
                (DSH_STDERR_LOG_FILE, "服务 stderr"),
                (DSH_PREFLIGHT_STDOUT_LOG_FILE, "预检 stdout"),
                (DSH_PREFLIGHT_STDERR_LOG_FILE, "预检 stderr"),
            ] {
                let path = paths.log_root.join(file);
                lines.push(format!("{label}：{}", path.display()));
                if let Some(tail) = read_log_tail(&path, DSH_LOG_TAIL_BYTES) {
                    lines.push(format!("{label}末尾：{}", truncate(&tail, 900)));
                }
            }
            match legacy_cleanup_candidates(&paths) {
                Ok(candidates) if !candidates.is_empty() => {
                    lines.push("可移入回收站的旧数据：".to_owned());
                    for (candidate, size) in candidates {
                        lines.push(format!(
                            "{}：{}（{}）",
                            candidate.label,
                            candidate.source.display(),
                            format_bytes(size)
                        ));
                    }
                    lines.push(
                        "托盘菜单中的“移入旧数据回收站”仅在 DSH 已验证运行且二次确认后执行"
                            .to_owned(),
                    );
                }
                Ok(_) => {}
                Err(error) => lines.push(format!("旧数据清理检查失败：{error}")),
            }
            if is_system_drive(&paths.data_root) {
                lines.push("建议：停止服务后将整个便携目录移动到 D 盘".to_owned());
            }
        }
        Err(error) => lines.push(format!("数据目录：无法解析：{error}")),
    }
    lines.join("\n")
}

fn show_diagnostics_report(hwnd: HWND, report: &str) {
    let copy_result = copy_to_clipboard(hwnd, report);
    let message = match copy_result {
        Ok(()) => format!("{report}\n\n诊断详情已复制到剪贴板。"),
        Err(error) => format!("{report}\n\n复制失败：{error}"),
    };
    let title = to_wide("DSH 诊断详情");
    let message = to_wide(&message);
    unsafe {
        MessageBoxW(hwnd, message.as_ptr(), title.as_ptr(), MB_OK);
    }
}

fn request_diagnostics(hwnd: HWND, state: Arc<AppState>) {
    if state.diagnostics_running.swap(true, Ordering::AcqRel) {
        push_status(hwnd, &state, "诊断详情正在收集中".to_owned());
        return;
    }
    unsafe {
        set_button_enabled(hwnd, CMD_DETAILS, false);
    }
    push_status(hwnd, &state, "正在收集诊断详情...".to_owned());
    let hwnd_value = hwnd as usize;
    thread::spawn(move || {
        let report = diagnostic_report(&state);
        if let Ok(mut pending) = state.pending_diagnostic_report.lock() {
            *pending = Some(report);
        }
        unsafe {
            PostMessageW(hwnd_value as HWND, DIAGNOSTICS_READY_MESSAGE, 0, 0);
        }
    });
}

fn copy_to_clipboard(hwnd: HWND, text: &str) -> Result<(), String> {
    let wide = to_wide(text);
    let size = wide.len() * std::mem::size_of::<u16>();
    unsafe {
        if OpenClipboard(hwnd) == 0 {
            return Err(format!("无法打开剪贴板：{}", GetLastError()));
        }
        if EmptyClipboard() == 0 {
            CloseClipboard();
            return Err(format!("无法清空剪贴板：{}", GetLastError()));
        }
        let memory = GlobalAlloc(GMEM_MOVEABLE, size);
        if memory.is_null() {
            CloseClipboard();
            return Err(format!("无法分配剪贴板内存：{}", GetLastError()));
        }
        let destination = GlobalLock(memory).cast::<u16>();
        if destination.is_null() {
            GlobalFree(memory);
            CloseClipboard();
            return Err(format!("无法锁定剪贴板内存：{}", GetLastError()));
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), destination, wide.len());
        GlobalUnlock(memory);
        if SetClipboardData(
            windows_sys::Win32::System::Ole::CF_UNICODETEXT as u32,
            memory,
        )
        .is_null()
        {
            GlobalFree(memory);
            CloseClipboard();
            return Err(format!("无法写入剪贴板：{}", GetLastError()));
        }
        CloseClipboard();
        Ok(())
    }
}

fn hidden_command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW_FLAG);
    command.stdin(Stdio::null());
    command
}

fn high_contrast_enabled() -> bool {
    let mut settings = HIGHCONTRASTW {
        cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
        ..HIGHCONTRASTW::default()
    };
    unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            settings.cbSize,
            (&mut settings as *mut HIGHCONTRASTW).cast::<c_void>(),
            0,
        ) != 0
            && settings.dwFlags & HCF_HIGHCONTRASTON != 0
    }
}

fn dark_mode_enabled(high_contrast: bool) -> bool {
    if high_contrast {
        return false;
    }
    let color = unsafe { GetSysColor(COLOR_WINDOW) };
    let red = color & 0xff;
    let green = (color >> 8) & 0xff;
    let blue = (color >> 16) & 0xff;
    red + green + blue < 3 * 128
}

fn theme_color(state: &AppState, color: u32) -> u32 {
    if state.high_contrast.load(Ordering::Acquire) {
        return unsafe {
            match color {
                COLOR_TEXT | COLOR_BLUE | COLOR_CYAN | COLOR_PURPLE | COLOR_GREEN | COLOR_RED
                | COLOR_AMBER => GetSysColor(COLOR_WINDOWTEXT),
                COLOR_MUTED | COLOR_DISABLED => GetSysColor(COLOR_GRAYTEXT),
                COLOR_BORDER => GetSysColor(COLOR_WINDOWTEXT),
                COLOR_HIGHLIGHT | COLOR_SURFACE_PRESSED => GetSysColor(SYSTEM_COLOR_HIGHLIGHT),
                _ => GetSysColor(COLOR_WINDOW),
            }
        };
    }
    if !state.dark_mode.load(Ordering::Acquire) {
        return color;
    }
    match color {
        COLOR_BACKGROUND | COLOR_BACKGROUND_TOP | COLOR_BACKGROUND_BOTTOM => rgb(24, 28, 36),
        COLOR_SURFACE => rgb(35, 41, 52),
        COLOR_SURFACE_HOVER => rgb(48, 57, 70),
        COLOR_SURFACE_PRESSED => rgb(55, 78, 112),
        COLOR_BORDER => rgb(82, 96, 119),
        COLOR_SHADOW => rgb(12, 15, 21),
        COLOR_TEXT => rgb(235, 240, 249),
        COLOR_MUTED => rgb(174, 185, 202),
        COLOR_DISABLED => rgb(111, 123, 143),
        COLOR_GREEN => rgb(61, 205, 155),
        COLOR_RED => rgb(255, 111, 135),
        COLOR_BLUE => rgb(102, 157, 255),
        COLOR_CYAN => rgb(75, 207, 218),
        COLOR_PURPLE => rgb(187, 145, 255),
        COLOR_AMBER => rgb(255, 201, 91),
        _ => color,
    }
}

fn write_self_update_health(path: Option<&Path>) -> Result<(), String> {
    let Some(path) = path else {
        return Ok(());
    };
    let paths = app_paths()?;
    if !path_is_same_or_below(path, &paths.update_root)
        || same_windows_path(path, &paths.update_root)
    {
        return Err("启动器健康握手路径不在便携数据目录内".to_owned());
    }
    let partial = path.with_extension("handshake.partial");
    ensure_safe_file_destination(&partial, "启动器健康握手暂存文件")?;
    ensure_safe_file_destination(path, "启动器健康握手")?;
    fs::write(
        &partial,
        format!(
            "ready=true\nversion={APP_VERSION}\npid={}\n",
            std::process::id()
        ),
    )
    .map_err(|error| format!("无法写入启动器健康握手：{error}"))?;
    fs::rename(&partial, path).map_err(|error| format!("无法提交启动器健康握手：{error}"))
}

fn health_handshake_matches(contents: &str, expected_version: &str, expected_pid: u32) -> bool {
    let value = |key: &str| {
        contents.lines().find_map(|line| {
            let (name, value) = line.trim().split_once('=')?;
            (name == key).then_some(value)
        })
    };
    value("ready") == Some("true")
        && value("version") == Some(expected_version)
        && value("pid").and_then(|pid| pid.parse::<u32>().ok()) == Some(expected_pid)
}

fn run_app(health_path: Option<PathBuf>) -> Result<(), String> {
    let app_user_model_id = to_wide(APP_USER_MODEL_ID);
    let app_id_result =
        unsafe { SetCurrentProcessExplicitAppUserModelID(app_user_model_id.as_ptr()) };
    if app_id_result < 0 {
        return Err(format!(
            "设置 Windows 应用身份失败：0x{:08x}",
            app_id_result as u32
        ));
    }
    let paths = app_paths()?;
    let high_contrast = high_contrast_enabled();
    let dark_mode = dark_mode_enabled(high_contrast);
    let system_dpi = unsafe { GetDpiForSystem() }.max(96);
    let hinstance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let hicon = unsafe { load_app_icon(hinstance, ICON_RESOURCE_ID) };
    let black_hicon = unsafe { load_app_icon(hinstance, BLACK_ICON_RESOURCE_ID) };
    if hicon.is_null() || black_hicon.is_null() {
        return Err("无法加载应用图标".to_owned());
    }

    let background_brush = unsafe {
        CreateSolidBrush(if high_contrast {
            GetSysColor(COLOR_WINDOW)
        } else if dark_mode {
            rgb(24, 28, 36)
        } else {
            COLOR_BACKGROUND
        })
    };
    let title_font = unsafe { create_ui_font(scale_for_dpi(-27, system_dpi), FW_SEMIBOLD as i32) };
    let body_font = unsafe { create_ui_font(scale_for_dpi(-16, system_dpi), FW_NORMAL as i32) };
    let small_font = unsafe { create_ui_font(scale_for_dpi(-14, system_dpi), FW_NORMAL as i32) };
    let button_font = unsafe { create_ui_font(scale_for_dpi(-16, system_dpi), FW_SEMIBOLD as i32) };
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
    let window_title = to_wide(&format!("{WINDOW_TITLE} · v{APP_VERSION}"));
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

    let initial_status = match verify_runtime_integrity(&paths) {
        Ok(()) => dsh_web_health_status(),
        Err(error) => format!("运行时需要修复：{error}"),
    };
    let taskbar_created_message = unsafe {
        let message = to_wide("TaskbarCreated");
        RegisterWindowMessageW(message.as_ptr())
    };
    let initial_tray_icon = if tray_icon_running_state(&initial_status) == Some(true) {
        hicon
    } else {
        black_hicon
    };
    let shared = Arc::new(AppState {
        hicon: hicon as usize,
        black_hicon: black_hicon as usize,
        tray_hicon: AtomicUsize::new(initial_tray_icon as usize),
        taskbar_created_message,
        data_root: paths.data_root.display().to_string(),
        background_brush: background_brush as usize,
        title_font: AtomicUsize::new(title_font as usize),
        body_font: AtomicUsize::new(body_font as usize),
        small_font: AtomicUsize::new(small_font as usize),
        button_font: AtomicUsize::new(button_font as usize),
        status_hwnd: AtomicUsize::new(0),
        tray_added: AtomicBool::new(false),
        busy: AtomicBool::new(false),
        cancelable: AtomicBool::new(false),
        health_checking: AtomicBool::new(false),
        diagnostics_running: AtomicBool::new(false),
        ui_page: AtomicUsize::new(UiPage::Home.as_atomic()),
        hover_levels: std::array::from_fn(|_| AtomicUsize::new(0)),
        messages: Mutex::new(VecDeque::new()),
        pending_diagnostic_report: Mutex::new(None),
        last_health: Mutex::new(initial_status.clone()),
        current_status: Mutex::new(initial_status.clone()),
        health_publish_after: Mutex::new(Instant::now()),
        high_contrast: AtomicBool::new(high_contrast),
        dark_mode: AtomicBool::new(dark_mode),
        close_notice_shown: AtomicBool::new(false),
    });
    let state_ptr = Box::into_raw(Box::new(Arc::clone(&shared)));
    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let window_width = scale_for_dpi(WINDOW_WIDTH, system_dpi);
    let window_height = scale_for_dpi(WINDOW_HEIGHT, system_dpi);
    let x = ((screen_width - window_width) / 2).max(0);
    let y = ((screen_height - window_height) / 2).max(0);
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_APPWINDOW | WS_EX_CONTROLPARENT,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            x,
            y,
            window_width,
            window_height,
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
        apply_window_theme(hwnd, &shared);
        SetWindowTextW(hwnd, window_title.as_ptr());
    }

    if let Err(error) = unsafe { create_controls(hwnd, hinstance, &shared) } {
        unsafe {
            DestroyWindow(hwnd);
        }
        return Err(error);
    }
    unsafe {
        layout_controls(hwnd, GetDpiForWindow(hwnd).max(96));
        set_ui_page(hwnd, &shared, UiPage::Home, false);
    }
    unsafe {
        SetTimer(hwnd, HOVER_TIMER_ID, HOVER_TIMER_INTERVAL_MS, None);
        SetTimer(hwnd, HEALTH_TIMER_ID, HEALTH_TIMER_INTERVAL_MS, None);
    }

    let tray_result = unsafe {
        add_tray_icon(
            hwnd,
            shared.tray_hicon.load(Ordering::Acquire) as HICON,
            "DSH启动器 · 就绪",
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
        set_window_icon(hwnd, initial_tray_icon);
        refresh_action_buttons(hwnd, &shared);
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        write_self_update_health(health_path.as_deref())?;
    }
    maybe_offer_legacy_migration(hwnd, Arc::clone(&shared));

    let cancel_accelerator = ACCEL {
        fVirt: FVIRTKEY,
        key: VK_ESCAPE,
        cmd: CMD_CANCEL as u16,
    };
    let accelerators = unsafe { CreateAcceleratorTableW(&cancel_accelerator, 1) };
    if accelerators.is_null() {
        unsafe {
            DestroyWindow(hwnd);
        }
        return Err("创建键盘快捷键失败".to_owned());
    }

    let mut message = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
        if result <= 0 {
            break;
        }
        unsafe {
            if message.message == WM_KEYDOWN && message.wParam == VK_RETURN as usize {
                let focused = GetFocus();
                if !focused.is_null() && GetParent(focused) == hwnd && IsWindowEnabled(focused) != 0
                {
                    SendMessageW(focused, BM_CLICK, 0, 0);
                    continue;
                }
            }
            if TranslateAcceleratorW(hwnd, accelerators, &message) == 0
                && IsDialogMessageW(hwnd, &message) == 0
            {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
    unsafe {
        DestroyAcceleratorTable(accelerators);
    }
    Ok(())
}

unsafe fn apply_window_theme(hwnd: HWND, state: &AppState) {
    let dark_mode: i32 = if state.dark_mode.load(Ordering::Acquire) {
        1
    } else {
        0
    };
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
        (
            DWMWA_CAPTION_COLOR,
            theme_color(state, COLOR_BACKGROUND_TOP),
        ),
        (DWMWA_BORDER_COLOR, theme_color(state, COLOR_BORDER)),
        (DWMWA_TEXT_COLOR, theme_color(state, COLOR_TEXT)),
    ] {
        let _ = DwmSetWindowAttribute(
            hwnd,
            attribute as u32,
            (&color as *const u32).cast::<c_void>(),
            std::mem::size_of::<u32>() as u32,
        );
    }
}

fn show_error_box(error: &str) {
    let title = to_wide("DSH启动器错误");
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
    LoadIconW(hinstance, resource_id as *const u16)
}

unsafe fn create_ui_font(height: i32, weight: i32) -> windows_sys::Win32::Graphics::Gdi::HFONT {
    let face_name = to_wide(UI_FONT_FAMILY);
    CreateFontW(
        height,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        GB2312_CHARSET as u32,
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
    let system_drive_warning = is_system_drive(Path::new(&state.data_root));
    let footer = if system_drive_warning {
        "⚠ 数据位于系统盘 · 停止服务后可整体移动到 D 盘".to_owned()
    } else {
        format!("v{APP_VERSION} · 关闭窗口后继续在托盘运行")
    };
    let footer_style = WS_CHILD
        | WS_VISIBLE
        | if system_drive_warning {
            0
        } else {
            SS_CENTERIMAGE
        };
    let controls = [
        create_control(
            hwnd,
            hinstance,
            "STATIC",
            WINDOW_TITLE,
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
            "正在检测 DSH 状态...",
            WS_CHILD | WS_VISIBLE | SS_OWNERDRAW,
            32,
            76,
            540,
            62,
            ID_STATUS,
        ),
        create_control(
            hwnd,
            hinstance,
            "STATIC",
            "常用操作",
            WS_CHILD | WS_VISIBLE | SS_CENTERIMAGE,
            32,
            158,
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
            192,
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
            192,
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
            192,
            262,
            58,
            CMD_RESTART,
        ),
        create_control(
            hwnd,
            hinstance,
            "BUTTON",
            "更新 DSH",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BUTTON_STYLE,
            310,
            336,
            262,
            58,
            CMD_UPGRADE,
        ),
        create_control(
            hwnd,
            hinstance,
            "BUTTON",
            "打开 Web UI",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BUTTON_STYLE,
            32,
            264,
            540,
            58,
            CMD_OPEN_WEB,
        ),
        create_control(
            hwnd,
            hinstance,
            "BUTTON",
            "打开数据目录",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BUTTON_STYLE,
            32,
            406,
            182,
            48,
            CMD_OPEN_DATA,
        ),
        create_control(
            hwnd,
            hinstance,
            "BUTTON",
            "验证并修复",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BUTTON_STYLE,
            224,
            406,
            170,
            48,
            CMD_REPAIR,
        ),
        create_control(
            hwnd,
            hinstance,
            "BUTTON",
            "更新启动器",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BUTTON_STYLE,
            404,
            406,
            168,
            48,
            CMD_LAUNCHER_UPDATE,
        ),
        create_control(
            hwnd,
            hinstance,
            "BUTTON",
            "更多工具",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BUTTON_STYLE,
            310,
            336,
            262,
            58,
            CMD_MORE_TOOLS,
        ),
        create_control(
            hwnd,
            hinstance,
            "BUTTON",
            "返回首页",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BUTTON_STYLE,
            376,
            414,
            96,
            34,
            CMD_HOME,
        ),
        create_control(
            hwnd,
            hinstance,
            "STATIC",
            &footer,
            footer_style,
            32,
            414,
            334,
            34,
            ID_FOOTER,
        ),
        create_control(
            hwnd,
            hinstance,
            "BUTTON",
            "诊断详情",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BUTTON_STYLE,
            32,
            336,
            540,
            58,
            CMD_DETAILS,
        ),
        create_control(
            hwnd,
            hinstance,
            "BUTTON",
            "取消操作",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BUTTON_STYLE,
            376,
            414,
            96,
            34,
            CMD_CANCEL,
        ),
        create_control(
            hwnd,
            hinstance,
            "BUTTON",
            "退出程序",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | BUTTON_STYLE,
            480,
            414,
            92,
            34,
            CMD_EXIT,
        ),
    ];

    if controls.iter().any(|control| control.is_null()) {
        return Err("创建窗口控件失败".to_owned());
    }
    configure_button_tab_order(hwnd);
    set_control_fonts(hwnd, state);
    state
        .status_hwnd
        .store(controls[1] as usize, Ordering::Release);
    Ok(())
}

unsafe fn configure_button_tab_order(hwnd: HWND) {
    let mut previous = GetDlgItem(hwnd, ID_SECTION as i32);
    for id in [
        CMD_START,
        CMD_STOP,
        CMD_OPEN_WEB,
        CMD_UPGRADE,
        CMD_MORE_TOOLS,
        CMD_RESTART,
        CMD_REPAIR,
        CMD_OPEN_DATA,
        CMD_LAUNCHER_UPDATE,
        CMD_DETAILS,
        CMD_HOME,
        CMD_CANCEL,
        CMD_EXIT,
    ] {
        let control = GetDlgItem(hwnd, id as i32);
        if !control.is_null() {
            SetWindowPos(
                control,
                previous,
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
            );
            previous = control;
        }
    }
}

unsafe fn set_control_fonts(hwnd: HWND, state: &AppState) {
    let title = state.title_font.load(Ordering::Acquire);
    let body = state.body_font.load(Ordering::Acquire);
    let small = state.small_font.load(Ordering::Acquire);
    let button = state.button_font.load(Ordering::Acquire);
    let title_control = GetDlgItem(hwnd, ID_TITLE as i32);
    if !title_control.is_null() {
        SendMessageW(title_control, WM_SETFONT, title, 1);
    }
    let status_control = GetDlgItem(hwnd, ID_STATUS as i32);
    if !status_control.is_null() {
        SendMessageW(status_control, WM_SETFONT, body, 1);
    }
    for id in [
        ID_SECTION,
        CMD_START,
        CMD_STOP,
        CMD_RESTART,
        CMD_UPGRADE,
        CMD_OPEN_WEB,
        CMD_OPEN_DATA,
        CMD_REPAIR,
        CMD_LAUNCHER_UPDATE,
        CMD_MORE_TOOLS,
        CMD_DETAILS,
    ] {
        let control = GetDlgItem(hwnd, id as i32);
        if !control.is_null() {
            SendMessageW(control, WM_SETFONT, button, 1);
        }
    }
    for id in [ID_FOOTER, CMD_HOME, CMD_CANCEL, CMD_EXIT] {
        let control = GetDlgItem(hwnd, id as i32);
        if !control.is_null() {
            SendMessageW(control, WM_SETFONT, small, 1);
        }
    }
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

fn scale_for_dpi(value: i32, dpi: u32) -> i32 {
    let scaled = i64::from(value) * i64::from(dpi);
    if scaled >= 0 {
        ((scaled + 48) / 96) as i32
    } else {
        -(((-scaled + 48) / 96) as i32)
    }
}

unsafe fn layout_controls(hwnd: HWND, dpi: u32) {
    let controls = [
        (ID_TITLE, 78, 18, 494, 36),
        (ID_STATUS, 32, 76, 540, 62),
        (ID_SECTION, 32, 158, 540, 24),
        (CMD_START, 32, 192, 262, 58),
        (CMD_STOP, 310, 192, 262, 58),
        (CMD_OPEN_WEB, 32, 264, 540, 58),
        (CMD_UPGRADE, 32, 336, 262, 58),
        (CMD_MORE_TOOLS, 310, 336, 262, 58),
        (CMD_RESTART, 32, 192, 262, 58),
        (CMD_REPAIR, 310, 192, 262, 58),
        (CMD_OPEN_DATA, 32, 264, 262, 58),
        (CMD_LAUNCHER_UPDATE, 310, 264, 262, 58),
        (CMD_DETAILS, 32, 336, 540, 58),
        (ID_FOOTER, 32, 414, 334, 34),
        (CMD_HOME, 376, 414, 96, 34),
        (CMD_CANCEL, 376, 414, 96, 34),
        (CMD_EXIT, 480, 414, 92, 34),
    ];
    for (id, x, y, width, height) in controls {
        let control = GetDlgItem(hwnd, id as i32);
        if !control.is_null() {
            SetWindowPos(
                control,
                std::ptr::null_mut(),
                scale_for_dpi(x, dpi),
                scale_for_dpi(y, dpi),
                scale_for_dpi(width, dpi),
                scale_for_dpi(height, dpi),
                SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
    }
}

const PAGE_BUTTONS: [u32; HOVER_BUTTON_COUNT] = [
    CMD_START,
    CMD_STOP,
    CMD_RESTART,
    CMD_UPGRADE,
    CMD_OPEN_WEB,
    CMD_OPEN_DATA,
    CMD_REPAIR,
    CMD_LAUNCHER_UPDATE,
    CMD_DETAILS,
    CMD_CANCEL,
    CMD_EXIT,
    CMD_MORE_TOOLS,
    CMD_HOME,
];

fn control_visible_on_page(page: UiPage, id: u32, busy: bool) -> bool {
    match id {
        CMD_START | CMD_STOP | CMD_UPGRADE | CMD_OPEN_WEB | CMD_MORE_TOOLS => page == UiPage::Home,
        CMD_RESTART | CMD_OPEN_DATA | CMD_REPAIR | CMD_LAUNCHER_UPDATE | CMD_DETAILS => {
            page == UiPage::Tools
        }
        CMD_HOME => page == UiPage::Tools && !busy,
        CMD_CANCEL => busy,
        CMD_EXIT => true,
        _ => true,
    }
}

unsafe fn set_control_visible(hwnd: HWND, id: u32, visible: bool) {
    let control = GetDlgItem(hwnd, id as i32);
    if !control.is_null() {
        ShowWindow(control, if visible { SW_SHOW } else { SW_HIDE });
    }
}

unsafe fn refresh_page_visibility(hwnd: HWND, state: &AppState) {
    let page = UiPage::from_atomic(state.ui_page.load(Ordering::Acquire));
    let busy = state.busy.load(Ordering::Acquire);
    for id in PAGE_BUTTONS {
        set_control_visible(hwnd, id, control_visible_on_page(page, id, busy));
    }
}

unsafe fn set_ui_page(hwnd: HWND, state: &AppState, page: UiPage, move_focus: bool) {
    state.ui_page.store(page.as_atomic(), Ordering::Release);
    let (section, focus_order): (&str, &[u32]) = match page {
        UiPage::Home => (
            "常用操作",
            &[
                CMD_START,
                CMD_STOP,
                CMD_OPEN_WEB,
                CMD_UPGRADE,
                CMD_MORE_TOOLS,
            ],
        ),
        UiPage::Tools => (
            "更多工具",
            &[
                CMD_RESTART,
                CMD_REPAIR,
                CMD_OPEN_DATA,
                CMD_LAUNCHER_UPDATE,
                CMD_DETAILS,
                CMD_HOME,
            ],
        ),
    };
    let section_control = GetDlgItem(hwnd, ID_SECTION as i32);
    if !section_control.is_null() {
        let text = to_wide(section);
        SetWindowTextW(section_control, text.as_ptr());
        NotifyWinEvent(EVENT_OBJECT_NAMECHANGE, section_control, OBJID_CLIENT, 0);
    }
    refresh_page_visibility(hwnd, state);
    if move_focus {
        for id in focus_order {
            let control = GetDlgItem(hwnd, *id as i32);
            if !control.is_null() && IsWindowEnabled(control) != 0 {
                SetFocus(control);
                break;
            }
        }
    }
    InvalidateRect(hwnd, std::ptr::null(), 1);
}

unsafe fn recreate_ui_fonts(hwnd: HWND, state: &AppState, dpi: u32) {
    let title = create_ui_font(scale_for_dpi(-27, dpi), FW_SEMIBOLD as i32);
    let body = create_ui_font(scale_for_dpi(-16, dpi), FW_NORMAL as i32);
    let small = create_ui_font(scale_for_dpi(-14, dpi), FW_NORMAL as i32);
    let button = create_ui_font(scale_for_dpi(-16, dpi), FW_SEMIBOLD as i32);
    if [title, body, small, button]
        .iter()
        .any(|font| font.is_null())
    {
        for font in [title, body, small, button] {
            if !font.is_null() {
                DeleteObject(font);
            }
        }
        return;
    }
    let old = [
        state.title_font.swap(title as usize, Ordering::AcqRel),
        state.body_font.swap(body as usize, Ordering::AcqRel),
        state.small_font.swap(small as usize, Ordering::AcqRel),
        state.button_font.swap(button as usize, Ordering::AcqRel),
    ];
    set_control_fonts(hwnd, state);
    layout_controls(hwnd, dpi);
    for font in old {
        if font != 0 {
            DeleteObject(font as *mut c_void);
        }
    }
}

unsafe fn set_status_text(state: &AppState, message: &str) {
    let status_hwnd = state.status_hwnd.load(Ordering::Acquire) as HWND;
    if !status_hwnd.is_null() {
        if let Ok(mut current_status) = state.current_status.lock() {
            *current_status = message.to_owned();
        }
        let display = if message.chars().count() > 64 {
            format!("{}…", truncate(message, 63))
        } else {
            message.to_owned()
        };
        let display = to_wide(&display);
        SetWindowTextW(status_hwnd, display.as_ptr());
        NotifyWinEvent(EVENT_OBJECT_NAMECHANGE, status_hwnd, OBJID_CLIENT, 0);
        InvalidateRect(status_hwnd, std::ptr::null(), 1);
    }
}

unsafe fn set_window_icon(hwnd: HWND, hicon: HICON) {
    SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, hicon as isize);
    SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, hicon as isize);
    SendMessageW(hwnd, WM_SETICON, ICON_SMALL2 as usize, hicon as isize);
}

unsafe fn set_button_enabled(hwnd: HWND, id: u32, enabled: bool) {
    let control = GetDlgItem(hwnd, id as i32);
    if !control.is_null() {
        EnableWindow(control, enabled as i32);
        InvalidateRect(control, std::ptr::null(), 1);
    }
}

unsafe fn refresh_action_buttons(hwnd: HWND, state: &AppState) {
    if state.busy.load(Ordering::Acquire) {
        for id in [
            CMD_START,
            CMD_STOP,
            CMD_RESTART,
            CMD_UPGRADE,
            CMD_OPEN_WEB,
            CMD_OPEN_DATA,
            CMD_REPAIR,
            CMD_LAUNCHER_UPDATE,
            CMD_EXIT,
        ] {
            set_button_enabled(hwnd, id, false);
        }
        set_button_enabled(hwnd, CMD_DETAILS, true);
        set_button_enabled(hwnd, CMD_CANCEL, state.cancelable.load(Ordering::Acquire));
        refresh_page_visibility(hwnd, state);
        return;
    }
    let status = state
        .current_status
        .lock()
        .map(|value| value.clone())
        .unwrap_or_default();
    let healthy = tray_icon_running_state(&status) == Some(true);
    let abnormal = status_is_abnormal(&status);
    set_button_enabled(hwnd, CMD_START, !healthy && !abnormal);
    set_button_enabled(hwnd, CMD_STOP, healthy || abnormal);
    set_button_enabled(hwnd, CMD_RESTART, healthy || abnormal);
    set_button_enabled(hwnd, CMD_OPEN_WEB, healthy);
    for id in [
        CMD_UPGRADE,
        CMD_OPEN_DATA,
        CMD_REPAIR,
        CMD_LAUNCHER_UPDATE,
        CMD_DETAILS,
        CMD_EXIT,
        CMD_MORE_TOOLS,
        CMD_HOME,
    ] {
        set_button_enabled(hwnd, id, true);
    }
    set_button_enabled(hwnd, CMD_CANCEL, false);
    refresh_page_visibility(hwnd, state);
}

unsafe fn paint_window(hwnd: HWND, state: &AppState) {
    let mut paint = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut paint);
    if hdc.is_null() {
        return;
    }

    let mut client = RECT::default();
    GetClientRect(hwnd, &mut client);
    fill_gradient(
        hdc,
        client,
        theme_color(state, COLOR_BACKGROUND_TOP),
        theme_color(state, COLOR_BACKGROUND_BOTTOM),
    );
    let dpi = GetDpiForWindow(hwnd).max(96);
    let scale = |value| scale_for_dpi(value, dpi);

    draw_ellipse(
        hdc,
        RECT {
            left: scale(-110),
            top: scale(-130),
            right: scale(220),
            bottom: scale(170),
        },
        if state.dark_mode.load(Ordering::Acquire) {
            rgb(31, 50, 78)
        } else {
            rgb(218, 236, 255)
        },
    );
    draw_ellipse(
        hdc,
        RECT {
            left: scale(440),
            top: scale(-100),
            right: scale(720),
            bottom: scale(160),
        },
        if state.dark_mode.load(Ordering::Acquire) {
            rgb(57, 42, 78)
        } else {
            rgb(237, 228, 255)
        },
    );

    draw_round_rect(
        hdc,
        RECT {
            left: scale(33),
            top: scale(25),
            right: scale(67),
            bottom: scale(67),
        },
        theme_color(state, rgb(154, 184, 220)),
        theme_color(state, rgb(154, 184, 220)),
        scale(11),
    );
    draw_round_rect(
        hdc,
        RECT {
            left: scale(32),
            top: scale(22),
            right: scale(64),
            bottom: scale(64),
        },
        theme_color(state, COLOR_SURFACE),
        theme_color(state, COLOR_BORDER),
        scale(11),
    );
    let icon = state.tray_hicon.load(Ordering::Acquire) as HICON;
    if !icon.is_null() {
        DrawIconEx(
            hdc,
            scale(32),
            scale(22),
            icon,
            scale(32),
            scale(32),
            0,
            std::ptr::null_mut(),
            DI_NORMAL,
        );
    }

    let divider = RECT {
        left: scale(32),
        top: scale(420),
        right: scale(572),
        bottom: scale(421),
    };
    fill_solid_rect(hdc, divider, theme_color(state, rgb(207, 222, 240)));
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
    let dpi = GetDpiForWindow(GetParent(item.hwndItem)).max(96);
    let scale = |value: i32| scale_for_dpi(value, dpi);
    let shadow = RECT {
        left: item.rcItem.left + scale(2),
        top: item.rcItem.top + scale(4),
        right: item.rcItem.right - scale(1),
        bottom: item.rcItem.bottom - scale(1),
    };
    draw_round_rect(
        item.hDC,
        shadow,
        theme_color(state, COLOR_SHADOW),
        theme_color(state, COLOR_SHADOW),
        scale(14),
    );
    let card = RECT {
        left: item.rcItem.left + scale(1),
        top: item.rcItem.top + scale(1),
        right: item.rcItem.right - scale(1),
        bottom: item.rcItem.bottom - scale(4),
    };
    draw_round_rect(
        item.hDC,
        card,
        theme_color(state, COLOR_SURFACE),
        theme_color(state, COLOR_HIGHLIGHT),
        scale(14),
    );
    draw_round_rect(
        item.hDC,
        inset_rect(card, scale(1)),
        theme_color(state, COLOR_SURFACE),
        theme_color(state, COLOR_BORDER),
        scale(13),
    );
    let text = read_control_text(item.hwndItem);
    let accent = theme_color(state, status_accent(&text));

    let center_y = (card.top + card.bottom) / 2;
    let old_brush = SelectObject(item.hDC, GetStockObject(DC_BRUSH));
    let old_pen = SelectObject(item.hDC, GetStockObject(DC_PEN));
    SetDCBrushColor(item.hDC, accent);
    SetDCPenColor(item.hDC, accent);
    Ellipse(
        item.hDC,
        card.left + scale(20),
        center_y - scale(6),
        card.left + scale(32),
        center_y + scale(6),
    );
    SelectObject(item.hDC, old_pen);
    SelectObject(item.hDC, old_brush);

    draw_text(
        item.hDC,
        state.small_font.load(Ordering::Acquire),
        theme_color(state, COLOR_MUTED),
        "服务状态",
        RECT {
            left: card.left + scale(48),
            top: card.top + scale(7),
            right: card.right - scale(18),
            bottom: card.top + scale(27),
        },
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
    draw_text(
        item.hDC,
        state.body_font.load(Ordering::Acquire),
        theme_color(state, COLOR_TEXT),
        &text,
        RECT {
            left: card.left + scale(48),
            top: card.top + scale(27),
            right: card.right - scale(18),
            bottom: card.bottom - scale(5),
        },
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
}

unsafe fn draw_button(state: &AppState, item: &DRAWITEMSTRUCT) {
    let dpi = GetDpiForWindow(GetParent(item.hwndItem)).max(96);
    let scale = |value: i32| scale_for_dpi(value, dpi);
    let disabled = item.itemState & ODS_DISABLED != 0;
    let selected = item.itemState & ODS_SELECTED != 0;
    let focused = item.itemState & ODS_FOCUS != 0;
    let (title, detail, accent, glyph) = button_spec(item.CtlID);
    let accent = theme_color(state, accent);
    let hover_level = button_index(item.CtlID)
        .map(|index| state.hover_levels[index].load(Ordering::Acquire))
        .unwrap_or(0);
    let surface = if disabled {
        theme_color(state, rgb(239, 244, 250))
    } else if selected {
        theme_color(state, COLOR_SURFACE_PRESSED)
    } else {
        blend_color(
            theme_color(state, COLOR_SURFACE),
            theme_color(state, COLOR_SURFACE_HOVER),
            hover_level,
            HOVER_STEPS,
        )
    };
    let border = if disabled {
        theme_color(state, COLOR_BORDER)
    } else if focused {
        theme_color(state, COLOR_BLUE)
    } else {
        blend_color(
            theme_color(state, COLOR_BORDER),
            accent,
            hover_level,
            HOVER_STEPS,
        )
    };
    let lift = scale(((hover_level * 2) / HOVER_STEPS) as i32);
    let pressed_offset = if selected { scale(2) } else { 0 };
    let card = RECT {
        left: item.rcItem.left + scale(1),
        top: item.rcItem.top + scale(2) - lift + pressed_offset,
        right: item.rcItem.right - scale(1),
        bottom: item.rcItem.bottom - scale(2) - lift + pressed_offset,
    };
    let shadow = RECT {
        left: card.left + scale(1),
        top: card.top + scale(3),
        right: card.right + scale(1),
        bottom: card.bottom + scale(3),
    };
    draw_round_rect(
        item.hDC,
        shadow,
        blend_color(
            theme_color(state, rgb(220, 231, 244)),
            theme_color(state, COLOR_SHADOW),
            hover_level,
            HOVER_STEPS,
        ),
        blend_color(
            theme_color(state, rgb(220, 231, 244)),
            theme_color(state, COLOR_SHADOW),
            hover_level,
            HOVER_STEPS,
        ),
        scale(12),
    );
    draw_round_rect(item.hDC, card, surface, border, scale(12));
    draw_round_rect(
        item.hDC,
        inset_rect(card, scale(1)),
        surface,
        theme_color(state, COLOR_HIGHLIGHT),
        scale(11),
    );

    if matches!(item.CtlID, CMD_EXIT | CMD_HOME | CMD_CANCEL) {
        draw_text(
            item.hDC,
            state.small_font.load(Ordering::Acquire),
            if disabled {
                theme_color(state, COLOR_DISABLED)
            } else {
                theme_color(state, COLOR_TEXT)
            },
            title,
            card,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        return;
    }

    let color = if disabled {
        theme_color(state, COLOR_DISABLED)
    } else {
        accent
    };
    let icon_rect = RECT {
        left: card.left + scale(17),
        top: card.top + scale(12),
        right: card.left + scale(47),
        bottom: card.top + scale(42),
    };
    draw_ellipse(
        item.hDC,
        icon_rect,
        if disabled {
            theme_color(state, rgb(226, 233, 241))
        } else {
            theme_color(state, button_tint(item.CtlID))
        },
    );
    draw_text(
        item.hDC,
        state.body_font.load(Ordering::Acquire),
        color,
        glyph,
        icon_rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    draw_text(
        item.hDC,
        state.button_font.load(Ordering::Acquire),
        if disabled {
            theme_color(state, COLOR_DISABLED)
        } else {
            theme_color(state, COLOR_TEXT)
        },
        title,
        RECT {
            left: card.left + scale(59),
            top: card.top + scale(6),
            right: card.right - scale(12),
            bottom: card.top + scale(31),
        },
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
    draw_text(
        item.hDC,
        state.small_font.load(Ordering::Acquire),
        if disabled {
            theme_color(state, COLOR_DISABLED)
        } else {
            theme_color(state, COLOR_MUTED)
        },
        detail,
        RECT {
            left: card.left + scale(59),
            top: card.top + scale(29),
            right: card.right - scale(12),
            bottom: card.bottom - scale(4),
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
        DeleteObject(brush);
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
        CMD_UPGRADE => ("更新 DSH", "下载并安装新版本", COLOR_PURPLE, "↑"),
        CMD_OPEN_WEB => ("打开 Web UI", "在浏览器中打开 DSH", COLOR_BLUE, "↗"),
        CMD_OPEN_DATA => ("打开数据目录", "查看便携数据", COLOR_BLUE, "⌂"),
        CMD_REPAIR => ("验证并修复", "重新检查运行时", COLOR_AMBER, "✓"),
        CMD_LAUNCHER_UPDATE => ("更新启动器", "官方 Release", COLOR_PURPLE, "↑"),
        CMD_DETAILS => ("诊断详情", "查看错误、日志与修复建议", COLOR_CYAN, "i"),
        CMD_MORE_TOOLS => ("更多工具", "维护、诊断与启动器更新", COLOR_CYAN, "⋯"),
        CMD_HOME => ("返回首页", "", COLOR_BORDER, ""),
        CMD_CANCEL => ("取消操作", "", COLOR_BORDER, ""),
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
        CMD_OPEN_WEB => rgb(222, 239, 255),
        CMD_OPEN_DATA => rgb(222, 239, 255),
        CMD_REPAIR => rgb(255, 244, 216),
        CMD_LAUNCHER_UPDATE => rgb(239, 231, 255),
        CMD_DETAILS | CMD_MORE_TOOLS => rgb(220, 244, 248),
        _ => rgb(232, 238, 246),
    }
}

fn button_index(id: u32) -> Option<usize> {
    match id {
        CMD_START => Some(0),
        CMD_STOP => Some(1),
        CMD_RESTART => Some(2),
        CMD_UPGRADE => Some(3),
        CMD_OPEN_WEB => Some(4),
        CMD_OPEN_DATA => Some(5),
        CMD_REPAIR => Some(6),
        CMD_LAUNCHER_UPDATE => Some(7),
        CMD_DETAILS => Some(8),
        CMD_CANCEL => Some(9),
        CMD_EXIT => Some(10),
        CMD_MORE_TOOLS => Some(11),
        CMD_HOME => Some(12),
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

    for id in PAGE_BUTTONS {
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
    if text.contains("未启动")
        || text.contains("未运行")
        || text.contains("当前未运行")
        || text.contains("已停止")
    {
        COLOR_DISABLED
    } else if text.contains("失败")
        || text.contains("错误")
        || text.contains("无效")
        || text.contains("未找到")
        || text.contains("异常")
        || text.contains("占用")
        || text.contains("不可用")
        || text.contains("无响应")
        || text.contains("拒绝")
        || text.contains("需要修复")
    {
        COLOR_RED
    } else if text.contains("正在运行") {
        COLOR_GREEN
    } else if text.contains("正在") {
        COLOR_AMBER
    } else if text.contains("关闭") {
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

fn status_is_abnormal(status: &str) -> bool {
    status.contains("需要修复")
        || status.contains("缺少")
        || status.contains("不可用")
        || status.contains("无法")
        || status.contains("异常")
        || status.contains("无响应")
        || status.contains("未知")
        || status.contains("占用")
        || status.contains("错误")
        || status.contains("失败")
        || status.contains("拒绝")
        || status.contains("无效")
        || status.contains("超时")
        || status.contains("已取消")
}

unsafe fn show_main_window(hwnd: HWND) {
    SetTimer(hwnd, HOVER_TIMER_ID, HOVER_TIMER_INTERVAL_MS, None);
    ShowWindow(hwnd, SW_RESTORE);
    SetForegroundWindow(hwnd);
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    if let Some(state) = state_for(hwnd) {
        if state.taskbar_created_message != 0 && message == state.taskbar_created_message {
            state.tray_added.store(false, Ordering::Release);
            let tooltip = state
                .current_status
                .lock()
                .map(|status| status.clone())
                .unwrap_or_else(|_| "DSH启动器".to_owned());
            if add_tray_icon(
                hwnd,
                state.tray_hicon.load(Ordering::Acquire) as HICON,
                &tooltip,
            ) != 0
            {
                state.tray_added.store(true, Ordering::Release);
            } else {
                SetTimer(hwnd, TRAY_RETRY_TIMER_ID, TRAY_RETRY_INTERVAL_MS, None);
            }
            return 0;
        }
    }
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
            } else if command == CMD_CANCEL {
                if let Some(state) = state_for(hwnd) {
                    if state.busy.load(Ordering::Acquire) {
                        if state.cancelable.load(Ordering::Acquire) {
                            CANCEL_REQUESTED.store(true, Ordering::Release);
                            push_status(
                                hwnd,
                                &state,
                                "正在取消当前操作，请等待子进程退出...".to_owned(),
                            );
                        } else {
                            push_status(
                                hwnd,
                                &state,
                                "当前为不可取消的目录提交阶段，请等待完成".to_owned(),
                            );
                        }
                    }
                }
            } else if command == CMD_DETAILS {
                if let Some(state) = state_for(hwnd) {
                    request_diagnostics(hwnd, state);
                }
            } else if command == CMD_MORE_TOOLS {
                if let Some(state) = state_for(hwnd) {
                    set_ui_page(hwnd, &state, UiPage::Tools, true);
                }
            } else if command == CMD_HOME {
                if let Some(state) = state_for(hwnd) {
                    set_ui_page(hwnd, &state, UiPage::Home, true);
                }
            } else if command == CMD_CLEANUP_LEGACY {
                if let Some(state) = state_for(hwnd) {
                    request_legacy_cleanup(hwnd, state);
                }
            } else if command == CMD_MIGRATE_LEGACY {
                if let Some(state) = state_for(hwnd) {
                    request_legacy_migration(hwnd, state, false);
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
        WM_DPICHANGED => {
            let dpi = (wparam & 0xffff) as u32;
            if dpi != 0 {
                if let Some(state) = state_for(hwnd) {
                    recreate_ui_fonts(hwnd, &state, dpi);
                } else {
                    layout_controls(hwnd, dpi);
                }
            }
            let suggested = lparam as *const RECT;
            if !suggested.is_null() {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    (*suggested).left,
                    (*suggested).top,
                    (*suggested).right - (*suggested).left,
                    (*suggested).bottom - (*suggested).top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
            }
            InvalidateRect(hwnd, std::ptr::null(), 1);
            0
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            if let Some(state) = state_for(hwnd) {
                let high_contrast = high_contrast_enabled();
                let dark_mode = dark_mode_enabled(high_contrast);
                state.high_contrast.store(high_contrast, Ordering::Release);
                state.dark_mode.store(dark_mode, Ordering::Release);
                apply_window_theme(hwnd, &state);
                InvalidateRect(hwnd, std::ptr::null(), 1);
                for id in [
                    ID_TITLE,
                    ID_STATUS,
                    ID_SECTION,
                    ID_FOOTER,
                    CMD_START,
                    CMD_STOP,
                    CMD_RESTART,
                    CMD_UPGRADE,
                    CMD_OPEN_WEB,
                    CMD_OPEN_DATA,
                    CMD_REPAIR,
                    CMD_LAUNCHER_UPDATE,
                    CMD_DETAILS,
                    CMD_CANCEL,
                    CMD_EXIT,
                    CMD_MORE_TOOLS,
                    CMD_HOME,
                ] {
                    let control = GetDlgItem(hwnd, id as i32);
                    if !control.is_null() {
                        InvalidateRect(control, std::ptr::null(), 1);
                    }
                }
            }
            0
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
            if let Some(state) = state_for(hwnd) {
                let hdc = wparam as *mut c_void;
                let control = lparam as HWND;
                let id = GetDlgCtrlID(control) as u32;
                SetBkMode(hdc, TRANSPARENT as i32);
                let color = if id == ID_TITLE || id == ID_SECTION {
                    theme_color(&state, COLOR_TEXT)
                } else {
                    theme_color(&state, COLOR_MUTED)
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
                    .and_then(|mut messages| messages.pop_front());
                if let Some(message) = latest {
                    set_status_text(&state, &message);
                    if let Some(running) = tray_icon_running_state(&message) {
                        let icon = if running {
                            state.hicon
                        } else {
                            state.black_hicon
                        };
                        state.tray_hicon.store(icon, Ordering::Release);
                        set_window_icon(hwnd, icon as HICON);
                        let _ = update_tray_icon(hwnd, icon as HICON);
                        InvalidateRect(hwnd, std::ptr::null(), 1);
                    }
                    let _ = update_tray_tooltip(
                        hwnd,
                        state.tray_hicon.load(Ordering::Acquire) as HICON,
                        &message,
                    );
                    refresh_action_buttons(hwnd, &state);
                }
            }
            0
        }
        DIAGNOSTICS_READY_MESSAGE => {
            if let Some(state) = state_for(hwnd) {
                state.diagnostics_running.store(false, Ordering::Release);
                set_button_enabled(hwnd, CMD_DETAILS, true);
                let report = state
                    .pending_diagnostic_report
                    .lock()
                    .ok()
                    .and_then(|mut pending| pending.take());
                if let Some(report) = report {
                    show_diagnostics_report(hwnd, &report);
                } else {
                    push_status(hwnd, &state, "诊断详情生成失败".to_owned());
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
                        "DSH启动器",
                    );
                    if result != 0 {
                        state.tray_added.store(true, Ordering::Release);
                        KillTimer(hwnd, TRAY_RETRY_TIMER_ID);
                        let tooltip = state
                            .current_status
                            .lock()
                            .map(|status| status.clone())
                            .unwrap_or_else(|_| "DSH启动器".to_owned());
                        let _ = update_tray_tooltip(
                            hwnd,
                            state.tray_hicon.load(Ordering::Acquire) as HICON,
                            &tooltip,
                        );
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
        SELF_UPDATE_EXIT_MESSAGE => {
            DestroyWindow(hwnd);
            0
        }
        WM_CLOSE => {
            if let Some(state) = state_for(hwnd) {
                if state.tray_added.load(Ordering::Acquire) {
                    if !state.close_notice_shown.swap(true, Ordering::AcqRel) {
                        let text = to_wide(
                            "关闭窗口后，DSH启动器会继续驻留系统托盘。右键托盘图标可重新打开或退出。",
                        );
                        let title = to_wide("DSH启动器");
                        MessageBoxW(hwnd, text.as_ptr(), title.as_ptr(), MB_OK);
                    }
                    KillTimer(hwnd, HOVER_TIMER_ID);
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
                    state.title_font.load(Ordering::Acquire),
                    state.body_font.load(Ordering::Acquire),
                    state.small_font.load(Ordering::Acquire),
                    state.button_font.load(Ordering::Acquire),
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

fn is_uncancellable_stage(message: &str) -> bool {
    message.contains("提交")
        || message.contains("目录交换")
        || message.contains("原子替换")
        || message.contains("正在恢复")
}

fn spawn_action(hwnd: HWND, state: Arc<AppState>, action: Action) {
    if state.busy.swap(true, Ordering::AcqRel) {
        push_status(hwnd, &state, "已有操作正在执行".to_owned());
        return;
    }
    CANCEL_REQUESTED.store(false, Ordering::Release);
    state.cancelable.store(true, Ordering::Release);
    unsafe {
        refresh_action_buttons(hwnd, &state);
    }
    push_status(hwnd, &state, format!("正在{}...", action.label()));
    let hwnd_value = hwnd as usize;
    thread::spawn(move || {
        let started_at = Instant::now();
        let progress = |message: &str| {
            let cancelable = !is_uncancellable_stage(message);
            state.cancelable.store(cancelable, Ordering::Release);
            push_status(
                hwnd_value as HWND,
                &state,
                format!(
                    "{message} · 已用 {} 秒 · {}",
                    started_at.elapsed().as_secs(),
                    if cancelable {
                        "可取消"
                    } else {
                        "提交阶段不可取消"
                    }
                ),
            );
        };
        let action_lock = match acquire_action_mutex() {
            Some(lock) => lock,
            None => {
                state.busy.store(false, Ordering::Release);
                state.cancelable.store(false, Ordering::Release);
                CANCEL_REQUESTED.store(false, Ordering::Release);
                push_status(
                    hwnd_value as HWND,
                    &state,
                    "已有启动器操作正在执行，请稍后重试".to_owned(),
                );
                return;
            }
        };
        let (result, should_exit_for_update) = match action {
            Action::Start => (start_dsh_with_progress(&progress), false),
            Action::Restart => (restart_dsh_with_progress(&progress), false),
            Action::Upgrade => (upgrade_dsh_with_progress(&progress), false),
            Action::LauncherUpdate => match update_launcher_with_progress(&progress) {
                Ok(outcome) => {
                    let should_exit = outcome.should_exit();
                    (Ok(outcome.into_message()), should_exit)
                }
                Err(error) => (Err(error), false),
            },
            Action::Repair => (repair_runtime_with_progress(&progress), false),
            Action::Migrate => (migrate_legacy_data_with_progress(&progress), false),
            Action::CleanupLegacy => (cleanup_legacy_data_to_recycle_bin(&progress), false),
            _ => (execute_action(action), false),
        };
        drop(action_lock);
        hold_health_status(&state);
        state.busy.store(false, Ordering::Release);
        state.cancelable.store(false, Ordering::Release);
        CANCEL_REQUESTED.store(false, Ordering::Release);
        let failed = result.is_err();
        let message = match result {
            Ok(message) => message,
            Err(error) => error,
        };
        push_status(hwnd_value as HWND, &state, message);
        unsafe {
            notify_user(
                hwnd_value as HWND,
                if failed {
                    "DSH 操作失败"
                } else {
                    "DSH 操作完成"
                },
                if failed {
                    "请打开诊断详情查看阶段、日志和修复建议"
                } else {
                    "操作已完成"
                },
                failed,
            );
        }
        schedule_health_check(hwnd_value as HWND, Arc::clone(&state));
        if should_exit_for_update {
            unsafe {
                PostMessageW(hwnd_value as HWND, SELF_UPDATE_EXIT_MESSAGE, 0, 0);
            }
        }
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

fn hold_health_status(state: &AppState) {
    if let Ok(mut publish_after) = state.health_publish_after.lock() {
        *publish_after = Instant::now() + ACTION_STATUS_HOLD;
    }
}

fn health_status_is_held(state: &AppState) -> bool {
    state
        .health_publish_after
        .lock()
        .map(|publish_after| Instant::now() < *publish_after)
        .unwrap_or(false)
}

fn schedule_health_check(hwnd: HWND, state: Arc<AppState>) {
    if state.busy.load(Ordering::Acquire)
        || health_status_is_held(&state)
        || state.health_checking.swap(true, Ordering::AcqRel)
    {
        return;
    }
    let hwnd_value = hwnd as usize;
    thread::spawn(move || {
        let preserved_runtime_error = state
            .current_status
            .lock()
            .map(|status| {
                if status.starts_with("运行时需要修复")
                    || status.starts_with("DSH 需要修复")
                    || status.starts_with("运行时修复失败")
                {
                    Some(status.clone())
                } else {
                    None
                }
            })
            .unwrap_or(None);
        let message = preserved_runtime_error.unwrap_or_else(dsh_web_health_status);
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
        let needs_publish = state
            .current_status
            .lock()
            .map(|current_status| *current_status != message)
            .unwrap_or(true);
        state.health_checking.store(false, Ordering::Release);
        if !state.busy.load(Ordering::Acquire)
            && !health_status_is_held(&state)
            && (changed || needs_publish)
        {
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
        CMD_LAUNCHER_UPDATE => Some(Action::LauncherUpdate),
        CMD_REPAIR => Some(Action::Repair),
        CMD_OPEN_WEB => Some(Action::OpenWeb),
        CMD_OPEN_DATA => Some(Action::OpenData),
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
        (CMD_OPEN_DATA, "打开数据目录"),
        (CMD_START, "启动服务"),
        (CMD_RESTART, "重启服务"),
        (CMD_STOP, "停止服务"),
        (CMD_UPGRADE, "更新 DSH"),
        (CMD_LAUNCHER_UPDATE, "更新启动器"),
        (CMD_MIGRATE_LEGACY, "迁移旧数据"),
        (CMD_CLEANUP_LEGACY, "移入旧数据回收站"),
        (CMD_EXIT, "退出"),
    ];
    let busy = state_for(hwnd)
        .map(|state| state.busy.load(Ordering::Acquire))
        .unwrap_or(false);
    let wide_labels: Vec<Vec<u16>> = labels.iter().map(|(_, label)| to_wide(label)).collect();
    for ((command, _), label) in labels.iter().zip(wide_labels.iter()) {
        if *command == CMD_START || *command == CMD_EXIT {
            AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        }
        let disabled = busy && *command != CMD_SHOW;
        let flags = MF_STRING | if disabled { MF_GRAYED } else { 0 };
        AppendMenuW(menu, flags, *command as usize, label.as_ptr());
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

unsafe fn notify_user(hwnd: HWND, title: &str, message: &str, error: bool) {
    let mut data = notify_data(hwnd, std::ptr::null_mut());
    data.uFlags = NIF_INFO;
    copy_wide(&mut data.szInfoTitle, title);
    copy_wide(&mut data.szInfo, message);
    data.dwInfoFlags = if error { NIIF_ERROR } else { NIIF_INFO };
    let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
}

unsafe fn delete_tray_icon(hwnd: HWND) {
    let data = notify_data(hwnd, std::ptr::null_mut());
    Shell_NotifyIconW(NIM_DELETE, &data);
}

fn notify_data(hwnd: HWND, hicon: HICON) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: TRAY_MESSAGE,
        hIcon: hicon,
        ..NOTIFYICONDATAW::default()
    }
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
        assert!(Action::from_name("launcher-update").is_some());
        assert!(Action::from_name("self-update").is_some());
        assert!(Action::from_name("repair").is_some());
        assert!(Action::from_name("migrate").is_some());
        assert!(Action::from_name("open").is_some());
        assert!(Action::from_name("web").is_some());
        assert!(Action::from_name("data").is_some());
        assert!(Action::from_name("open-data").is_some());
        assert!(Action::from_name("unknown").is_none());
    }

    #[test]
    fn single_instance_name_is_scoped_to_normalized_executable_path() {
        let first =
            single_instance_mutex_name_for_path(Path::new("D:\\DSH-Launcher\\DSH-Launcher.exe"));
        let same_path_different_case =
            single_instance_mutex_name_for_path(Path::new("d:\\dsh-launcher\\dsh-launcher.exe"));
        let other_path =
            single_instance_mutex_name_for_path(Path::new("E:\\DSH-Launcher\\DSH-Launcher.exe"));

        assert_eq!(first, same_path_different_case);
        assert_ne!(first, other_path);
        assert!(first.starts_with("Local\\DeepSeekHarness.DshLauncher."));
    }

    #[test]
    fn command_line_parser_rejects_unknown_arguments_and_supports_data_root() {
        let args = vec![
            "dsh-launcher.exe".to_owned(),
            "--action".to_owned(),
            "open-data".to_owned(),
            "--data-dir".to_owned(),
            "D:\\DSH-Launcher\\data".to_owned(),
        ];
        assert!(matches!(parse_action(&args), Ok(Some(Action::OpenData))));
        assert_eq!(
            data_dir_override(&args).expect("data directory should parse"),
            Some(PathBuf::from("D:\\DSH-Launcher\\data"))
        );
        assert!(parse_action(&["dsh-launcher.exe".to_owned(), "--unknown".to_owned(),]).is_err());
    }

    #[test]
    fn app_paths_keep_all_persistent_state_under_the_selected_root() {
        let paths = AppPaths::from_data_root(PathBuf::from("D:\\DSH-Launcher\\data"));
        assert_eq!(
            paths.runtime_root,
            PathBuf::from("D:\\DSH-Launcher\\data\\runtime")
        );
        assert_eq!(
            paths.npm_prefix,
            PathBuf::from("D:\\DSH-Launcher\\data\\npm-global")
        );
        assert_eq!(
            paths.dsh_home,
            PathBuf::from("D:\\DSH-Launcher\\data\\profile\\.dsh")
        );
        assert_eq!(
            paths.npm_cache,
            PathBuf::from("D:\\DSH-Launcher\\data\\cache\\npm")
        );
        assert_eq!(
            paths.pid_path(),
            PathBuf::from("D:\\DSH-Launcher\\data\\state\\dsh-launcher-native.pid")
        );
    }

    #[test]
    fn runtime_bootstrap_sources_have_fixed_sha256_values() {
        let manifest = runtime_manifest().expect("bundled runtime manifest should parse");
        assert_eq!(manifest.node_sha256.len(), 64);
        assert_eq!(manifest.dsh_bootstrap_version, "0.1.2-alpha.3");
        assert_eq!(
            manifest.dsh_registry_url,
            "https://registry.npmjs.org/@deepseek-ai%2fdsh"
        );
        assert!(manifest.dsh_peer_dependencies.is_empty());
        assert!(manifest.node_url.ends_with(&manifest.node_archive_name));
        assert!(manifest
            .node_url
            .starts_with("https://nodejs.org/download/release/"));
        assert_eq!(manifest.architecture, "x86_64-pc-windows-gnu");
    }

    #[test]
    fn runtime_manifest_rejects_unpinned_or_incomplete_values() {
        let mut value: serde_json::Value =
            serde_json::from_str(RUNTIME_MANIFEST_TEXT).expect("embedded manifest is JSON");
        value["node"]["sha256"] = serde_json::Value::String("bad".to_owned());
        assert!(parse_runtime_manifest(&value.to_string()).is_err());
        let mut value: serde_json::Value =
            serde_json::from_str(RUNTIME_MANIFEST_TEXT).expect("embedded manifest is JSON");
        value["node"]["url"] =
            serde_json::Value::String("https://example.com/node-v24.19.0-win-x64.zip".to_owned());
        assert!(parse_runtime_manifest(&value.to_string()).is_err());
        let mut value: serde_json::Value =
            serde_json::from_str(RUNTIME_MANIFEST_TEXT).expect("embedded manifest is JSON");
        value["dsh"]["peer_dependencies"] = serde_json::json!(["@deepseek-ai/dsh@bad"]);
        assert!(parse_runtime_manifest(&value.to_string()).is_err());
    }

    #[test]
    fn byte_sizes_are_human_readable_without_losing_small_values() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MiB");
    }

    #[test]
    fn log_rotation_keeps_five_files_per_log_type() {
        let root = env::temp_dir().join(format!(
            "dsh-launcher-log-test-{}-{}",
            std::process::id(),
            transaction_nonce()
        ));
        fs::create_dir_all(&root).expect("test log directory should be created");
        let active = root.join("service.log");
        fs::write(&active, vec![b'x'; LOG_ROTATION_LIMIT_BYTES as usize])
            .expect("active log should be written");
        for index in 1..LOG_ROTATION_FILES {
            fs::write(
                active.with_extension(format!("log.{index}")),
                format!("old-{index}"),
            )
            .expect("rotated log should be written");
        }

        rotate_log_file(&active).expect("log rotation should succeed");
        fs::write(&active, "new").expect("new active log should be written");
        assert!(active.is_file());
        for index in 1..LOG_ROTATION_FILES {
            assert!(active.with_extension(format!("log.{index}")).is_file());
        }
        assert!(!active
            .with_extension(format!("log.{LOG_ROTATION_FILES}"))
            .exists());
        fs::remove_dir_all(&root).expect("test log directory should be removed");
    }

    #[test]
    fn migration_rollback_recovers_before_commit_journal_write() {
        let base = env::temp_dir().join(format!(
            "dsh-launcher-migration-recovery-test-{}-{}",
            std::process::id(),
            transaction_nonce()
        ));
        let transaction = base.join("transaction");
        let source = base.join("source");
        let target = base.join("target");
        let backup = transaction.join(r"backups\runtime");
        let staged = transaction.join(r"stage\runtime");
        fs::create_dir_all(&source).expect("test source should be created");
        fs::create_dir_all(&target).expect("test target should be created");
        fs::create_dir_all(&backup).expect("test backup should be created");
        fs::create_dir_all(&staged).expect("test stage should be created");
        fs::write(source.join("source.txt"), "source").expect("test source should be written");
        fs::write(backup.join("old.txt"), "old").expect("test backup should be written");
        fs::write(staged.join("new.txt"), "new").expect("test stage should be written");

        let candidate = MigrationCandidate {
            id: "runtime",
            label: "test runtime",
            source: source.clone(),
            target: target.clone(),
        };
        rollback_migration(&transaction, std::slice::from_ref(&candidate), &[])
            .expect("rollback should restore a target recreated after a crash");
        assert_eq!(
            fs::read_to_string(target.join("old.txt")).expect("restored target should exist"),
            "old"
        );
        assert!(!transaction.exists());
        fs::remove_dir_all(&base).expect("migration recovery test root should be removed");
    }

    #[test]
    fn migration_rollback_preserves_unverified_target_contents() {
        let base = env::temp_dir().join(format!(
            "dsh-launcher-migration-safety-test-{}-{}",
            std::process::id(),
            transaction_nonce()
        ));
        let transaction = base.join("transaction");
        let source = base.join("source");
        let target = base.join("target");
        let backup = transaction.join(r"backups\runtime");
        fs::create_dir_all(&source).expect("test source should be created");
        fs::create_dir_all(&target).expect("test target should be created");
        fs::create_dir_all(&backup).expect("test backup should be created");
        fs::write(source.join("source.txt"), "source").expect("test source should be written");
        fs::write(target.join("untrusted.txt"), "untrusted")
            .expect("test target should be written");
        fs::write(backup.join("old.txt"), "old").expect("test backup should be written");

        let candidate = MigrationCandidate {
            id: "runtime",
            label: "test runtime",
            source,
            target: target.clone(),
        };
        assert!(
            rollback_migration(&transaction, std::slice::from_ref(&candidate), &["runtime"])
                .is_err()
        );
        assert_eq!(
            fs::read_to_string(target.join("untrusted.txt"))
                .expect("unverified target should be preserved"),
            "untrusted"
        );
        assert!(backup.exists());
        assert!(transaction.exists());
        fs::remove_dir_all(&base).expect("migration safety test root should be removed");
    }

    #[test]
    fn dsh_process_ownership_requires_entrypoint_web_and_port() {
        let command_line = r#""C:\Users\user\AppData\Local\DSH-Runtime\node\node.exe" C:\Users\user\AppData\Local\npm-global\node_modules\@deepseek-ai\dsh\lib\bin.js web --no-open --host 127.0.0.1 --port 3080"#;
        assert!(is_verified_dsh_command(command_line, 3080));
        assert!(!is_verified_dsh_command(command_line, 3081));
        assert!(is_verified_dsh_command(
            r#"node.exe C:\pkg\@deepseek-ai\dsh\lib\bin.js web"#,
            3080
        ));
        assert!(!is_verified_dsh_command(
            r#""C:\Program Files\other\server.exe" web --port 3080"#,
            3080
        ));
        assert!(!is_verified_dsh_command(
            r#"node.exe C:\pkg\@deepseek-ai\dsh\lib\bin.js --port 3080"#,
            3080
        ));
    }

    #[test]
    fn native_process_command_line_reader_handles_current_process() {
        let command_line = process_command_line(std::process::id())
            .expect("current process command line should be readable")
            .expect("current process should have a command line");
        assert!(!command_line.trim().is_empty());
    }

    #[test]
    fn netstat_parser_only_returns_listening_pid_for_requested_port() {
        assert_eq!(
            parse_listening_pid(
                "  TCP    127.0.0.1:3080    0.0.0.0:0    LISTENING    1404",
                3080
            ),
            Some(1404)
        );
        assert_eq!(
            parse_listening_pid(
                "  TCP    127.0.0.1:3081    0.0.0.0:0    LISTENING    1404",
                3080
            ),
            None
        );
        assert_eq!(
            parse_listening_pid(
                "  TCP    127.0.0.1:3080    0.0.0.0:0    ESTABLISHED    1404",
                3080
            ),
            None
        );
    }

    #[test]
    fn self_update_arguments_require_a_valid_parent_process() {
        let args = vec![
            "dsh-launcher.exe".to_owned(),
            "--apply-self-update".to_owned(),
            "transaction.json".to_owned(),
        ];
        assert_eq!(
            parse_self_update(&args),
            Some(PathBuf::from("transaction.json"))
        );
        let invalid_pid = vec![
            "dsh-launcher.exe".to_owned(),
            "--apply-self-update".to_owned(),
            "transaction.json".to_owned(),
            "unexpected".to_owned(),
        ];
        assert!(parse_self_update(&invalid_pid).is_none());
    }

    #[test]
    fn release_smoke_flag_requires_an_exact_standalone_argument() {
        assert!(is_release_smoke(&[
            "dsh-launcher.exe".to_owned(),
            "--release-smoke".to_owned(),
        ]));
        assert!(!is_release_smoke(&["dsh-launcher.exe".to_owned()]));
        assert!(!is_release_smoke(&[
            "dsh-launcher.exe".to_owned(),
            "--release-smoke".to_owned(),
            "unexpected".to_owned(),
        ]));
    }

    #[test]
    fn portable_root_requires_marker_and_never_falls_back() {
        let root = env::temp_dir().join(format!(
            "dsh-launcher-portable-test-{}-{}",
            std::process::id(),
            transaction_nonce()
        ));
        fs::create_dir_all(&root).expect("test directory should be created");
        assert!(resolve_data_root(&root, None).is_err());
        fs::write(root.join(PORTABLE_MARKER_FILE), "").expect("marker should be written");
        let data = resolve_data_root(&root, None).expect("marked directory should be accepted");
        assert_eq!(data, root.join(DATA_DIRECTORY));
        assert!(resolve_data_root(&root, Some(root.join("other"))).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dsh_semver_uses_precedence_and_rejects_malformed_values() {
        assert!(parse_dsh_semver("1.2.4").unwrap() > parse_dsh_semver("1.2.3").unwrap());
        assert!(parse_dsh_semver("1.2.3").unwrap() > parse_dsh_semver("1.2.3-rc.2").unwrap());
        assert!(parse_dsh_semver("1.2.3-rc.10").unwrap() > parse_dsh_semver("1.2.3-rc.2").unwrap());
        assert!(parse_dsh_semver("01.2.3").is_none());
        assert!(parse_dsh_semver("1.2").is_none());
        assert!(
            parse_dsh_semver("0.1.2-alpha.3").unwrap() > parse_dsh_semver("0.1.1-rc.2").unwrap()
        );
    }

    #[test]
    fn dsh_versions_are_checked_before_rollback() {
        assert!(is_safe_dsh_version("0.1.1-rc.2"));
        assert!(is_safe_dsh_version("1.2.3+build.4"));
        assert!(!is_safe_dsh_version(""));
        assert!(!is_safe_dsh_version("1.2.3;malicious"));
    }

    #[test]
    fn older_valid_dsh_can_be_used_as_an_upgrade_source() {
        let older = parse_dsh_upgrade_source_version("0.1.1-rc.1")
            .expect("a valid older release should be upgradeable");
        let bootstrap = parse_dsh_semver("0.1.1-rc.2").expect("bootstrap should be valid");
        assert!(older < bootstrap);
        assert!(parse_dsh_upgrade_source_version("0.1.1-rc.1;bad").is_err());
    }

    #[test]
    fn latest_version_parser_considers_prereleases_outside_latest_dist_tag() {
        assert_eq!(
            parse_latest_dsh_version(r#"["0.1.1-rc.2","0.1.2-alpha.2","0.1.2-alpha.3"]"#),
            Some("0.1.2-alpha.3".to_owned())
        );
        assert_eq!(
            parse_latest_dsh_version("0.1.1-rc.2\n0.1.2-alpha.3\n"),
            Some("0.1.2-alpha.3".to_owned())
        );
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
    fn self_update_health_requires_version_and_child_pid() {
        let healthy = "ready=true\nversion=0.2.1\npid=42\n";
        assert!(health_handshake_matches(healthy, "0.2.1", 42));
        assert!(!health_handshake_matches(healthy, "0.2.0", 42));
        assert!(!health_handshake_matches(healthy, "0.2.1", 43));
        assert!(!health_handshake_matches(
            "ready\nversion=0.2.1\npid=42\n",
            "0.2.1",
            42
        ));
    }

    #[test]
    fn chunked_health_response_is_decoded_and_validated() {
        let encoded = b"5\r\n{\"ok\"\r\n3\r\n:1}\r\n0\r\n\r\n";
        assert_eq!(decode_chunked_body(encoded).unwrap(), b"{\"ok\":1}");
        assert!(decode_chunked_body(b"4\r\nbad").is_err());
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
        assert_eq!(
            tray_icon_running_state("运行时需要修复：清单不一致"),
            Some(false)
        );
        assert_eq!(tray_icon_running_state("更新失败：网络错误"), Some(false));
    }

    #[test]
    fn status_colors_distinguish_stopped_progress_and_failures() {
        assert_eq!(status_accent("服务未启动"), COLOR_DISABLED);
        assert_eq!(status_accent("服务进程无响应"), COLOR_RED);
        assert_eq!(status_accent("运行时需要修复：Node.js 缺失"), COLOR_RED);
        assert_eq!(status_accent("正在准备 Node.js 运行时..."), COLOR_AMBER);
        assert_eq!(status_accent("服务运行中"), COLOR_GREEN);
        assert_eq!(scale_for_dpi(100, 144), 150);
        assert_eq!(scale_for_dpi(-16, 144), -24);
        assert!(is_uncancellable_stage("正在提交已验证的运行时目录..."));
        assert!(!is_uncancellable_stage("正在下载更新..."));
        assert!(status_is_abnormal("运行时需要修复：清单不一致"));
        assert!(!status_is_abnormal("服务已停止"));
    }

    #[test]
    fn home_page_only_exposes_frequent_actions() {
        for id in [
            CMD_START,
            CMD_STOP,
            CMD_OPEN_WEB,
            CMD_UPGRADE,
            CMD_MORE_TOOLS,
        ] {
            assert!(control_visible_on_page(UiPage::Home, id, false));
        }
        for id in [
            CMD_RESTART,
            CMD_REPAIR,
            CMD_OPEN_DATA,
            CMD_LAUNCHER_UPDATE,
            CMD_DETAILS,
            CMD_HOME,
            CMD_CANCEL,
        ] {
            assert!(!control_visible_on_page(UiPage::Home, id, false));
        }
        assert!(control_visible_on_page(UiPage::Home, CMD_EXIT, false));
    }

    #[test]
    fn tools_page_swaps_back_navigation_for_busy_cancel() {
        for id in [
            CMD_RESTART,
            CMD_REPAIR,
            CMD_OPEN_DATA,
            CMD_LAUNCHER_UPDATE,
            CMD_DETAILS,
            CMD_HOME,
        ] {
            assert!(control_visible_on_page(UiPage::Tools, id, false));
        }
        assert!(!control_visible_on_page(UiPage::Tools, CMD_CANCEL, false));
        assert!(!control_visible_on_page(UiPage::Tools, CMD_HOME, true));
        assert!(control_visible_on_page(UiPage::Tools, CMD_CANCEL, true));
    }

    #[test]
    fn launcher_versions_compare_release_tags() {
        assert_eq!(
            parse_launcher_version("v0.1.0"),
            Some(LauncherVersion {
                major: 0,
                minor: 1,
                patch: 0,
            })
        );
        assert!(
            parse_launcher_version("v0.1.1").unwrap() > parse_launcher_version("0.1.0").unwrap()
        );
        assert!(parse_launcher_version("0.1.0.1").is_none());
        assert!(parse_launcher_version("v0.1.0-rc.1").is_none());
    }

    #[test]
    fn launcher_release_parser_accepts_only_project_assets() {
        let release = parse_launcher_release(
            "v0.1.1\thttps://github.com/Francesco502/dsh-launcher/releases/download/v0.1.1/DSH-Launcher.exe\thttps://github.com/Francesco502/dsh-launcher/releases/download/v0.1.1/DSH-Launcher.exe.sha256\n",
        )
        .expect("release metadata should parse");
        assert_eq!(release.version, parse_launcher_version("0.1.1").unwrap());
        assert!(parse_launcher_release(
            "v0.1.1\thttps://example.com/DSH-Launcher.exe\thttps://github.com/Francesco502/dsh-launcher/releases/download/v0.1.1/DSH-Launcher.exe.sha256"
        )
        .is_err());
    }

    #[test]
    fn launcher_update_only_exits_after_scheduling_replacement() {
        assert!(!LauncherUpdateOutcome::UpToDate("已是最新".to_owned()).should_exit());
        assert!(LauncherUpdateOutcome::Scheduled("已安排更新".to_owned()).should_exit());
    }

    #[test]
    fn checksum_parser_accepts_common_sha256_manifest_lines() {
        let hash = "A".repeat(64);
        assert_eq!(
            parse_sha256(&format!("{hash}  DSH-Launcher.exe")),
            Some(hash.to_lowercase())
        );
        assert!(parse_sha256("not-a-sha256").is_none());
        assert!(parse_sha256(&"a".repeat(63)).is_none());
    }

    #[test]
    fn powershell_literals_escape_single_quotes() {
        assert_eq!(
            powershell_literal("C:\\DSH's\\launcher.exe"),
            "'C:\\DSH''s\\launcher.exe'"
        );
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
        // Five seconds are reserved for graceful tree shutdown and two more
        // for a forced process to publish its final status.
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
