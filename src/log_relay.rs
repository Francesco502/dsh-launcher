//! Keep bounded DSH logs alive independently of the launcher window or CLI.
use super::{append_log, hidden_command, valid_web_url, Paths, LOG_COPIES, LOG_LIMIT};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::thread;

pub(super) fn is_worker(args: &[String]) -> bool {
    if !matches!(args.len(), 11 | 13) || args[1] != "--dsh-log-worker" || args[5] != "web" {
        return false;
    }
    let tail = if args.len() == 13 {
        if args[6] != "--patch" || args[7].is_empty() {
            return false;
        }
        &args[8..]
    } else {
        &args[6..]
    };
    tail == ["--no-open", "--host", "127.0.0.1", "--port", "3080"]
}

pub(super) fn run(
    paths: &Paths,
    node: &Path,
    entry: &Path,
    patch: Option<&Path>,
) -> Result<i32, String> {
    let mut command = hidden_command(node);
    command
        .arg("--require")
        .arg(paths.state.join("browser-entry.cjs"))
        .arg(entry)
        .arg("web");
    if let Some(patch) = patch {
        if patch != paths.state.join("plugin-startup.patch.json") {
            return Err("插件启动覆盖层路径无效".to_owned());
        }
        command.arg("--patch").arg(patch);
    }
    command.args(["--no-open", "--host", "127.0.0.1", "--port", "3080"]);
    collect(&mut command, &paths.logs)
}

fn collect(command: &mut std::process::Command, logs: &Path) -> Result<i32, String> {
    let out = RotatingLog::open(logs.join("dsh.out.log"), LOG_LIMIT, true)
        .map_err(|error| format!("无法准备 DSH 输出日志：{error}"))?;
    let mut err = RotatingLog::open(logs.join("dsh.err.log"), LOG_LIMIT, false)
        .map_err(|error| format!("无法准备 DSH 错误日志：{error}"))?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            let message = format!("无法启动 DSH：{error}");
            let _ = err.write(message.as_bytes());
            message
        })?;
    let error_log = logs.join("launcher.log");
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let other_error_log = error_log.clone();
    let out_thread = thread::spawn(move || pump(stdout, out, &error_log));
    let err_thread = thread::spawn(move || pump(stderr, err, &other_error_log));
    let status = child.wait().map_err(|error| error.to_string())?;
    for reader in [out_thread, err_thread] {
        reader
            .join()
            .map_err(|_| "DSH 日志线程异常".to_owned())?
            .map_err(|error| format!("无法读取 DSH 输出：{error}"))?;
    }
    Ok(status.code().unwrap_or(1))
}

fn pump(mut reader: impl Read, mut log: RotatingLog, error_log: &Path) -> io::Result<()> {
    let mut buffer = [0u8; 8192];
    let mut failed = false;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(());
        }
        if !failed {
            if let Err(error) = log.write(&buffer[..read]) {
                append_log(error_log, &format!("DSH 日志写入失败：{error}"));
                log.file = None;
                failed = true; // Keep draining if the disk becomes unavailable.
            }
        }
    }
}

struct RotatingLog {
    path: PathBuf,
    file: Option<File>,
    size: u64,
    limit: u64,
    auth: bool,
    pending: Vec<u8>,
    overflow: bool,
    header: Vec<u8>,
}

impl RotatingLog {
    fn open(path: PathBuf, limit: u64, auth: bool) -> io::Result<Self> {
        for index in 0..LOG_COPIES {
            trim_old_log(&log_path(&path, index), limit)?;
        }
        remove_if_present(&log_path(&path, LOG_COPIES))?;
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let size = file.metadata()?.len();
        Ok(Self {
            path,
            file: Some(file),
            size,
            limit,
            auth,
            pending: Vec::new(),
            overflow: false,
            header: Vec::new(),
        })
    }

