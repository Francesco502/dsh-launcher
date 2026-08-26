# DSH Launcher

`dsh-launcher.exe` is a small native Windows control application for the Windows-native DeepSeek Harness installation.

当前应用版本以 `Cargo.toml` 的 `[package].version` 为唯一来源；版本和发布规则见 [`VERSIONING.md`](VERSIONING.md)，变更记录见 [`CHANGELOG.md`](CHANGELOG.md)。

This project targets local DSH lifecycle management: install/update, start, restart, stop, and open the Web UI. The current launcher requires the portable Node/npm runtime to be provisioned under `%LOCALAPPDATA%\DSH-Runtime`; its update flow stages and validates a package before promotion, while standalone first-run runtime provisioning is not yet exposed as a separate action.

## Main window

Launching the executable opens a visible `DSH 控制中心` window with these actions:

- 启动 dsh
- 重启 dsh
- 关闭 dsh
- 更新 dsh
- 打开 Web UI（托盘菜单）

Closing the window keeps the launcher in the notification area. Launching the executable again restores the existing window instead of silently exiting. The tray menu exposes the four service actions, `打开 Web UI`, `检查启动器更新`, and a full application exit command. The Web UI health state refreshes every five seconds while the launcher is open.

The launcher uses native Win32 controls, a single-instance mutex, and worker threads for long-running actions. It does not open a console window for DSH or upgrade commands. `--action` command-line mode attaches to a caller console when one is available; normal launch without arguments remains a windowed application.

It starts the official Windows-global `@deepseek-ai/dsh` with the portable runtime in `%LOCALAPPDATA%\DSH-Runtime`, using `%USERPROFILE%\.dsh` for settings and plugins. The launcher can stop a DSH service started by the launcher or by another local method only after verifying that the `3080` listener's command line contains the DSH package entry point and the `web` command. An explicit `--port` argument must match `3080`; omitting it is accepted because the verified process is the listener on `3080`. It refuses to terminate an unverified process. If port `3080` is occupied by another service, release it before starting the Windows-native service. Do not run the launcher as Administrator: it intentionally uses the current user's DSH profile.

DSH stdout and stderr are retained in `%USERPROFILE%\.dsh\logs\dsh-launcher-native.out.log` and `dsh-launcher-native.err.log`; a launch failure reports the log path and the final diagnostic lines. The upgrade action first installs the candidate into an isolated staging directory, starts it on port `3081`, and checks both the Web UI and `dsh-quota` configuration endpoint. Only a passing candidate replaces the Windows-global package. A failed promotion or restart restores the prior version and, when needed, restarts the prior service.

## Build

```powershell
cargo test --offline --manifest-path Cargo.toml
cargo build --offline --release --manifest-path Cargo.toml --target x86_64-pc-windows-gnu --bins
target\x86_64-pc-windows-gnu\release\embed_icon.exe target\x86_64-pc-windows-gnu\release\dsh-launcher.exe
```

The release executable is:

`target\x86_64-pc-windows-gnu\release\dsh-launcher.exe`

The packaged desktop copy is `%USERPROFILE%\Desktop\DSH-Launcher.exe`.

The icon embedder writes custom multi-size color and grayscale DSH icons into the release executable. The tray icon stays colored while DSH is healthy and switches to the grayscale resource when DSH is unavailable or unhealthy. The `--action start|restart|stop|upgrade|launcher-update|open` command-line modes are included for automation and verification; normal launch without arguments opens the main window.

## 启动器自身更新

托盘菜单中的 `检查启动器更新` 会读取本项目的公开 GitHub Release。只有 Release 版本高于当前版本时才会执行更新；程序会下载并校验以下两个固定资产：

- `DSH-Launcher.exe`
- `DSH-Launcher.exe.sha256`

校验通过后，启动器复制自身为临时更新助手，退出当前进程，由助手替换旧 EXE 并重新启动。网络失败、版本不高或 SHA-256 不匹配都不会修改当前 EXE。命令行等价入口为 `--action launcher-update`。

推送符合 `vMAJOR.MINOR.PATCH` 的标签会触发 `.github/workflows/release.yml`：它执行版本、格式、测试和 Windows release build 门禁，嵌入图标，生成 `release-manifest.json`，然后发布上述三个资产。应用能够更新到后续启动器 Release，但不会把 DSH 包更新误认为启动器更新。
