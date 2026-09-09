//! Slint owns the event loop; workers own blocking service and filesystem work.
use super::*;
use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

slint::include_modules!();

#[derive(Default)]
struct OperationProgress {
    text: String,
    busy: bool,
    cancelable: bool,
}

struct Backend {
    paths: Paths,
    snapshot: Mutex<Snapshot>,
    progress: Mutex<OperationProgress>,
    refresh: Arc<SnapshotRefresh>,
}

enum Prompt {
    Worker(mpsc::Sender<bool>),
    Discard { close: bool },
}

struct Panel {
    backend: Arc<Backend>,
    window: Option<LauncherWindow>,
    tray: LauncherTray,
    timer: slint::Timer,
    catalog: Option<plugins::Catalog>,
    prompt: Option<Prompt>,
    error: Option<String>,
    view_generation: u64,
}

thread_local! { static PANEL: RefCell<Option<Panel>> = const { RefCell::new(None) }; }

fn dispatch(f: impl FnOnce(&mut Panel) + Send + 'static) {
    let _ = slint::invoke_from_event_loop(move || {
        PANEL.with_borrow_mut(|slot| {
            if let Some(panel) = slot {
                f(panel);
            }
        })
    });
}

pub(super) fn process_exit(generation: usize) {
    dispatch(move |panel| {
        if lifecycle::current_exit(generation) {
            panel.refresh(true);
        }
    });
}

fn show_event_name() -> Vec<u16> {
    let exe = env::current_exe().unwrap_or_default();
    to_wide(&format!(
        "Local\\DeepSeek.DSHLauncher.Show.{:016x}",
        instance_hash(&exe)
    ))
}

pub(super) fn signal_show() {
    use windows_sys::Win32::System::Threading::{OpenEventW, SetEvent, EVENT_MODIFY_STATE};
    unsafe {
        let event = OpenEventW(EVENT_MODIFY_STATE, 0, show_event_name().as_ptr());
        if !event.is_null() {
            SetEvent(event);
            CloseHandle(event);
        }
    }
}

fn observe_show_requests() -> Result<(), String> {
    use windows_sys::Win32::System::Threading::{CreateEventW, INFINITE};
    let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, show_event_name().as_ptr()) };
    if event.is_null() {
        return Err("无法创建面板唤醒事件".into());
    }
    let event = event as usize;
    thread::spawn(move || {
        while unsafe { WaitForSingleObject(event as HANDLE, INFINITE) } == WAIT_OBJECT_0 {
            dispatch(|panel| {
                if let Err(error) = panel.show() {
                    panel.error(error);
                }
            });
        }
        unsafe {
            CloseHandle(event as HANDLE);
        }
    });
    Ok(())
}

pub(super) fn run() -> Result<(), String> {
    unsafe {
        windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(
            to_wide(APP_USER_MODEL_ID).as_ptr(),
        );
    }
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name("software".into())
        .select()
        .map_err(|e| e.to_string())?;
    let backend = Arc::new(Backend {
        paths: app_paths()?,
        snapshot: Mutex::new(Snapshot::default()),
        progress: Mutex::new(OperationProgress::default()),
        refresh: Arc::new(SnapshotRefresh::default()),
    });
    let tray = LauncherTray::new().map_err(|e| e.to_string())?;
    tray.on_show_panel(|| {
        dispatch(|panel| {
            let _ = panel.show();
        })
    });
    tray.on_main_action(|| dispatch(|panel| panel.action("main")));
    tray.on_open_web(|| dispatch(|panel| panel.action("web")));
    tray.on_exit_app(|| {
        dispatch(|panel| {
            if !panel.backend.progress.lock().unwrap().busy {
                lifecycle::release_observer();
                let _ = slint::quit_event_loop();
            }
        })
    });
    tray.show().map_err(|e| e.to_string())?;
    let mut panel = Panel {
        backend,
        window: None,
        tray,
        timer: slint::Timer::default(),
        catalog: None,
        prompt: None,
        error: None,
        view_generation: 0,
    };
    panel.show()?;
    PANEL.with_borrow_mut(|slot| *slot = Some(panel));
    observe_show_requests()?;
    slint::run_event_loop().map_err(|e| e.to_string())
}