    fn observe_auth(&mut self, bytes: &[u8]) {
        if !self.auth {
            return;
        }
        for part in bytes.split_inclusive(|byte| *byte == b'\n') {
            if self.pending.len() + part.len() > 8192 {
                self.overflow = true;
            }
            if !self.overflow {
                self.pending.extend_from_slice(part);
            }
            if part.ends_with(b"\n") {
                if !self.overflow {
                    let text = String::from_utf8_lossy(&self.pending);
                    if let Some(url) = text.trim().strip_prefix("dsh web: ") {
                        if valid_web_url(url) && url.len() + 11 < self.limit as usize {
                            self.header = format!("dsh web: {url}\n").into_bytes();
                        }
                    }
                }
                self.pending.clear();
                self.overflow = false;
            }
        }
    }

    fn write(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        self.observe_auth(bytes);
        while !bytes.is_empty() {
            if self.size >= self.limit {
                self.rotate()?;
            }
            let count = bytes.len().min((self.limit - self.size) as usize);
            self.file.as_mut().unwrap().write_all(&bytes[..count])?;
            self.size += count as u64;
            bytes = &bytes[count..];
        }
        Ok(())
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file = None;
        remove_if_present(&log_path(&self.path, LOG_COPIES - 1))?;
        for index in (0..LOG_COPIES - 1).rev() {
            match fs::rename(log_path(&self.path, index), log_path(&self.path, index + 1)) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                result => result?,
            }
        }
        let mut file = File::create(&self.path)?;
        file.write_all(&self.header)?;
        self.size = self.header.len() as u64;
        self.file = Some(file);
        Ok(())
    }
}

