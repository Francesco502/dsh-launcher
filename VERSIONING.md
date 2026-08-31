# dsh-launcher 版本与发布标准

本文档是 `dsh-launcher` 后续开发、版本管理和公开发布的规范。除非先修改本文档并同步修改发布校验脚本，否则不得引入另一套版本规则。

## 1. 唯一版本来源

- 唯一人工维护的版本来源是 `Cargo.toml` 的 `[package].version`。
- 程序内的 `APP_VERSION` 必须继续通过 `env!("CARGO_PKG_VERSION")` 获取，不得再维护第二个应用版本常量。
- `Cargo.lock` 中 `dsh-launcher` 的 package 版本由 Cargo 同步，发布前必须与 `Cargo.toml` 一致。
- README、窗口文案、发布说明和脚本中的版本号都不是版本来源；它们只能引用或验证 Cargo 版本。

发布前检查：

```powershell
cargo metadata --locked --no-deps --format-version 1
& .github\scripts\validate-release.ps1 -Tag vX.Y.Z
```

## 2. SemVer 规则

使用稳定版三段式语义化版本号 `MAJOR.MINOR.PATCH`，Git 标签必须严格为 `vMAJOR.MINOR.PATCH`。

- `MAJOR`：不兼容的命令行、配置、数据或用户工作流变更。
- `MINOR`：向后兼容的新功能或公开能力。
- `PATCH`：向后兼容的修复、稳定性改进、安全修复或兼容性修正。
- 仅文档、测试或内部重构且不改变发布物时，不强制创建版本；若需要对外发布，仍必须按完整流程递增版本。
- 当前启动器更新逻辑和发布工作流只支持稳定版三段式版本号。`-alpha`、`-beta`、`-rc` 等预发布版本在另行实现解析和更新策略前禁止使用。

版本号一旦发布即不可重用。禁止移动、覆盖或强制推送已经公开的版本标签；需要修复时创建下一个版本。

## 3. 变更记录标准

`CHANGELOG.md` 遵循 Keep a Changelog 的结构：

- `Unreleased` 保存尚未发布的用户可见变更。
- 发布时把已完成条目移到对应版本，并使用实际发布日期 `YYYY-MM-DD`。
- 优先使用 `Added`、`Changed`、`Deprecated`、`Removed`、`Fixed`、`Security` 分类。
- 条目描述用户能观察到的行为，不记录无关的实现细节或未经验证的性能承诺。

## 4. 发布前门禁

创建标签前，发布提交必须满足以下条件：

1. 工作区干净，代码已提交到 `main`。
2. `Cargo.toml`、`Cargo.lock`、`CHANGELOG.md` 和标签版本一致。
3. `cargo fmt --check --all` 通过。
4. `cargo test --locked` 和 `cargo clippy --locked --all-targets -- -D warnings` 通过。
5. Windows GNU release build 通过，`embed_icon.exe` 已把官方蓝色和黑色鲸鱼图标写入最终 EXE。
6. 轻量主 ZIP 通过顶层目录、`portable.flag`、无用户数据/凭据、SHA-256 和 SPDX SBOM 检查；主 ZIP 不得包含运行时目录，首次运行由用户在线执行运行时修复。
7. `.github\scripts\validate-release.ps1 -Tag vX.Y.Z -ReleaseDirectory dist` 通过。

发布门禁由 `.github/workflows/release.yml` 在 GitHub Actions 上重复执行；本地通过不能替代远程构建结果。

## 5. 标准发布流程

以 `vX.Y.Z` 为例：

1. 在 `Cargo.toml` 更新 `[package].version`。
2. 运行 Cargo 命令同步 `Cargo.lock`，确认根 package 版本相同。
3. 将本次用户可见变更从 `Unreleased` 移到 `## [X.Y.Z] - YYYY-MM-DD`。
4. 运行全部本地门禁，检查 `git diff --check`，提交版本变更。
5. 将发布提交推送到 `main`。
6. 创建并推送不可变的附注标签：

   ```powershell
   git tag -a vX.Y.Z -m "Release vX.Y.Z"
   git push origin vX.Y.Z
   ```

