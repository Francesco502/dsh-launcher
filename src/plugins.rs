use super::*;
use windows_sys::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_LISTVIEW_CLASSES, INITCOMMONCONTROLSEX, LVCF_TEXT, LVCF_WIDTH,
    LVCOLUMNW, LVIF_STATE, LVIF_TEXT, LVIS_STATEIMAGEMASK, LVITEMW, LVM_GETITEMSTATE,
    LVM_INSERTCOLUMNW, LVM_INSERTITEMW, LVM_SETEXTENDEDLISTVIEWSTYLE, LVM_SETITEMSTATE,
    LVM_SETITEMTEXTW, LVN_ITEMCHANGING, LVS_EX_CHECKBOXES, LVS_EX_FULLROWSELECT, LVS_EX_LABELTIP,
    LVS_REPORT, NMHDR, NMLISTVIEW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BS_DEFPUSHBUTTON, WM_NOTIFY, WS_BORDER, WS_EX_CLIENTEDGE, WS_VSCROLL,
};

const BRIDGE: &[u8] = include_bytes!("plugin_bridge.cjs");
const CLASS: &str = "DeepSeek.DSHLauncher.Plugins";
const LIST: u32 = 1201;
const SAVE: u32 = 1202;
const CANCEL_BUTTON: u32 = 1203;
const HELP: u32 = 1204;
const SAVED: u32 = WM_APP + 40;

#[derive(Clone)]
pub(super) struct Plugin {
    name: String,
    version: String,
    enabled: bool,
    supported: bool,
    aliases: Vec<String>,
    conflict: bool,
    reason: String,
}

pub(super) struct Catalog {
    key: String,
    plugins: Vec<Plugin>,
    previous_status: Option<String>,
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
    inspect_mode(paths, installation, settings, false)
}
fn inspect_mode(
    paths: &Paths,
    installation: &Installation,
    settings: &Path,
    preflight: bool,
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
    if preflight {
        command.arg("preflight");
    }
    command.env("TEMP", &paths.temp).env("TMP", &paths.temp);
    let output = run_capture(paths, &mut command, "读取插件配置", QUERY_TIMEOUT, true)?;
    serde_json::from_str(&output).map_err(|error| format!("插件列表格式无效：{error}"))
}

pub(super) fn preflight(paths: &Paths, installation: &Installation) -> Result<(), String> {
    inspect_mode(paths, installation, &settings_path(paths), true).map(|_| ())
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

fn catalog(paths: &Paths, installation: &Installation) -> Result<Catalog, String> {
    let value = inspect(paths, installation)?;
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
        previous_status: None,
    })
}

pub(super) unsafe fn request(hwnd: HWND) {
    let Some(state) = state_for(hwnd) else {
        return;
    };
    if !state.refresh.initialized.load(Ordering::Acquire) || state.busy.swap(true, Ordering::AcqRel)
    {
        return;
    }
    state.refresh.generation.fetch_add(1, Ordering::AcqRel);
    refresh_controls(hwnd, &state);
    let previous_status = state.status.lock().map(|v| v.clone()).unwrap_or_default();
    push_status(hwnd, &state, "正在读取插件列表...".to_owned());
    let owner = hwnd as usize;
    thread::spawn(move || {
        let result = (|| {
            let _guard = acquire_action_mutex().ok_or("已有启动器操作正在执行")?;
            let installation = discover_installation(&state.paths)?.ok_or("请先安装 DSH")?;
            let mut catalog = catalog(&state.paths, &installation)?;
            catalog.previous_status = Some(previous_status);
            Ok(catalog)
        })();
        if let Ok(mut pending) = state.plugin_result.lock() {
            *pending = Some(result);
        }
        PostMessageW(owner as HWND, PLUGINS_MESSAGE, 0, 0);
    });
}

