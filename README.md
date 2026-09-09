# DSH启动器

`DSH-Launcher.exe` 是 Windows 10/11 x64 的原生 DSH 启动器。0.5.0 使用 Slint 1.17.1 与现有 Rust 后端，提供启动/停止、重启、打开 Web UI、手动更新 DSH、插件选择和启动器自更新。界面编译进 EXE，使用 winit 与软件渲染，不依赖 WebView2；普通启动只检查本机，不联网。

界面采用白色背景、清晰文字和克制的强调色。插件列表、更新确认及错误详情都在同一窗口内显示。关闭面板会释放窗口和界面模型，托盘与后台任务继续运行；再次打开会重建面板并读取最新状态。

## 使用

下载并完整解压 `DSH-Launcher-Portable-x64.zip`，然后运行 `DSH-Launcher.exe`。`portable.flag` 和 `runtime-manifest.json` 必须与 EXE 同目录，启动器管理的数据固定写入同目录的 `data`。若把完整目录放在 D 盘，DSH 管理副本、配置、缓存、日志和更新暂存也会留在 D 盘。

首页提供以下操作：

- 主按钮按状态显示“启动 DSH”“停止 DSH”“安装 DSH”“重新安装 DSH”或“安装 Node.js”；启动等待、下载和预检阶段显示“取消”。
- “打开 Web UI”仅在已验证为 DSH 页面且 Web 服务健康时可用。
- “重启 DSH”在 DSH 正在运行且本机安装可用时启用；先停止旧进程再启动，失败时保留具体错误。重启期间禁用重复操作，成功时不弹系统通知。
- “更新 DSH”仅在已发现 DSH 且 npm 可用时显示并启用。
- “选择插件”列出已安装的第三方插件；运行时可保存勾选结果，下次启动生效；需要用户主动重启。不兼容的插件需要保持取消勾选，选择启用不会修复插件缺失的接口。

页脚“检查启动器更新”检查正式 GitHub Release；确认新版后下载并验证便携 ZIP、SHA-256 与发布清单，以事务替换四个程序文件并重新打开窗口。新版未就绪时恢复整组旧文件。已是最新版、取消或检查失败均保留窗口。0.3.3 首次升级仍需完整安装 0.4.0，一键更新支持后续版本。退出启动器保留 DSH；停止优先请求 DSH 释放资源，12 秒内未退出则清理已核实的进程树。托盘固定为“打开面板”“启动/停止 DSH”“打开 Web UI”“退出”。右上角关闭会释放面板与绘制资源，托盘继续运行；托盘右键“退出”才完全退出启动器，DSH 服务和日志助手继续运行，重新打开面板会检测已有服务。悬停只显示“DSH运行中”或“DSH未启动”；正常启动、停止和隐藏窗口均不弹系统通知。隐藏后停止 15 秒健康检查，不会周期运行 `netstat.exe`。

插件选择写入 `data\state\plugin-settings.json`，按便携/用户 profile 分开保存。启动器为本次启动生成 `plugin-startup.patch.json`，覆盖原 profile 的插件开关，原配置和依赖保持不变。通过启动器或其 CLI 启动时生效；绕过启动器直接运行 DSH 不应用此设置。插件市场中的开关可能与启动器选择不同，以启动器保存的选择为准。仅修改共享配置、没有独立加载入口的 bundle 会显示“不支持直接切换”。

## 本机发现与首次安装

发现顺序是：

1. `data\npm-global` 中的启动器管理 DSH。
2. 当前 Windows 用户 npm 全局目录中的系统 DSH。

