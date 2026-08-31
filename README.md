# DSH Launcher

`dsh-launcher.exe` 是面向 Windows 的原生 DSH 服务控制器。

当前应用版本以 `Cargo.toml` 的 `[package].version` 为唯一来源；版本和发布规则见 [`VERSIONING.md`](VERSIONING.md)，变更记录见 [`CHANGELOG.md`](CHANGELOG.md)。

This project targets local DSH lifecycle management: install/update, start, restart, stop, and open the Web UI. Version 0.2.0 ships as a lightweight Windows x64 portable launcher for Windows 10/11; the runtime is installed on demand when the user chooses `验证并修复运行时`.

## Main window

Launching the executable opens a visible `DSH 控制中心` window with these actions:

- 启动 dsh
- 重启 dsh
- 关闭 dsh
- 更新 dsh
- 打开 Web UI
- 打开数据目录
- 验证并修复运行时
- 诊断详情、取消和退出

Closing the window keeps the launcher in the notification area. The first close explains this behavior; launching the executable again restores the existing window instead of silently exiting. The tray menu exposes service actions, `打开 Web UI`, `打开数据目录`, `更新 DSH`, `更新启动器`, `移入旧数据回收站`, and exit. The Web UI health state refreshes every five seconds while the launcher is open, and the tray icon is recreated after Explorer restarts.

The launcher uses native Win32 controls, a path-scoped single-instance mutex, and worker threads for long-running actions. The same executable directory remains single-instance, while separate portable copies can run side by side during migration or acceptance testing. It does not open a console window for DSH or upgrade commands. `--action` command-line mode attaches to a caller console and rebinds input, output, and error streams; normal launch without arguments remains a windowed application. The main panel supports Tab, Enter, Space, Esc, visible focus, high contrast, light/dark system themes, and 100%/150%/200% DPI changes.

The data root is strict: a normal launch requires `portable.flag` and `runtime-manifest.json` beside the EXE; the writable `data` directory is created there on first launch. There is no `%LOCALAPPDATA%` fallback, environment-variable fallback, or scan of other drives. `--data-dir` remains only as a compatibility parser and is rejected unless it resolves to that exact sibling `data` directory. Therefore, putting the complete directory under `D:\Apps\DSH-Launcher` keeps runtime, npm packages/cache, profile, logs, state, updates, and temporary files on D:. If the directory is on the system drive, the main window warns the user to stop DSH and move the whole directory to D:. The operating system may still write its own Prefetch or event data outside the application root.

The `DSH-Launcher-Portable-x64.zip` deliberately contains no Node.js, PowerShell 7, or DSH package. It launches immediately and keeps the download small; the first `验证并修复运行时` action downloads and verifies Node.js and DSH into `data\updates` before promotion. Windows 10/11 system `powershell.exe` is used for the small launcher-side download and archive operations, so PowerShell 7 is not duplicated in the package. A disconnected first-run installation is not supported because no offline runtime archive is distributed; prepare the runtime while connected, then move the complete portable directory if offline use is required. Do not run the launcher as Administrator: it intentionally uses the current user's DSH profile.

The launcher can stop a DSH service started by the launcher or by another local method only after verifying that the `3080` listener's command line contains the DSH package entry point and the `web` command. An explicit `--port` argument must match `3080`; omitting it is accepted because the verified process is the listener on `3080`. It refuses to terminate an unverified process. If port `3080` is occupied by another service, release it before starting the Windows-native service. The GUI can offer a transactional copy of old `%LOCALAPPDATA%\DSH-Runtime`, `%LOCALAPPDATA%\npm-global`, and `%USERPROFILE%\.dsh` data; each file is checked by size and SHA-256, failures roll back the target, and the old source is never auto-deleted.

DSH stdout and stderr are retained in `<data-root>\logs\dsh-launcher-native.out.log` and `dsh-launcher-native.err.log`; logs rotate by type at five files of 5 MiB each. A launch failure reports the stage, log path, and final diagnostic lines. The upgrade action compares full SemVer and refuses downgrade, installs the exact candidate with a lockfile, blocks high/critical npm audit findings, starts it on port `3081`, and checks both the Web UI and `dsh-quota` JSON endpoint. Only that already-verified directory replaces the data-root package. A failed promotion or restart restores the prior version and, when needed, restarts the prior service.

## Build

```powershell
cargo fmt --check --all --manifest-path Cargo.toml
cargo test --locked --manifest-path Cargo.toml
cargo clippy --locked --all-targets --manifest-path Cargo.toml -- -D warnings
cargo build --locked --release --manifest-path Cargo.toml --target x86_64-pc-windows-gnu --bins
target\x86_64-pc-windows-gnu\release\embed_icon.exe target\x86_64-pc-windows-gnu\release\dsh-launcher.exe
```

The release executable is:

`target\x86_64-pc-windows-gnu\release\dsh-launcher.exe`

The release output is not copied to the desktop automatically; choose any writable directory, preferably on the data drive.

The icon embedder writes the official DeepSeek whale mark as custom multi-size blue and black icons into the release executable. The tray icon uses DeepSeek blue (`#4D6BFE`) while DSH is healthy and switches to the black whale mark (`#000000`) when DSH is unavailable, stopped, or unhealthy. The `--action start|restart|stop|upgrade|launcher-update|repair|migrate|open|data` command-line modes are included for automation and verification; normal launch without arguments opens the main window.

Command-line actions accept `--data-dir D:\path\to\data` only when it is the exact sibling data directory. The `DSH-Launcher-Portable-x64.zip` Release asset is the supported lightweight cross-device package; extract it as a complete directory and keep `portable.flag` and `runtime-manifest.json` beside the EXE. After the first connected runtime repair, the complete portable directory can be moved to another device; no separate runtime archive is distributed.

## 启动器自身更新

托盘菜单中的 `更新启动器` 会读取本项目的公开 GitHub Release。只有 Release 版本严格高于当前版本时才会执行更新；程序会下载并校验以下两个固定资产：

- `DSH-Launcher.exe`
- `DSH-Launcher.exe.sha256`

校验通过后，启动器把更新暂存到便携 `data\updates`，记录父进程、事务令牌、目标路径和旧哈希，复制自身为更新助手，退出当前进程，由助手以同盘原子替换并保留按事务命名的备份；新版本必须完成窗口、托盘、数据清单初始化并写入健康握手，助手才删除备份。恢复、删除或清理失败会返回真实错误。网络失败、版本不高或 SHA-256 不匹配都不会修改当前 EXE。命令行等价入口为 `--action launcher-update`。

推送符合 `vMAJOR.MINOR.PATCH` 的标签会触发 `.github/workflows/release.yml`：它执行版本、格式、Clippy、测试、Windows x64 release build、图标资源、轻量 ZIP 白名单、SBOM 和 manifest 门禁，先创建草稿 Release，重新下载并校验全部资产后才公开。公开资产为 `DSH-Launcher.exe`、两个校验文件、`DSH-Launcher-Portable-x64.zip`、`release-manifest.json` 和 `sbom.spdx.json`，并明确标注未签名。应用能够更新到后续启动器 Release，但不会把 DSH 包更新误认为启动器更新。
