# DSH启动器

`DSH-Launcher.exe` 是 Windows 10/11 x64 的轻量原生 DSH 启动器。0.3.1 只保留启动/停止、打开 Web UI 和手动更新 DSH；普通启动只检查本机，不联网。

## 使用

下载并完整解压 `DSH-Launcher-Portable-x64.zip`，然后运行 `DSH-Launcher.exe`。`portable.flag` 和 `runtime-manifest.json` 必须与 EXE 同目录，启动器管理的数据固定写入同目录的 `data`。若把完整目录放在 D 盘，DSH 管理副本、配置、缓存、日志和更新暂存也会留在 D 盘。

首页最多显示三个操作按钮：

- 主按钮按状态显示“启动 DSH”“停止 DSH”“安装 DSH”“重新安装 DSH”或“安装 Node.js”；只有下载和预检阶段显示“取消”。
- “打开 Web UI”仅在已验证为 DSH 页面且 Web 服务健康时可用。
- “更新 DSH”仅在已发现 DSH 且 npm 可用时显示并启用。

页脚“检查启动器更新”只比较 GitHub Release 版本；发现新版后打开官方 Release 页面，不下载或替换当前 EXE。托盘固定为“打开面板”“启动/停止 DSH”“打开 Web UI”“退出”。窗口关闭时立即隐藏并以非阻塞系统通知说明托盘操作；隐藏后停止 15 秒健康检查，不会周期运行 `netstat.exe`。

## 本机发现与首次安装

发现顺序是：

1. `data\npm-global` 中的启动器管理 DSH。
2. 当前 Windows 用户 npm 全局目录中的系统 DSH。

启动器复用本机 `node.exe` 和 `npm.cmd`，不下载、校验或解压 Node.js。缺少 Node.js 时只打开 [Node.js 官方下载页](https://nodejs.org/en/download)。有 Node.js/npm 但没有 DSH 时，用户确认确切版本和目标路径后，启动器才从官方 npm registry 安装到 `data\npm-global`。

安装 Node.js 后，请在启动器托盘菜单选择“退出”，再重新运行启动器，以读取新的安装路径。关闭窗口只会隐藏面板。

系统 DSH 直接使用当前 Windows 用户配置。由启动器首次安装的 DSH 使用 `data\profile`。系统 DSH 更新为启动器管理副本时不会修改系统安装，并继续使用 Windows 用户配置。

## DSH 手动更新

“更新 DSH”读取官方包的全部版本并按完整 SemVer 比较，因此会识别未进入 `latest` 标签的预发布版本，例如 `0.1.2-alpha.4`，且不会降级。

候选版本使用以下约束安装到同盘 `data\updates`：

```text
--ignore-scripts --omit=dev --no-audit --no-fund
```

启动器校验包名、精确版本、`lib/bin.js` 和全部直接运行依赖后，交换完整的管理 npm 前缀。更新前运行中的 DSH 会在提交前停止；提交成功后固定保留最新版本，即使启动失败也不会降级。启动失败时主按钮允许重新安装同一最新版。短暂隔离的旧目录只会被清理，崩溃恢复也不会重新启用旧版本。

对于要求 Web 认证的 DSH 版本，启动器读取并重新验证 DSH 自己输出的本地认证地址用于健康检查和“打开 Web UI”，但状态栏、托盘和通知不会显示认证令牌。普通占用 3080 端口的 HTTP 程序不会被识别为 DSH。

由启动器启动 DSH 后，可在浏览器地址栏直接输入 `http://localhost:3080/` 或 `http://127.0.0.1:3080/`，也可使用这两个地址的书签。启动器为本机主动打开的首页完成 DSH 自身的令牌与 Cookie 交换，无需复制认证地址。两个地址各自建立 Cookie；API、跨站请求、嵌入页面及未经识别的请求仍由 DSH 执行原认证。无法提供 Fetch Metadata 的浏览器应使用“打开 Web UI”。此入口只存在于启动器启动的 DSH 进程内，退出面板后仍随 DSH 进程保留。

认证日志采用增量读取，日志增长或中文字符不会使有效地址丢失。认证失效时，界面说明停止后重新启动的恢复方法。更新中断后会重新校验候选包及运行依赖；候选缺失或损坏时进入重装流程，保留配置模式，不会把空目录当作恢复成功或重新启用旧版。

下载前要求更新盘至少有 512 MiB 可用空间。npm 缓存、更新暂存和命令临时文件都位于便携目录；缓存会在查询或安装后清空。

DSH 输出位于 `data\logs\dsh.out.log` 和 `data\logs\dsh.err.log`，运行期间持续轮转，每类最多五个文件（当前文件加四个归档），每个不超过 5 MiB。退出启动器面板后日志仍持续受限；轮转保留当前认证入口，重新打开面板后仍可使用 Web UI。错误对话框显示摘要和日志路径，可按 Ctrl+C 复制详情。

## CLI

公开命令仅有：

```powershell
DSH-Launcher.exe --action start
DSH-Launcher.exe --action stop
DSH-Launcher.exe --action upgrade
DSH-Launcher.exe --action open
```

ZIP 内的 `dshctl.cmd` 提供相同的四个动作，阻塞等待完成、输出中文结果并返回退出码。0.3.1 不再接受 `restart`、`repair`、`migrate`、`launcher-update`、`data`、`--data-dir` 或自更新内部参数。

## 构建与发布门禁

```powershell
cargo fmt --check --all --manifest-path Cargo.toml
cargo test --locked --manifest-path Cargo.toml
cargo clippy --locked --all-targets --manifest-path Cargo.toml -- -D warnings
cargo build --locked --release --manifest-path Cargo.toml --target x86_64-pc-windows-gnu --bins
target\x86_64-pc-windows-gnu\release\embed_icon.exe target\x86_64-pc-windows-gnu\release\dsh-launcher.exe
```

图标嵌入器把 DeepSeek 官方鲸鱼标志和 Windows VERSIONINFO 写入 EXE：DSH 健康时使用官方蓝色 `#4D6BFE`，停止、不可用或异常时使用黑色。界面使用适配简体中文的 `Microsoft YaHei UI`，并保留 DPI、高对比度、键盘焦点和 Explorer 重启后的托盘恢复。

Release 固定发布六个资产：EXE、EXE SHA-256、便携 ZIP、ZIP SHA-256、`release-manifest.json` 和 Rust SPDX SBOM。发布物未签名并明确记录 `authenticode_status: unsigned`。技术型单独 EXE 不作为完整便携包使用。

版本与发布规则见 [VERSIONING.md](VERSIONING.md)，变更记录见 [CHANGELOG.md](CHANGELOG.md)。