struct Dialog {
    owner: HWND,
    state: Arc<AppState>,
    catalog: Catalog,
    initializing: AtomicBool,
    font: AtomicUsize,
    saved: AtomicBool,
    saving: AtomicBool,
}

pub(super) unsafe fn open(owner: HWND, state: Arc<AppState>, catalog: Catalog) {
    let module = GetModuleHandleW(std::ptr::null());
    let class_name = to_wide(CLASS);
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(dialog_proc),
        hInstance: module,
        hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
        hbrBackground: (COLOR_BTNFACE + 1) as *mut c_void,
        lpszClassName: class_name.as_ptr(),
        ..WNDCLASSEXW::default()
    };
    RegisterClassExW(&class);
    InitCommonControlsEx(&INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_LISTVIEW_CLASSES,
    });
    let data = Box::into_raw(Box::new(Dialog {
        owner,
        state: Arc::clone(&state),
        catalog,
        initializing: AtomicBool::new(true),
        font: AtomicUsize::new(0),
        saved: AtomicBool::new(false),
        saving: AtomicBool::new(false),
    }));
    let dpi = GetDpiForWindow(owner).max(96);
    let window = CreateWindowExW(
        WS_EX_CONTROLPARENT,
        class_name.as_ptr(),
        to_wide("选择插件").as_ptr(),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
        240,
        200,
        scale(660, dpi),
        scale(460, dpi),
        owner,
        std::ptr::null_mut(),
        module,
        data.cast(),
    );
    if window.is_null() {
        drop(Box::from_raw(data));
        finish_operation(owner, &state, Err("无法打开插件选择窗口".to_owned()), false);
        return;
    }
    state
        .plugin_window
        .store(window as usize, Ordering::Release);
    EnableWindow(owner, 0);
    ShowWindow(window, SW_SHOW);
    SetFocus(GetDlgItem(window, LIST as i32));
}

