//! Retain verified process identity; idle refreshes never start PowerShell.
use super::*;
use windows_sys::Win32::System::Threading::QueryFullProcessImageNameW;
use windows_sys::Win32::System::Threading::{
    CreateEventW, OpenEventW, RegisterWaitForSingleObject, SetEvent, UnregisterWaitEx,
    WaitForMultipleObjects, EVENT_MODIFY_STATE, INFINITE, WT_EXECUTEONLYONCE,
};

static WINDOW: AtomicUsize = AtomicUsize::new(0);
static GENERATION: AtomicUsize = AtomicUsize::new(0);
static TRACKED: Mutex<Option<Verified>> = Mutex::new(None);

struct Verified {
    pid: u32,
    process: ProcessHandle,
    command: String,
    _image: String,
    wait: HANDLE,
}
// Access is serialized by TRACKED; Windows process/wait handles are thread safe.
unsafe impl Send for Verified {}
impl Drop for Verified {
    fn drop(&mut self) {
        if !self.wait.is_null() {
            unsafe {
                UnregisterWaitEx(self.wait, INVALID_HANDLE_VALUE);
            }
        }
        // The process handle is released only after callbacks have completed.
    }
}
unsafe extern "system" fn exited(context: *mut c_void, _: bool) {
    let hwnd = WINDOW.load(Ordering::Acquire) as HWND;
    if !hwnd.is_null() {
        PostMessageW(hwnd, PROCESS_EXIT_MESSAGE, context as usize, 0);
    }
}
pub(super) fn set_window(hwnd: HWND) {
    WINDOW.store(hwnd as usize, Ordering::Release);
    if hwnd.is_null() {
        if let Ok(mut value) = TRACKED.try_lock() {
            *value = None;
        }
    }
}
pub(super) fn current_exit(generation: usize) -> bool {
    generation == GENERATION.load(Ordering::Acquire)
}
pub(super) fn command_line(pid: u32) -> Result<Option<String>, String> {
    let mut cache = TRACKED.lock().map_err(|_| "进程缓存不可用")?;
    if let Some(value) = cache.as_ref() {
        if value.pid == pid && value.process.running()? {
            return Ok(Some(value.command.clone()));
        }
    }
    *cache = None;
    let Some(process) = ProcessHandle::open(pid)? else {
        return Ok(None);
    };
    let Some(command) = process_command_line(pid)? else {
        return Ok(None);
    };
    let image = image(&process)?;
    if !matches!(
        Path::new(&image)
            .file_name()
            .and_then(OsStr::to_str)
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("node.exe" | "log-worker.exe")
    ) {
        return Ok(None);
    }
    if !process.running()? || !is_dsh_command(&command, DSH_PORT) {
        return Ok(None);
    }
    let mut wait = std::ptr::null_mut();
    let generation = GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
    if unsafe {
        RegisterWaitForSingleObject(
            &mut wait,
            process.0,
            Some(exited),
            generation as *const c_void,
            INFINITE,
            WT_EXECUTEONLYONCE,
        )
    } == 0
    {
        return Err("无法监视 DSH 退出".to_owned());
    }
    *cache = Some(Verified {
        pid,
        process,
        command: command.clone(),
        _image: image,
        wait,
    });
    Ok(Some(command))
}

pub(super) fn image(process: &ProcessHandle) -> Result<String, String> {
    let mut text = vec![0u16; 32768];
    let mut length = text.len() as u32;
    if unsafe { QueryFullProcessImageNameW(process.0, 0, text.as_mut_ptr(), &mut length) } == 0 {
        return Err("无法核验进程映像".to_owned());
    }
    Ok(String::from_utf16_lossy(&text[..length as usize]))
}
fn event_name(pid: u32, process: &ProcessHandle) -> Result<Vec<u16>, String> {
    Ok(to_wide(&format!(
        "Local\\DSH.Stop.{pid}.{}",
        process.times()?.0
    )))
}
pub(super) fn graceful_stop(pid: u32) -> Result<Option<bool>, String> {
    let Some(process) = ProcessHandle::open(pid)? else {
        return Ok(Some(true));
    };
    let name = event_name(pid, &process)?;
    let event = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr()) };
    if event.is_null() {
        return Ok(None);
    }
    let sent = unsafe { SetEvent(event) } != 0;
    unsafe {
        CloseHandle(event);
    }
    if !sent {
        return Ok(None);
    }
    Ok(Some(
        unsafe { WaitForSingleObject(process.0, STOP_TIMEOUT.as_millis() as u32) } == WAIT_OBJECT_0,
    ))
}