impl Panel {
    fn show(&mut self) -> Result<(), String> {
        if let Some(window) = &self.window {
            window.window().set_minimized(false);
            window.show().map_err(|e| e.to_string())?;
            return Ok(());
        }
        let window = LauncherWindow::new().map_err(|e| e.to_string())?;
        self.view_generation = self.view_generation.wrapping_add(1);
        window.set_version(APP_VERSION.into());
        window.on_action(|action| dispatch(move |panel| panel.action(action.as_str())));
        window.on_answer(|answer| dispatch(move |panel| panel.answer(answer)));
        window.on_leave_plugins(|| dispatch(|panel| panel.leave_plugins()));
        window.on_toggle(|index, enabled| {
            dispatch(move |panel| {
                if let Some(window) = &panel.window {
                    let rows = window.get_plugins();
                    if let Some(mut row) = rows.row_data(index as usize) {
                        row.enabled = enabled;
                        rows.set_row_data(index as usize, row);
                        window.set_dirty(true);
                    }
                }
            })
        });
        window.window().on_close_requested(|| {
            dispatch(|panel| panel.close());
            slint::CloseRequestResponse::KeepWindowShown
        });
        self.window = Some(window);
        self.sync();
        self.window
            .as_ref()
            .unwrap()
            .show()
            .map_err(|e| e.to_string())?;
        // A queued callback runs after show; runtime QA verifies the actual pixels.
        let weak = self.window.as_ref().unwrap().as_weak();
        slint::Timer::single_shot(Duration::from_millis(1), move || {
            if let Some(window) = weak.upgrade() {
                if window.window().take_snapshot().is_ok() {
                    self_update::window_ready();
                }
            }
        });
        if let Some(error) = self.error.clone() {
            self.error(error);
        }
        self.timer
            .start(slint::TimerMode::Repeated, Duration::from_secs(15), || {
                dispatch(|panel| panel.refresh(false))
            });
        self.refresh(false);
        Ok(())
    }

    fn close(&mut self) {
        if self
            .window
            .as_ref()
            .is_some_and(|window| window.get_dirty())
        {
            self.prompt = Some(Prompt::Discard { close: true });
            self.modal(
                "放弃未保存的更改？",
                "插件选择尚未保存。返回将保留原设置。",
                true,
            );
            return;
        }
        if let Some(Prompt::Worker(sender)) = self.prompt.take() {
            let _ = sender.send(false);
        }
        self.timer.stop();
        if let Some(window) = self.window.take() {
            let _ = window.hide();
        }
        self.catalog = None;
        self.view_generation = self.view_generation.wrapping_add(1);
    }

    fn sync(&self) {
        let snapshot = self.backend.snapshot.lock().unwrap().clone();
        let progress = self.backend.progress.lock().unwrap();
        let ready = self.backend.refresh.initialized.load(Ordering::Acquire);
        self.tray.set_running(snapshot.healthy);
        self.tray.set_busy(progress.busy || !ready);
        if let Some(window) = &self.window {
            window.set_running(snapshot.healthy);
            window.set_initialized(ready);
            window.set_status(
                if ready {
                    status_for_snapshot(&snapshot)
                } else {
                    "正在检查本机 DSH…".into()
                }
                .into(),
            );
            window.set_task(
                if snapshot.pending_plugins && !progress.busy {
                    format!("{}\n插件选择待下次启动生效。", progress.text)
                } else {
                    progress.text.clone()
                }
                .into(),
            );
            window.set_busy(progress.busy);
            let button = main_button(&snapshot, progress.busy || !ready, progress.cancelable);
            window.set_main_label(
                match button {
                    MainButton::Start => "启动 DSH",
                    MainButton::Stop => "停止 DSH",
                    MainButton::InstallDsh => "安装 DSH",
                    MainButton::InstallNode => "安装 Node.js",
                    MainButton::RepairDsh => "重新安装 DSH",
                    MainButton::Cancel => "取消",
                    MainButton::Busy => "正在处理…",
                }
                .into(),
            );
            window.set_main_enabled(button != MainButton::Busy);
            window.set_web_enabled(snapshot.healthy && !progress.busy);
            window.set_restart_enabled(restart_allowed(&snapshot, progress.busy));
            window.set_plugins_enabled(ready && !progress.busy && snapshot.installation.is_some());
            window.set_update_enabled(
                ready
                    && !progress.busy
                    && snapshot.npm_available
                    && snapshot.installation.is_some(),
            );
        }
    }