unsafe extern "system" fn dialog_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Dialog;
    if pointer.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let dialog = &*pointer;
    match message {
        WM_CREATE => {
            let list = CreateWindowExW(
                WS_EX_CLIENTEDGE,
                to_wide("SysListView32").as_ptr(),
                std::ptr::null(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WS_BORDER | WS_VSCROLL | LVS_REPORT,
                0,
                0,
                1,
                1,
                hwnd,
                LIST as usize as *mut c_void,
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null_mut(),
            );
            SendMessageW(
                list,
                LVM_SETEXTENDEDLISTVIEWSTYLE,
                0,
                (LVS_EX_CHECKBOXES | LVS_EX_FULLROWSELECT | LVS_EX_LABELTIP) as isize,
            );
            for (index, title) in ["插件", "版本 / 状态"].iter().enumerate() {
                let mut text = to_wide(title);
                let column = LVCOLUMNW {
                    mask: LVCF_TEXT | LVCF_WIDTH,
                    cx: 400,
                    pszText: text.as_mut_ptr(),
                    ..LVCOLUMNW::default()
                };
                SendMessageW(list, LVM_INSERTCOLUMNW, index, &column as *const _ as isize);
            }
            for (index, plugin) in dialog.catalog.plugins.iter().enumerate() {
                let mut name = to_wide(&plugin.name);
                let item = LVITEMW {
                    mask: LVIF_TEXT | LVIF_STATE,
                    iItem: index as i32,
                    pszText: name.as_mut_ptr(),
                    stateMask: LVIS_STATEIMAGEMASK,
                    state: if !plugin.supported {
                        0
                    } else if plugin.enabled {
                        2 << 12
                    } else {
                        1 << 12
                    },
                    ..LVITEMW::default()
                };
                SendMessageW(list, LVM_INSERTITEMW, 0, &item as *const _ as isize);
                SendMessageW(list, LVM_SETITEMSTATE, index, &item as *const _ as isize);
                let mut version = to_wide(&if plugin.reason.is_empty() {
                    plugin.version.clone()
                } else {
                    format!(
                        "{} · {}",
                        if plugin.enabled {
                            "已启用"
                        } else {
                            "已停用"
                        },
                        plugin.reason
                    )
                });
                let item = LVITEMW {
                    iSubItem: 1,
                    pszText: version.as_mut_ptr(),
                    ..LVITEMW::default()
                };
                SendMessageW(list, LVM_SETITEMTEXTW, index, &item as *const _ as isize);
            }
            create_control(
                hwnd,
                "STATIC",
                if dialog.catalog.plugins.is_empty() {
                    "当前 profile 没有已安装的第三方插件。"
                } else {
                    "勾选后保存，下次启动生效；运行中的 DSH 需要手动重启。无复选框的项目受配置限制。"
                },
                HELP,
                0,
                false,
            );
            create_control(hwnd, "BUTTON", "保存", SAVE, BS_DEFPUSHBUTTON as u32, true);
            create_control(hwnd, "BUTTON", "取消", CANCEL_BUTTON, 0, true);
            dialog_font(hwnd, dialog);
            dialog_layout(hwnd);
            dialog.initializing.store(false, Ordering::Release);
            0
        }
        WM_SIZE => {
            dialog_layout(hwnd);
            0
        }
        WM_DPICHANGED => {
            let rect = &*(lparam as *const RECT);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            dialog_font(hwnd, dialog);
            dialog_layout(hwnd);
            0
        }
        WM_NOTIFY => {
            let header = &*(lparam as *const NMHDR);
            if header.idFrom == LIST as usize && header.code == LVN_ITEMCHANGING {
                let change = &*(lparam as *const NMLISTVIEW);
                if !dialog.initializing.load(Ordering::Acquire)
                    && change.iItem >= 0
                    && dialog
                        .catalog
                        .plugins
                        .get(change.iItem as usize)
                        .is_some_and(|p| !p.supported)
                    && (change.uNewState ^ change.uOldState) & LVIS_STATEIMAGEMASK != 0
                {
                    return 1;
                }
            }
            0
        }
        WM_COMMAND => {
            match (wparam & 0xffff) as u32 {
                SAVE => save(hwnd, dialog),
                CANCEL_BUTTON | 2 if !dialog.saving.load(Ordering::Acquire) => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }
        SAVED => {
            let result = *Box::from_raw(lparam as *mut Result<(), String>);
            dialog.saving.store(false, Ordering::Release);
            match result {
                Ok(()) => {
                    dialog.saved.store(true, Ordering::Release);
                    dialog.state.sticky_status.store(true, Ordering::Release);
                    push_status(
                        dialog.owner,
                        &dialog.state,
                        "插件设置已保存，待下次启动 / 重启生效".to_owned(),
                    );
                    DestroyWindow(hwnd);
                }
                Err(error) => {
                    EnableWindow(GetDlgItem(hwnd, SAVE as i32), 1);
                    EnableWindow(GetDlgItem(hwnd, CANCEL_BUTTON as i32), 1);
                    EnableWindow(GetDlgItem(hwnd, LIST as i32), 1);
                    show_error_box(hwnd, &error);
                }
            }
            0
        }
        WM_CLOSE => {
            if !dialog.saving.load(Ordering::Acquire) {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            if !dialog.saved.load(Ordering::Acquire) {
                if let Ok(snapshot) = dialog.state.snapshot.lock() {
                    push_status(
                        dialog.owner,
                        &dialog.state,
                        dialog
                            .catalog
                            .previous_status
                            .clone()
                            .unwrap_or_else(|| status_for_snapshot(&snapshot)),
                    );
                }
            }
            dialog.state.plugin_window.store(0, Ordering::Release);
            dialog.state.busy.store(false, Ordering::Release);
            EnableWindow(dialog.owner, 1);
            refresh_controls(dialog.owner, &dialog.state);
            SetForegroundWindow(dialog.owner);
            let font = dialog.font.load(Ordering::Acquire);
            if font != 0 {
                DeleteObject(font as *mut c_void);
            }
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(pointer));
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

unsafe fn dialog_font(hwnd: HWND, dialog: &Dialog) {
    let font = create_font(scale(15, GetDpiForWindow(hwnd).max(96)), FW_NORMAL as i32);
    let old = dialog.font.swap(font as usize, Ordering::AcqRel);
    for id in [LIST, HELP, SAVE, CANCEL_BUTTON] {
        SendMessageW(GetDlgItem(hwnd, id as i32), WM_SETFONT, font as usize, 1);
    }
    if old != 0 {
        DeleteObject(old as *mut c_void);
    }
}

unsafe fn dialog_layout(hwnd: HWND) {
    let dpi = GetDpiForWindow(hwnd).max(96);
    let mut rect = RECT::default();
    GetClientRect(hwnd, &mut rect);
    let margin = scale(20, dpi);
    move_control(
        hwnd,
        HELP,
        margin,
        margin,
        rect.right - margin * 2,
        scale(52, dpi),
    );
    move_control(
        hwnd,
        LIST,
        margin,
        scale(80, dpi),
        rect.right - margin * 2,
        rect.bottom - scale(150, dpi),
    );
    move_control(
        hwnd,
        SAVE,
        rect.right - scale(228, dpi),
        rect.bottom - scale(52, dpi),
        scale(96, dpi),
        scale(32, dpi),
    );
    move_control(
        hwnd,
        CANCEL_BUTTON,
        rect.right - scale(116, dpi),
        rect.bottom - scale(52, dpi),
        scale(96, dpi),
        scale(32, dpi),
    );
    let list = GetDlgItem(hwnd, LIST as i32);
    SendMessageW(
        list,
        windows_sys::Win32::UI::Controls::LVM_SETCOLUMNWIDTH,
        0,
        (rect.right - scale(212, dpi)) as isize,
    );
    SendMessageW(
        list,
        windows_sys::Win32::UI::Controls::LVM_SETCOLUMNWIDTH,
        1,
        scale(160, dpi) as isize,
    );
}

unsafe fn save(hwnd: HWND, dialog: &Dialog) {
    if dialog.saving.swap(true, Ordering::AcqRel) {
        return;
    }
    let mut choices = serde_json::Map::new();
    let list = GetDlgItem(hwnd, LIST as i32);
    for (index, plugin) in dialog.catalog.plugins.iter().enumerate() {
        if plugin.supported {
            let flags = SendMessageW(list, LVM_GETITEMSTATE, index, LVIS_STATEIMAGEMASK as isize);
            let enabled = flags & LVIS_STATEIMAGEMASK as isize == 2 << 12;
            if enabled != plugin.enabled || plugin.conflict {
                for name in &plugin.aliases {
                    choices.insert(name.clone(), serde_json::Value::Bool(enabled));
                }
            }
        }
    }
    for id in [SAVE, CANCEL_BUTTON, LIST] {
        EnableWindow(GetDlgItem(hwnd, id as i32), 0);
    }
    let state = Arc::clone(&dialog.state);
    let key = dialog.catalog.key.clone();
    let window = hwnd as usize;
    thread::spawn(move || {
        let result = (|| {
            let _guard = acquire_action_mutex().ok_or("已有启动器操作正在执行")?;
            if choices.is_empty() {
                return Ok(());
            }
            let file = settings_path(&state.paths);
            let candidate = state.paths.state.join("plugin-settings.pending.json");
            if file.exists() {
                fs::copy(&file, &candidate).map_err(|e| e.to_string())?;
            } else {
                atomic_write(&candidate, b"{\"profiles\":{}}")?;
            }
            let checked = (|| {
                write_choices(&candidate, &key, choices)?;
                let installation = discover_installation(&state.paths)?.ok_or("请先安装 DSH")?;
                let value = inspect_settings(&state.paths, &installation, &candidate)?;
                if let Some(error) = value["error"].as_str() {
                    return Err(error.to_owned());
                }
                atomic_write(&file, &fs::read(&candidate).map_err(|e| e.to_string())?)
            })();
            let _ = fs::remove_file(candidate);
            checked
        })();
        let pointer = Box::into_raw(Box::new(result));
        if PostMessageW(window as HWND, SAVED, 0, pointer as isize) == 0 {
            drop(Box::from_raw(pointer));
        }
    });
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
    #[ignore = "opens real Win32 windows; run alone in an interactive desktop"]
    fn real_dialog_lifetime_stress() {
        use windows_sys::Win32::System::ProcessStatus::{
            K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};
        unsafe fn counters() -> (usize, u32) {
            let mut memory = PROCESS_MEMORY_COUNTERS_EX::default();
            let size = std::mem::size_of_val(&memory) as u32;
            memory.cb = size;
            assert_ne!(
                K32GetProcessMemoryInfo(
                    GetCurrentProcess(),
                    (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
                    size
                ),
                0
            );
            let mut handles = 0;
            assert_ne!(GetProcessHandleCount(GetCurrentProcess(), &mut handles), 0);
            (memory.PrivateUsage, handles)
        }
        let root = env::temp_dir().join(format!("dsh-dialog-stress-{}", transaction_nonce()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(PORTABLE_MARKER), b"").unwrap();
        fs::write(root.join(MANIFEST_FILE), MANIFEST_TEXT).unwrap();
        let state = Arc::new(AppState {
            paths: Paths::at_root(&root).unwrap(),
            blue_icon: AtomicUsize::new(0),
            black_icon: AtomicUsize::new(0),
            tray_icon: AtomicUsize::new(0),
            tray_added: AtomicBool::new(false),
            taskbar_created: 0,
            background_brush: 0,
            control_background_brush: AtomicUsize::new(0),
            title_font: AtomicUsize::new(0),
            body_font: AtomicUsize::new(0),
            small_font: AtomicUsize::new(0),
            snapshot: Mutex::new(Snapshot::default()),
            status: Mutex::new(String::new()),
            sticky_status: AtomicBool::new(false),
            messages: Mutex::new(VecDeque::new()),
            operation_result: Mutex::new(None),
            plugin_result: Mutex::new(None),
            plugin_window: AtomicUsize::new(0),
            busy: AtomicBool::new(false),
            cancelable: AtomicBool::new(false),
            refresh: Arc::new(SnapshotRefresh::default()),
            high_contrast: AtomicBool::new(false),
        });
        unsafe {
            let owner = CreateWindowExW(
                0,
                to_wide("STATIC").as_ptr(),
                to_wide("DSH plugin dialog lifetime test").as_ptr(),
                WS_OVERLAPPED | WS_CAPTION | WS_VISIBLE,
                100,
                100,
                500,
                400,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            );
            assert!(!owner.is_null());
            let cycle = || {
                let catalog = Catalog {
                    key: "portable:web".to_owned(),
                    previous_status: None,
                    plugins: vec![Plugin {
                        name: "fixture".to_owned(),
                        version: "1.0.0".to_owned(),
                        enabled: true,
                        supported: true,
                        aliases: vec!["fixture".to_owned()],
                        conflict: false,
                        reason: String::new(),
                    }],
                };
                open(owner, Arc::clone(&state), catalog);
                let dialog = state.plugin_window.load(Ordering::Acquire) as HWND;
                assert!(!dialog.is_null());
                let mut title = [0u16; 64];
                let length = windows_sys::Win32::UI::WindowsAndMessaging::GetWindowTextW(
                    dialog,
                    title.as_mut_ptr(),
                    64,
                );
                assert_eq!(
                    String::from_utf16_lossy(&title[..length as usize]),
                    "选择插件"
                );
                DestroyWindow(dialog);
                assert_eq!(state.plugin_window.load(Ordering::Acquire), 0);
                assert_eq!(Arc::strong_count(&state), 1);
                let mut message = MSG::default();
                while windows_sys::Win32::UI::WindowsAndMessaging::PeekMessageW(
                    &mut message,
                    std::ptr::null_mut(),
                    0,
                    0,
                    1,
                ) != 0
                {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            };
            for _ in 0..5 {
                cycle();
            }
            let before = counters();
            for _ in 0..50 {
                cycle();
            }
            let middle = counters();
            for _ in 0..50 {
                cycle();
            }
            let after = counters();
            let value = serde_json::json!({"cycles":100,"privateBefore":before.0,"privateAfter":after.0,"handlesBefore":before.1,"handlesMiddle":middle.1,"handlesAfter":after.1});
            println!("{value}");
            assert!(after.0 <= before.0 + 2 * 1024 * 1024);
            assert!(after.1 <= middle.1 + 1);
            DestroyWindow(owner);
            // A private Node fixture exercises ownership rediscovery without binding
            // the user's DSH port or loading their profile.
            state.paths.ensure_layout().unwrap();
            let entry = root.join("node_modules/@deepseek-ai/dsh/lib/bin.js");
            fs::create_dir_all(entry.parent().unwrap()).unwrap();
            fs::write(&entry, b"setTimeout(() => {}, 60000);").unwrap();
            let mut service = hidden_command(find_command("node.exe").unwrap())
                .arg(&entry)
                .args(["web", "--port", "3080"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            fs::write(state.paths.pid_file(), service.id().to_string()).unwrap();
            assert_eq!(tracked_dsh_pid(&state.paths).unwrap(), Some(service.id()));
            // Exercise the actual main-window message handlers without network work.
            state.busy.store(true, Ordering::Release);
            state.cancelable.store(true, Ordering::Release);
            state.refresh.initialized.store(true, Ordering::Release);
            let class_name = to_wide("DSH.Test.MainWindow");
            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(window_proc),
                hInstance: GetModuleHandleW(std::ptr::null()),
                lpszClassName: class_name.as_ptr(),
                ..WNDCLASSEXW::default()
            };
            RegisterClassExW(&class);
            let pointer = Box::into_raw(Box::new(Arc::clone(&state)));
            let window = CreateWindowExW(
                0,
                class_name.as_ptr(),
                to_wide("DSH message response test").as_ptr(),
                WS_OVERLAPPED | WS_CAPTION | WS_VISIBLE,
                100,
                100,
                500,
                440,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                pointer.cast(),
            );
            assert!(!window.is_null());
            let begin = Instant::now();
            SendMessageW(window, WM_COMMAND, CMD_MAIN as usize, 0);
            let response = begin.elapsed();
            assert!(CANCEL.load(Ordering::Acquire));
            assert!(response < Duration::from_millis(100));
            SendMessageW(window, WM_CLOSE, 0, 0);
            assert_eq!(IsWindowVisible(window), 0);
            assert!(service.try_wait().unwrap().is_none());
            assert_ne!(
                windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(window),
                0
            );
            show_main_window(window);
            assert_ne!(IsWindowVisible(window), 0);
            state.busy.store(false, Ordering::Release);
            CANCEL.store(false, Ordering::Release);
            SendMessageW(window, WM_COMMAND, CMD_EXIT as usize, 0);
            assert_eq!(
                windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(window),
                0
            );
            assert!(service.try_wait().unwrap().is_none());
            // WM_DESTROY released the observer; a fresh panel must rediscover it.
            assert_eq!(tracked_dsh_pid(&state.paths).unwrap(), Some(service.id()));
            lifecycle::set_window(std::ptr::null_mut());
            service.kill().unwrap();
            service.wait().unwrap();
            println!(
                "mainFeedbackMicroseconds={} closeHides=true reopenShows=true exitDestroys=true serviceSurvives=true rediscovered=true",
                response.as_micros()
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

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
