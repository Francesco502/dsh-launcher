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

#[derive(Clone)]
pub(super) struct Plugin {
    name: String,
    version: String,
    enabled: bool,
    supported: bool,
}

pub(super) struct Catalog {
    key: String,
    plugins: Vec<Plugin>,
}

fn settings_path(paths: &Paths) -> PathBuf {
    paths.state.join("plugin-settings.json")
}

fn inspect(paths: &Paths, installation: &Installation) -> Result<serde_json::Value, String> {
    let bridge = paths.state.join("plugin-bridge.cjs");
    if fs::read(&bridge).ok().as_deref() != Some(BRIDGE) {
        atomic_write(&bridge, BRIDGE)?;
    }
    let mut command = hidden_command(&installation.node);
    command
        .arg(bridge)
        .arg(&installation.entry)
        .arg(settings_path(paths));
    if installation.profile == ProfileMode::Portable {
        command.arg(&paths.profile);
    }
    command.env("TEMP", &paths.temp).env("TMP", &paths.temp);
    let output = run_capture(paths, &mut command, "读取插件配置", QUERY_TIMEOUT, false)?;
    serde_json::from_str(&output).map_err(|error| format!("插件列表格式无效：{error}"))
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
        });
    }
    Ok(Catalog { key, plugins })
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
    push_status(hwnd, &state, "正在读取插件列表...".to_owned());
    let owner = hwnd as usize;
    thread::spawn(move || {
        let result = (|| {
            let _guard = acquire_action_mutex().ok_or("已有启动器操作正在执行")?;
            let installation = discover_installation(&state.paths)?.ok_or("请先安装 DSH")?;
            catalog(&state.paths, &installation)
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
        finish_operation(owner, &state, Err("无法打开插件选择窗口".to_owned()));
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
        return 1;
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
                    state: if plugin.enabled { 2 << 12 } else { 1 << 12 },
                    ..LVITEMW::default()
                };
                SendMessageW(list, LVM_INSERTITEMW, 0, &item as *const _ as isize);
                SendMessageW(list, LVM_SETITEMSTATE, index, &item as *const _ as isize);
                let mut version = to_wide(&if plugin.supported {
                    plugin.version.clone()
                } else {
                    "不支持直接切换".to_owned()
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
                    "勾选要启用的插件，停止 DSH 后保存，下次启动生效。不兼容插件请保持停用。"
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
                SAVE => match save(hwnd, dialog) {
                    Ok(()) => {
                        dialog.saved.store(true, Ordering::Release);
                        push_status(
                            dialog.owner,
                            &dialog.state,
                            "插件设置已保存，下次启动生效".to_owned(),
                        );
                        DestroyWindow(hwnd);
                    }
                    Err(error) => show_error_box(hwnd, &error),
                },
                CANCEL_BUTTON | 2 => {
                    DestroyWindow(hwnd);
                }
                _ => {}
            }
            0
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            if !dialog.saved.load(Ordering::Acquire) {
                if let Ok(snapshot) = dialog.state.snapshot.lock() {
                    push_status(dialog.owner, &dialog.state, status_for_snapshot(&snapshot));
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

unsafe fn save(hwnd: HWND, dialog: &Dialog) -> Result<(), String> {
    let _guard = acquire_action_mutex().ok_or("已有启动器操作正在执行")?;
    if tracked_process_running(&dialog.state.paths)? || tcp_open(DSH_PORT) {
        return Err("DSH 已启动，请停止后再保存插件设置。".to_owned());
    }
    let mut choices = serde_json::Map::new();
    let list = GetDlgItem(hwnd, LIST as i32);
    for (index, plugin) in dialog.catalog.plugins.iter().enumerate() {
        if plugin.supported {
            let flags = SendMessageW(list, LVM_GETITEMSTATE, index, LVIS_STATEIMAGEMASK as isize);
            choices.insert(
                plugin.name.clone(),
                serde_json::Value::Bool(flags & LVIS_STATEIMAGEMASK as isize == 2 << 12),
            );
        }
    }
    write_choices(
        &settings_path(&dialog.state.paths),
        &dialog.catalog.key,
        choices,
    )
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
    profiles.insert(key.to_owned(), serde_json::Value::Object(choices));
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
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        assert_eq!(value["profiles"]["portable:web"]["plugin"], false);
        assert_eq!(value["profiles"]["user:other"]["plugin"], true);
        fs::write(&file, b"invalid").unwrap();
        assert!(write_choices(&file, "portable:web", choices(true)).is_err());
        assert_eq!(fs::read(&file).unwrap(), b"invalid");
        fs::remove_dir_all(root).unwrap();
    }
}