    fn refresh(&self, hidden: bool) {
        let backend = self.backend.clone();
        if !health_check_allowed(
            hidden || self.window.is_some(),
            backend.progress.lock().unwrap().busy,
            backend.refresh.checking.load(Ordering::Acquire),
        ) {
            return;
        }
        self.backend.refresh.spawn(
            move || {
                if !backend.refresh.initialized.load(Ordering::Acquire) {
                    if let Some(_guard) = acquire_action_mutex() {
                        if let Err(error) = recover_for_use(&backend.paths) {
                            return Snapshot {
                                discovery_error: Some(error),
                                ..Snapshot::default()
                            };
                        }
                        cleanup_npm_cache(&backend.paths);
                    }
                }
                refresh_discovery(&backend.paths)
            },
            || {
                dispatch(|panel| {
                    let busy = panel.backend.progress.lock().unwrap().busy;
                    if let Some(snapshot) = panel.backend.refresh.take_current(busy) {
                        *panel.backend.snapshot.lock().unwrap() = snapshot;
                        panel.sync();
                    }
                })
            },
        );
    }

    fn modal(&self, title: &str, text: &str, confirming: bool) {
        if let Some(window) = &self.window {
            window.set_modal_title(title.into());
            window.set_modal_text(text.into());
            window.set_copy_label("复制详情".into());
            window.set_details_expanded(false);
            window.set_confirming(confirming);
            window.set_modal_visible(true);
        }
    }

    fn error(&mut self, text: String) {
        append_log(&self.backend.paths.logs.join("launcher.log"), &text);
        self.error = Some(text.clone());
        if let Some(window) = &self.window {
            let line = text.lines().next().unwrap_or("操作未完成");
            let summary = line.chars().take(120).collect::<String>();
            window.set_error_summary(
                if line.chars().count() > 120 {
                    format!("{summary}…")
                } else {
                    summary
                }
                .into(),
            );
            window.set_error_recovery(text.contains("插件") || text.contains("profile"));
        }
        self.modal(
            "操作未完成",
            &format!(
                "{text}\n\n日志：{}",
                self.backend.paths.logs.join("launcher.log").display()
            ),
            false,
        );
    }

    fn answer(&mut self, answer: bool) {
        if let Some(window) = &self.window {
            window.set_modal_visible(false);
        }
        match self.prompt.take() {
            Some(Prompt::Worker(sender)) => {
                let _ = sender.send(answer);
            }
            Some(Prompt::Discard { close }) if answer => {
                self.view_generation = self.view_generation.wrapping_add(1);
                if let Some(window) = &self.window {
                    window.set_dirty(false);
                    window.set_page(0);
                }
                self.catalog = None;
                if close {
                    self.close();
                }
            }
            _ => {}
        }
        self.error = None;
    }

    fn leave_plugins(&mut self) {
        if self.window.as_ref().is_some_and(|w| w.get_dirty()) {
            self.prompt = Some(Prompt::Discard { close: false });
            self.modal("放弃未保存的更改？", "取消会保留原插件设置。", true);
        } else {
            self.view_generation = self.view_generation.wrapping_add(1);
            if let Some(window) = &self.window {
                window.set_page(0);
            }
            self.catalog = None;
        }
    }