7. 标签推送触发 `.github/workflows/release.yml`。工作流会重新验证版本、测试、构建、嵌入图标、生成资产和 SPDX SBOM。
8. 工作流先创建草稿 Release，上传六个公开资产，重新下载并校验每个资产的哈希和 ZIP 内容，随后才把草稿转为公开状态；任一步失败都不会公开。
9. 发布后检查 Release 为非草稿状态，资产名称完整，manifest 中的提交、版本、运行时清单摘要、依赖锁摘要和 SHA-256 与本次构建相符。
10. 下载 `DSH-Launcher.exe` 并按 `.sha256` 校验；自更新功能只能使用官方仓库 Release 的固定技术资产。

工作流失败时，不得跳过门禁手工上传未知构建物。修复后应使用新的 PATCH 版本；已经创建的标签或 Release 不得复用。

## 6. Release 资产契约

每个稳定版 Release 必须发布以下六个固定名称的资产：

| 资产 | 用途 |
| --- | --- |
| `DSH-Launcher.exe` | 面向 Windows x86_64 GNU 目标的最终启动器，已嵌入图标 |
| `DSH-Launcher.exe.sha256` | 启动器 EXE 的 SHA-256，格式为 `<hash>  DSH-Launcher.exe` |
| `DSH-Launcher-Portable-x64.zip` | 轻量 Windows x64 主便携包，仅包含启动器、发布契约和 `portable.flag` |
| `DSH-Launcher-Portable-x64.zip.sha256` | 便携包 ZIP 的 SHA-256 |
| `release-manifest.json` | 机器可读的版本、提交、目标平台和资产校验信息 |
| `sbom.spdx.json` | SPDX 2.3 的 Rust 与按需下载运行时依赖清单 |

Release manifest 必须包含 `authenticode_status: "unsigned"`。`release-manifest.json` 本身不放入自己的 `assets` 哈希数组，因为这样会形成自引用；其余五个资产必须逐一列出并校验。正式 Release 的 `commit` 必须是标签提交的 40 位 SHA；未提交工作树只能在本地显式验证模式下使用 `local-working-tree` 占位值。

`release-manifest.json` 的当前 schema 为 `1`，至少包含：

```json
{
  "schema_version": 1,
  "project": "Francesco502/dsh-launcher",
  "version": "X.Y.Z",
  "tag": "vX.Y.Z",
  "commit": "<github-sha>",
  "target": "x86_64-pc-windows-gnu",
  "architecture": "x86_64",
  "authenticode_status": "unsigned",
  "runtime_manifest_sha256": "<runtime-manifest-hash>",
  "dependency_lock_sha256": "not_applicable",
  "assets": [
    { "name": "DSH-Launcher.exe", "sha256": "<hash>", "type": "application/vnd.microsoft.portable-executable" },
    { "name": "DSH-Launcher.exe.sha256", "sha256": "<hash>", "type": "text/plain" },
    { "name": "DSH-Launcher-Portable-x64.zip", "sha256": "<hash>", "type": "application/zip" },
    { "name": "DSH-Launcher-Portable-x64.zip.sha256", "sha256": "<hash>", "type": "text/plain" },
    { "name": "sbom.spdx.json", "sha256": "<hash>", "type": "application/spdx+json" }
  ]
}
```

新增字段必须向后兼容；改变字段含义时递增 `schema_version` 并同步修改校验与文档。

## 7. 回滚与修复

- 回滚运行版本时，应恢复上一个已验证的 Release 资产，不删除或重写版本标签。
- 发布后发现问题，先在 `Unreleased` 记录，再发布下一个 PATCH 版本。
- 若 GitHub Release 创建成功但资产不完整，保留事件记录并修复工作流；禁止用同一版本重新打标签掩盖构建问题。
- 发布资产只允许包含公开构建物、manifest 和校验信息；不得上传令牌、私钥、用户配置、日志或内部运维证据。