pub(super) struct StopChannel(HANDLE);
impl Drop for StopChannel {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
impl StopChannel {
    pub(super) fn create() -> Result<Self, String> {
        let pid = std::process::id();
        let process = ProcessHandle::open(pid)?.ok_or("日志进程已退出")?;
        // Default DACL belongs to the launching user; Local limits it to this session.
        let handle =
            unsafe { CreateEventW(std::ptr::null(), 0, 0, event_name(pid, &process)?.as_ptr()) };
        if handle.is_null() {
            return Err("无法创建停止通道".to_owned());
        }
        Ok(Self(handle))
    }
    pub(super) fn forward(
        &self,
        child: &std::process::Child,
        mut stdin: std::process::ChildStdin,
    ) -> thread::JoinHandle<()> {
        let event = self.0 as usize;
        let child_handle = child.as_raw_handle() as usize;
        thread::spawn(move || {
            let handles = [child_handle as HANDLE, event as HANDLE];
            if unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, INFINITE) }
                == WAIT_OBJECT_0 + 1
            {
                let _ = stdin.write_all(b"DSH_LAUNCHER_STOP\n");
                let _ = stdin.flush();
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stop_fixture() {
        let Ok(mode) = env::var("DSH_TEST_STOP_MODE") else {
            return;
        };
        let directory = PathBuf::from(env::var_os("DSH_TEST_STOP_DIRECTORY").unwrap());
        let preload = directory.join("preload.cjs");
        fs::write(&preload, BROWSER_ENTRY).unwrap();
        let script = if mode == "ignore" {
            "process.on('SIGTERM',()=>{});setInterval(()=>{},1000);console.log('ready')"
        } else {
            "process.on('SIGTERM',()=>process.exit(0));setInterval(()=>{},1000);console.log('ready')"
        };
        let mut command = hidden_command(find_command("node.exe").unwrap());
        command.arg("--require").arg(preload).args(["-e", script]);
        assert_eq!(log_relay::collect(&mut command, &directory).unwrap(), 0);
    }
    #[test]
    #[ignore = "runs real stop-channel fixtures including the 12-second timeout"]
    fn graceful_channel_stops_and_times_out() {
        for mode in ["exit", "ignore"] {
            let directory =
                env::temp_dir().join(format!("dsh-stop-channel-{}", transaction_nonce()));
            fs::create_dir_all(&directory).unwrap();
            let mut child = hidden_command(env::current_exe().unwrap())
                .args(["--exact", "lifecycle::tests::stop_fixture", "--nocapture"])
                .env("DSH_TEST_STOP_MODE", mode)
                .env("DSH_TEST_STOP_DIRECTORY", &directory)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(10);
            while !fs::read_to_string(directory.join("dsh.out.log"))
                .unwrap_or_default()
                .contains("ready")
            {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(20));
            }
            let mut tree = ProcessTree::capture(child.id()).unwrap();
            let start = Instant::now();
            assert_eq!(graceful_stop(child.id()).unwrap(), Some(mode == "exit"));
            if mode == "ignore" {
                assert!(start.elapsed() >= STOP_TIMEOUT);
                tree.terminate().unwrap();
            }
            child.wait().unwrap();
            assert!(tree.finished().unwrap());
            println!("stopMode={mode} elapsedMs={}", start.elapsed().as_millis());
            fs::remove_dir_all(directory).unwrap();
        }
    }
}