    fn action(&mut self, action: &str) {
        if action == "copy" {
            if let Some(window) = &self.window {
                window.set_copy_label(
                    if copy_text(&window.get_modal_text()) {
                        "已复制"
                    } else {
                        "复制失败，请重试"
                    }
                    .into(),
                );
            }
            return;
        }
        if action == "error-plugins" && !self.backend.progress.lock().unwrap().busy {
            self.answer(false);
            self.load_plugins();
            return;
        }
        if self.prompt.is_some() || self.window.as_ref().is_some_and(|w| w.get_modal_visible()) {
            return;
        }
        let busy = self.backend.progress.lock().unwrap().busy;
        if action == "main" && busy {
            if self.backend.progress.lock().unwrap().cancelable {
                CANCEL.store(true, Ordering::Release);
            }
            return;
        }
        if busy || !self.backend.refresh.initialized.load(Ordering::Acquire) {
            return;
        }
        let snapshot = self.backend.snapshot.lock().unwrap().clone();
        match action {
            "main" => match main_button(&snapshot, false, false) {
                MainButton::Start => self.operate(Operation::Start), MainButton::Stop => self.operate(Operation::Stop),
                MainButton::InstallDsh => self.operate(Operation::Install), MainButton::RepairDsh => self.operate(Operation::Upgrade),
                MainButton::InstallNode => { if let Err(e) = open_url(NODE_DOWNLOAD_URL) { self.error(e); } }, _ => {}
            },
            "web" if snapshot.healthy => self.operate(Operation::Open),
            "restart" if restart_allowed(&snapshot, false) => self.operate(Operation::Restart),
            "update" => self.operate(Operation::Upgrade),
            "plugins" => self.load_plugins(),
            "save-plugins" => self.save_plugins(),
            "repair-plugins" => self.repair_plugins(),
            "launcher-update" => {
                let backend = self.begin("正在检查启动器更新…", true);
                thread::spawn(move || {
                    let result = (|| {
                        let _guard = acquire_action_mutex().ok_or("已有启动器操作正在执行")?;
                        self_update::prepare(&backend.paths, &confirm, &|text, cancelable| progress(&backend, text, cancelable))
                    })();
                    match result {
                        Ok(true) => { let _ = slint::quit_event_loop(); }
                        Ok(false) => complete(backend, Ok(format!("启动器已是最新版本 · v{APP_VERSION}"))),
                        Err(e) => complete(backend, Err(e)),
                    }
                });
            }
            "about" => self.modal("DSH 启动器", &format!("v{APP_VERSION}\n\n简洁、专注的本地 DSH 管理工具。\n\n界面使用 Slint — https://slint.dev\nSlint Royalty-free License 2.0\n项目代码：MIT"), false),
            _ => {}
        }
    }

    fn begin(&mut self, text: &str, cancelable: bool) -> Arc<Backend> {
        self.backend
            .refresh
            .generation
            .fetch_add(1, Ordering::AcqRel);
        *self.backend.progress.lock().unwrap() = OperationProgress {
            text: text.into(),
            busy: true,
            cancelable,
        };
        CANCEL.store(false, Ordering::Release);
        self.sync();
        self.backend.clone()
    }

    fn operate(&mut self, operation: Operation) {
        let backend = self.begin(
            "正在处理…",
            matches!(
                operation,
                Operation::Start | Operation::Install | Operation::Upgrade
            ),
        );
        thread::spawn(move || {
            let result = (|| {
                let _guard = acquire_action_mutex().ok_or("已有启动器操作正在执行")?;
                if !matches!(
                    operation,
                    Operation::Stop | Operation::Restart | Operation::Open
                ) {
                    recover_for_use(&backend.paths)?;
                }
                let report = |text: &str, cancelable| progress(&backend, text, cancelable);
                match operation {
                    Operation::Start => start_dsh(),
                    Operation::Stop => stop_dsh(),
                    Operation::Open => open_dsh_web(&backend.paths),
                    Operation::Restart => restart_sequence(stop_dsh, start_dsh, &report),
                    Operation::Install => install_or_update(true, &report, Some(&confirm)),
                    Operation::Upgrade => install_or_update(false, &report, Some(&confirm)),
                }
            })();
            complete(backend, result);
        });
    }

    fn load_plugins(&mut self) {
        if let Some(window) = &self.window {
            window.set_page(1);
            window.set_plugins(ModelRc::default());
            window.set_plugin_note("正在读取插件…".into());
            window.set_can_save(false);
            window.set_can_repair(false);
        }
        let backend = self.begin("正在读取插件…", false);
        let generation = self.view_generation;
        thread::spawn(move || {
            let result = (|| {
                let _guard = acquire_action_mutex().ok_or("已有启动器操作正在执行")?;
                let installation = discover_installation(&backend.paths)?.ok_or("请先安装 DSH")?;
                plugins::catalog(&backend.paths, &installation)
            })();
            {
                let mut progress = backend.progress.lock().unwrap();
                progress.busy = false;
                progress.text.clear();
            }
            dispatch(move |panel| {
                if panel.view_generation != generation {
                    panel.sync();
                    return;
                }
                match result {
                    Ok(catalog) => {
                        if let Some(window) = &panel.window {
                            let rows = catalog
                                .plugins
                                .iter()
                                .map(|p| PluginRow {
                                    name: p.name.clone().into(),
                                    detail: format!("{}  {}", p.version, p.reason).into(),
                                    enabled: p.enabled,
                                    supported: p.supported,
                                })
                                .collect::<Vec<_>>();
                            window.set_plugins(ModelRc::from(Rc::new(VecModel::from(rows))));
                            window.set_plugin_note(catalog.note.clone().into());
                            window.set_can_save(catalog.complete);
                            window.set_can_repair(!catalog.complete);
                            window.set_dirty(false);
                            panel.catalog = Some(catalog);
                        }
                    }
                    Err(error) => {
                        if let Some(window) = &panel.window {
                            window.set_plugin_note(error.clone().into());
                        }
                        panel.error(error);
                    }
                }
                panel.sync();
            });
        });
    }