启动器复用本机 `node.exe` 和 `npm.cmd`，不下载、校验或解压 Node.js。缺少 Node.js 时只打开 [Node.js 官方下载页](https://nodejs.org/en/download)。有 Node.js/npm 但没有 DSH 时，用户确认确切版本和目标路径后，启动器才从官方 npm registry 安装到 `data\npm-global`。

安装 Node.js 后，请在启动器托盘菜单选择“退出”，再重新运行启动器，以读取新的安装路径。关闭窗口会释放面板，托盘继续运行；重新打开时重建界面。

系统 DSH 直接使用当前 Windows 用户配置。由启动器首次安装的 DSH 使用 `data\profile`。系统 DSH 更新为启动器管理副本时不会修改系统安装，并继续使用 Windows 用户配置。

## DSH 手动更新

“更新 DSH”读取官方 npm 包的全部版本并按完整 SemVer 比较，包括未进入 `latest` 标签的预发布版本，且不会降级。0.4.1 起同时核对官方 GitHub 的 `dsh-v*` 源码发布；源码版本领先 npm 时，显示“源码已发布，npm 尚未提供”和可安装版本。GitHub 查询失败时明确说明核实范围，不阻断 npm 更新。查询只在用户点击安装或更新时执行。

例如 2026-09-07 核实的 `0.1.3-alpha.1` 已发布源码，但官方 npm 尚只有 `0.1.2-rc.1`。启动器不会将源码压缩包当成 npm 安装包，也不内置 pnpm 或源码构建环境；官方 npm 发布后，再次点击“更新 DSH”即可识别并安装。

候选版本使用以下约束安装到同盘 `data\updates`：

```text
--ignore-scripts --omit=dev --no-audit --no-fund
```

0.4.3 起，安装后实际加载内置会话持久化所需的原生依赖；缺失时仅对已识别的 `fs-ext` / `koffi` 执行构建，并为 npm 12 写入候选目录内的精确包版本脚本许可。源码构建需要本机已有 Python 和 Visual C++ 构建工具；启动器不自动安装编译工具。Node 头文件缓存保存在 `data\node-gyp`，临时文件仍在便携目录。构建或加载失败会中止候选，保留原安装和运行实例。同版本存在此类故障时，点击“更新 DSH”也会重新暂存并修复。

0.4.4 的重绘补丁未能在所有显示环境中解决首次透明问题。0.5.0 替换旧 Win32 自绘窗口和业务弹窗，使用不透明的 Slint 界面，并保留系统标题栏与鲸鱼图标。

## 插件诊断与恢复

单个插件缺失不会阻止插件页打开：列表保留包名和错误原因，并禁止保存不完整的依赖状态。启动预检仍严格检查必需插件，不会自动删除引用或跳过错误。

“修复依赖”展示原 profile、精确版本与安装来源，确认后才暂存和验证候选。registry 依赖优先使用锁文件；本地 `link:` 引用只重新连接原路径，不替换成公共包。原路径不存在、私有源不可用或版本无法验证时会给出错误。提交前要求 DSH 已停止，失败保留原配置及有效依赖。插件开关继续使用原 `plugin-settings.json` 格式，保存不会自动重启 DSH。

启动器校验包名、精确版本、`lib/bin.js`、直接运行依赖、原生模块实际加载、内置持久化组件导入、Node 要求、boot 接口和配置/插件入口后，交换完整的管理 npm 前缀。更新前运行中的 DSH 会在提交前停止；提交成功后固定保留最新版本，即使启动失败也不会降级。普通启动失败允许重试，只有验证到安装损坏时提供重新安装。短暂隔离的旧目录只会被清理，崩溃恢复也不会重新启用旧版本。

对于要求 Web 认证的 DSH 版本，启动器读取并重新验证 DSH 自己输出的本地认证地址用于健康检查和“打开 Web UI”，但状态栏、托盘和通知不会显示认证令牌。普通占用 3080 端口的 HTTP 程序不会被识别为 DSH。

由启动器启动 DSH 后，可在浏览器地址栏直接输入 `http://localhost:3080/` 或 `http://127.0.0.1:3080/`，也可使用这两个地址的书签。启动器为本机主动打开的首页完成 DSH 自身的令牌与 Cookie 交换，无需复制认证地址。两个地址各自建立 Cookie；API、跨站请求、嵌入页面及未经识别的请求仍由 DSH 执行原认证。无法提供 Fetch Metadata 的浏览器应使用“打开 Web UI”。此入口只存在于启动器启动的 DSH 进程内，退出面板后仍随 DSH 进程保留。

认证日志采用增量读取，日志增长或中文字符不会使有效地址丢失。认证失效时，界面说明停止后重新启动的恢复方法。更新中断后会重新校验候选包及运行依赖；候选缺失或损坏时进入重装流程，保留配置模式，不会把空目录当作恢复成功或重新启用旧版。

下载前要求更新盘至少有 512 MiB 可用空间。npm 缓存、更新暂存和命令临时文件都位于便携目录；缓存会在查询或安装后清空。

DSH 输出位于 `data\logs\dsh.out.log` 和 `data\logs\dsh.err.log`，运行期间持续轮转，每类最多五个文件（当前文件加四个归档），每个不超过 5 MiB。退出启动器面板后日志仍持续受限；轮转保留当前认证入口，重新打开面板后仍可使用 Web UI。错误对话框显示摘要和日志路径，可按 Ctrl+C 复制详情。

若启动失败来自插件加载，提示会列出涉及的插件；请更新或停用不兼容插件后重新启动。重新安装 DSH 不会更新用户 profile 中的第三方插件。停止操作在确认进程退出后显示“DSH 已停止”，是否使用强制进程树清理记录在 `launcher.log` 中。

## CLI

公开命令仅有：

```powershell
DSH-Launcher.exe --action start
DSH-Launcher.exe --action stop
DSH-Launcher.exe --action upgrade
DSH-Launcher.exe --action open
```

ZIP 内的 `dshctl.cmd` 提供相同的四个动作，阻塞等待完成、输出中文结果并返回退出码。从 0.3.1 起不再接受 `restart`、`repair`、`migrate`、`launcher-update`、`data`、`--data-dir` 。0.4.0 使用严格校验的私有自更新参数，不属于公开 CLI。

## 构建与发布门禁

```powershell
cargo fmt --check --all --manifest-path Cargo.toml
cargo test --locked --manifest-path Cargo.toml
cargo clippy --locked --all-targets --manifest-path Cargo.toml -- -D warnings
cargo build --locked --release --manifest-path Cargo.toml --target x86_64-pc-windows-gnu --bins
target\x86_64-pc-windows-gnu\release\embed_icon.exe target\x86_64-pc-windows-gnu\release\dsh-launcher.exe
```

图标嵌入器把 DeepSeek 官方鲸鱼标志和 Windows VERSIONINFO 写入 EXE：DSH 健康时使用官方蓝色 `#4D6BFE`，停止、不可用或异常时使用黑色。界面使用适配简体中文的 `Microsoft YaHei UI`，保留系统标题栏、缩放与可见键盘焦点。首次显示、实际 DPI 和托盘生命周期需要独立的 Windows 显示验收，编译或离屏截图不能代替这些检查。

Release 固定发布六个资产：EXE、EXE SHA-256、便携 ZIP、ZIP SHA-256、`release-manifest.json` 和 Rust SPDX SBOM。发布物未签名并明确记录 `authenticode_status: unsigned`。技术型单独 EXE 不作为完整便携包使用。

版本与发布规则见 [VERSIONING.md](VERSIONING.md)，变更记录见 [CHANGELOG.md](CHANGELOG.md)。

## 许可与第三方署名

项目自身代码使用 [MIT](LICENSE)。界面使用 [Slint](https://slint.dev)，按 [Slint Royalty-free License 2.0](licenses/Slint-Royalty-free-2.0.md) 使用；应用“关于”显示 Slint 提供的署名组件。Slint 及其依赖保持各自许可，发布资产中的 SPDX SBOM 列出 Rust 依赖版本。本项目的 MIT 许可不覆盖或重新授权这些第三方代码。