fn log_path(path: &Path, index: usize) -> PathBuf {
    if index == 0 {
        path.to_path_buf()
    } else {
        path.with_extension(format!("log.{index}"))
    }
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn trim_old_log(path: &Path, limit: u64) -> io::Result<()> {
    let mut file = match OpenOptions::new().read(true).write(true).open(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        result => result?,
    };
    let length = file.metadata()?.len();
    if length <= limit {
        return Ok(());
    }
    let mut offset = 0;
    let mut buffer = [0u8; 8192];
    while offset < limit {
        let count = buffer.len().min((limit - offset) as usize);
        file.seek(SeekFrom::Start(length - limit + offset))?;
        file.read_exact(&mut buffer[..count])?;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(&buffer[..count])?;
        offset += count as u64;
    }
    file.set_len(limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::time::{Duration, Instant};

    struct TestDirectory(PathBuf);
    impl TestDirectory {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
                ".tmp-log-{}-{}-{}",
                std::process::id(),
                crate::transaction_nonce(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TestDirectory {
        fn drop(&mut self) {
            assert!(self.0.canonicalize().unwrap().starts_with(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .canonicalize()
                    .unwrap()
            ));
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn rotation_bounds_files_and_preserves_the_retained_tail() {
        let root = TestDirectory::new();
        let file = root.0.join("dsh.out.log");
        fs::write(&file, b"old".repeat(300)).unwrap();
        fs::write(log_path(&file, LOG_COPIES), b"excess archive").unwrap();
        let mut log = RotatingLog::open(file.clone(), 256, false).unwrap();
        assert_eq!(fs::metadata(&file).unwrap().len(), 256);
        assert!(!log_path(&file, LOG_COPIES).exists());
        let bytes: Vec<u8> = (0..1950).map(|index| (index % 251) as u8).collect();
        log.write(&bytes).unwrap();
        drop(log);
        assert_eq!(fs::read_dir(&root.0).unwrap().count(), LOG_COPIES);
        let mut tail = Vec::new();
        for index in (0..LOG_COPIES).rev() {
            let path = log_path(&file, index);
            assert!(fs::metadata(&path).unwrap().len() <= 256);
            tail.extend(fs::read(path).unwrap());
        }
        assert_eq!(tail, bytes[bytes.len() - tail.len()..]);
    }

    #[test]
    fn output_fixture() {
        let Ok(mode) = env::var("DSH_LOG_FIXTURE") else {
            return;
        };
        if mode == "fail" {
            eprintln!("isolated DSH startup failure");
            std::process::exit(7);
        }
        let mut out = io::stdout().lock();
        let mut err = io::stderr().lock();
        writeln!(out, "{}", "中".repeat(9000)).unwrap();
        write!(out, "dsh web: http://127.0.0.1:3080/?tok").unwrap();
        out.flush().unwrap();
        thread::sleep(Duration::from_millis(20));
        writeln!(out, "en=isolated-rotation-token").unwrap();
        for _ in 0..6144 {
            out.write_all(&[b'x'; 1024]).unwrap();
            err.write_all(&[b'e'; 1024]).unwrap();
        }
        writeln!(out, "BURST-END").unwrap();
        writeln!(err, "BURST-END").unwrap();
        out.flush().unwrap();
        err.flush().unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !Path::new(&mode).exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn live_output_rotates_both_streams_and_keeps_authentication() {
        let root = TestDirectory::new();
        let stop = root.0.join("stop");
        let mut command = hidden_command(env::current_exe().unwrap());
        command
            .args(["--exact", "log_relay::tests::output_fixture", "--nocapture"])
            .env("DSH_LOG_FIXTURE", &stop);
        let logs = root.0.clone();
        let worker = thread::spawn(move || collect(&mut command, &logs));
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut ready = false;
        while Instant::now() < deadline {
            ready = ["out", "err"].iter().all(|kind| {
                fs::read(root.0.join(format!("dsh.{kind}.log")))
                    .is_ok_and(|bytes| bytes.ends_with(b"BURST-END\n"))
            });
            if ready {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let active = fs::read(root.0.join("dsh.out.log")).unwrap_or_default();
        let limits_hold = ["out", "err"].iter().all(|kind| {
            let file = root.0.join(format!("dsh.{kind}.log"));
            log_path(&file, 1).exists()
                && (0..LOG_COPIES).all(|index| {
                    fs::metadata(log_path(&file, index))
                        .map_or(true, |value| value.len() <= LOG_LIMIT)
                })
        });
        // Signal shutdown and join before assertions so a failed check leaves no process.
        fs::write(stop, b"stop").unwrap();
        let result = worker.join().unwrap().unwrap();
        assert!(ready && limits_hold);
        assert!(
            active.starts_with(b"dsh web: http://127.0.0.1:3080/?token=isolated-rotation-token\n")
        );
        assert_eq!(result, 0);
    }

    #[test]
    fn worker_preserves_startup_failure_and_is_not_a_public_action() {
        let root = TestDirectory::new();
        let mut command = hidden_command(env::current_exe().unwrap());
        command
            .args(["--exact", "log_relay::tests::output_fixture", "--nocapture"])
            .env("DSH_LOG_FIXTURE", "fail");
        assert_eq!(collect(&mut command, &root.0).unwrap(), 7);
        assert!(fs::read_to_string(root.0.join("dsh.err.log"))
            .unwrap()
            .contains("isolated DSH startup failure"));
        let mut args: Vec<String> = [
            "launcher",
            "--dsh-log-worker",
            "root",
            "node",
            "entry",
            "web",
            "--no-open",
            "--host",
            "127.0.0.1",
            "--port",
            "3080",
        ]
        .map(str::to_owned)
        .into();
        assert!(is_worker(&args));
        assert!(crate::parse_action(&args).is_err());
        let mut with_patch = args.clone();
        with_patch.splice(
            6..6,
            [
                "--patch".to_owned(),
                "D:\\测试目录\\plugin-startup.patch.json".to_owned(),
            ],
        );
        assert!(is_worker(&with_patch));
        assert!(crate::parse_action(&with_patch).is_err());
        with_patch[6] = "--unexpected".to_owned();
        assert!(!is_worker(&with_patch));
        args.push("extra".to_owned());
        assert!(!is_worker(&args));
    }
}