    fn save_plugins(&mut self) {
        let Some(catalog) = self.catalog.clone() else {
            return;
        };
        let Some(window) = &self.window else {
            return;
        };
        let choices = window
            .get_plugins()
            .iter()
            .map(|p| p.enabled)
            .collect::<Vec<_>>();
        let backend = self.begin("正在保存插件选择…", false);
        let generation = self.view_generation;
        thread::spawn(move || {
            let result = (|| {
                let _guard = acquire_action_mutex().ok_or("已有启动器操作正在执行")?;
                plugins::save_choices(&backend.paths, &catalog, &choices)?;
                Ok("插件选择已保存，下次启动生效；运行中的 DSH 需手动重启。".into())
            })();
            if result.is_ok() {
                dispatch(move |panel| {
                    if panel.view_generation != generation {
                        return;
                    }
                    if let Some(w) = &panel.window {
                        w.set_dirty(false);
                        w.set_page(0);
                    }
                    panel.catalog = None;
                });
            }
            complete(backend, result);
        });
    }

    fn repair_plugins(&mut self) {
        let backend = self.begin("正在检查插件修复方案…", true);
        thread::spawn(move || {
            let result = plugins::repair(&backend.paths, &confirm, &|text, cancelable| {
                progress(&backend, text, cancelable)
            });
            let success = result.is_ok();
            complete(backend, result);
            if success {
                dispatch(|panel| {
                    if panel.window.as_ref().is_some_and(|w| w.get_page() == 1) {
                        panel.load_plugins();
                    }
                });
            }
        });
    }
}

fn progress(backend: &Backend, text: &str, cancelable: bool) {
    {
        let mut p = backend.progress.lock().unwrap();
        p.text = text.into();
        p.cancelable = cancelable;
    }
    dispatch(|panel| panel.sync());
}

fn confirm(text: &str) -> bool {
    let (tx, rx) = mpsc::channel();
    let text = text.to_owned();
    dispatch(move |panel| {
        if panel.window.is_none() {
            let _ = tx.send(false);
            return;
        }
        panel.prompt = Some(Prompt::Worker(tx));
        panel.modal("请确认", &text, true);
    });
    loop {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(value) => return value,
            Err(mpsc::RecvTimeoutError::Disconnected) => return false,
            Err(_) if CANCEL.load(Ordering::Acquire) => return false,
            Err(_) => {}
        }
    }
}

fn complete(backend: Arc<Backend>, result: Result<String, String>) {
    *backend.snapshot.lock().unwrap() = refresh_discovery(&backend.paths);
    backend.refresh.initialized.store(true, Ordering::Release);
    {
        let mut p = backend.progress.lock().unwrap();
        p.busy = false;
        p.cancelable = false;
        p.text = match &result {
            Ok(text) | Err(text) => text.clone(),
        };
    }
    CANCEL.store(false, Ordering::Release);
    dispatch(move |panel| {
        if let Err(error) = result {
            if error != "操作已取消" {
                panel.error(error);
            }
        }
        panel.sync();
    });
}

fn copy_text(text: &str) -> bool {
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
    let value = to_wide(text);
    unsafe {
        let memory = GlobalAlloc(GMEM_MOVEABLE, value.len() * 2);
        if memory.is_null() {
            return false;
        }
        let pointer = GlobalLock(memory);
        if pointer.is_null() {
            GlobalFree(memory);
            return false;
        }
        std::ptr::copy_nonoverlapping(value.as_ptr(), pointer.cast(), value.len());
        GlobalUnlock(memory);
        let owner = GetActiveWindow();
        if owner.is_null() || OpenClipboard(owner) == 0 {
            GlobalFree(memory);
            return false;
        }
        let copied = EmptyClipboard() != 0 && !SetClipboardData(13, memory).is_null();
        if !copied {
            GlobalFree(memory);
        }
        CloseClipboard();
        copied
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resources() -> (usize, u32) {
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};
        unsafe {
            let mut memory: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
            memory.cb = std::mem::size_of_val(&memory) as u32;
            assert_ne!(
                GetProcessMemoryInfo(
                    GetCurrentProcess(),
                    (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
                    memory.cb
                ),
                0
            );
            let mut handles = 0;
            assert_ne!(GetProcessHandleCount(GetCurrentProcess(), &mut handles), 0);
            (memory.PrivateUsage, handles)
        }
    }

    #[test]
    #[ignore = "creates Slint windows; external watchdog, run alone on Windows"]
    fn slint_window_lifetime_and_opaque_states() {
        slint::BackendSelector::new()
            .backend_name("winit".into())
            .renderer_name("software".into())
            .select()
            .unwrap();
        let tray = LauncherTray::new().unwrap();
        tray.show().unwrap();
        let current = Rc::new(RefCell::new(None::<LauncherWindow>));
        let samples = Rc::new(RefCell::new(Vec::new()));
        let count = Rc::new(std::cell::Cell::new(0));
        let resource_samples = Rc::new(RefCell::new(Vec::new()));
        let resource_rows = resource_samples.clone();
        let started = Rc::new(std::cell::Cell::new(Instant::now()));
        let timer = slint::Timer::default();
        let (slot, results, iterations, clock) = (
            current.clone(),
            samples.clone(),
            count.clone(),
            started.clone(),
        );
        timer.start(
            slint::TimerMode::Repeated,
            Duration::from_millis(30),
            move || {
                if let Some(window) = slot.borrow_mut().take() {
                    let pixels = window.window().take_snapshot().unwrap();
                    assert!(pixels.width() >= 440 && pixels.height() >= 420);
                    assert!(
                        pixels.as_slice().iter().all(|pixel| pixel.a == 255),
                        "client must be fully opaque"
                    );
                    assert!(
                        pixels
                            .as_slice()
                            .iter()
                            .filter(|pixel| pixel.r < 200)
                            .count()
                            > 100,
                        "text and controls must be rendered"
                    );
                    results.borrow_mut().push(clock.get().elapsed().as_micros());
                    let weak = window.as_weak();
                    window.hide().unwrap();
                    drop(window);
                    assert!(weak.upgrade().is_none(), "closed component was retained");
                }
                let n = iterations.get();
                if n > 0 && n % 10 == 0 {
                    let usage = resources();
                    println!("cycle={n} privateBytes={} handles={}", usage.0, usage.1);
                    resource_rows.borrow_mut().push(usage);
                }
                if n == 100 {
                    slint::quit_event_loop().unwrap();
                    return;
                }
                clock.set(Instant::now());
                let window = LauncherWindow::new().unwrap();
                window.set_version(APP_VERSION.into());
                match n % 3 {
                    1 => {
                        window.set_page(1);
                        window.set_plugin_note("插件缺失，原设置未修改。".into());
                        window.set_plugins(ModelRc::from(Rc::new(VecModel::from(vec![
                            PluginRow {
                                name: "@qa/missing".into(),
                                detail: "无法解析插件依赖".into(),
                                ..Default::default()
                            },
                        ]))));
                    }
                    2 => {
                        window.set_modal_visible(true);
                        window.set_modal_title("更新确认".into());
                        window.set_modal_text("将更新 DSH，确认前不修改原安装。".into());
                        window.set_confirming(true);
                    }
                    _ => {}
                }
                window.show().unwrap();
                *slot.borrow_mut() = Some(window);
                iterations.set(n + 1);
            },
        );
        slint::run_event_loop().unwrap();
        timer.stop();
        tray.hide().unwrap();
        assert_eq!(samples.borrow().len(), 100);
        let resource_samples = resource_samples.borrow();
        let baseline = resource_samples[2]; // Allow initial font and accessibility caches to settle.
        let last = resource_samples[9];
        assert!(
            last.0 <= baseline.0 + 8 * 1024 * 1024,
            "memory grows after warmup"
        );
        assert!(
            last.1 <= baseline.1 + 10,
            "window handles grow after warmup"
        );
        let mut measured = samples.borrow().clone();
        measured.sort();
        println!(
            "cycles=100 opaque=true componentsReleased=true timerToSnapshotP95Us={}",
            measured[94]
        );
    }
}
