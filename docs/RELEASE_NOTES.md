# Pinset 发布说明

本文档记录 Pinset 各版本已经交付的主要变化。未来范围与开发状态请参阅
[Plans](PLANS.md)，计划中的功能不会提前写成已发布能力。

## v0.1.0-alpha.1

- 版本日期：2026-08-11
- 发布阶段：Alpha 预发布
- 许可证：MIT License
- Pinset CLI 产物：Linux x64、Windows x64、macOS Apple Silicon
- Node.js 运行时目标：Windows x64、Linux x64、macOS x64、macOS arm64
- GitHub Release：[v0.1.0-alpha.1](https://github.com/Future-Element/pinset/releases/tag/v0.1.0-alpha.1)

### 更新内容

- 发布 Node-first MVP，支持用精确稳定版本 `node@x.y.z` 初始化、锁定和安装项目 Node.js。
- 新增可提交的 `pinset.toml` 与 `pinset.lock`；锁文件包含四个平台的 canonical 产物、
  SHA-256 和验证信息，当前机器只安装对应目标。
- `pinset use` 从 Node 官方 HTTPS `SHASUMS256.txt` 获取可信哈希，并在配置和锁文件落盘
  后安装当前平台产物。
- `pinset install --locked` 在配置与锁不一致时提前失败，适合克隆项目和 CI 复现。
- 安装器支持 ZIP 与 TAR.XZ，执行路径穿越、非法归档链接、展开限制、必要文件和哈希校验，
  只有完整安装才会原子提交到最终目录。
- 重复安装已验证版本时直接复用，不重复下载。
- 新增 `pinset current`、`pinset which`、`pinset exec` 和 `pinset doctor`，用于查看当前项目
  选择、真实可执行文件、显式运行和只读环境诊断。
- 新增独立轻量 `pinset-shim`，可为 `node`、`npm`、`npx` 和 `corepack` 创建用户目录入口。
- npm、npx 和 corepack 子进程会获得所选 Node 命令目录，支持 `/usr/bin/env node` 启动方式。
- 新增本机安装源管理：

  ```shell
  pinset source list [provider]
  pinset source add <provider> <alias> --base-url <url>
  pinset source use <provider> <alias>
  pinset source fallback <provider> <aliases...>
  pinset source remove <provider> <alias>
  ```

- 内置 `official` 源不可覆盖或删除；自定义源默认要求 HTTPS，只有显式
  `--allow-insecure` 才允许受信任局域网 HTTP。
- 只有网络类失败可以按用户配置尝试下一来源；哈希失败立即停止，不通过换源掩盖异常。
- 修复 Node 官方校验清单中带嵌套目录的合法条目无法解析的问题。
- 支持 Node Unix TAR.XZ 中经过边界验证的相对符号链接，并拒绝绝对、越界或悬空链接。
- GitHub Actions 在 PR 与 `main` 执行格式、Clippy、workspace 测试、安装器离线测试和 shim
  依赖约束检查。
- 版本标签自动构建 Linux x64、Windows x64 和 macOS arm64 归档，生成 `SHA256SUMS` 并
  发布 GitHub Release。
- 仓库以 MIT License 公开发布，并启用主分支质量门禁和安全报告渠道。

### Release 资产

```text
install.sh
pinset-linux-x86_64.tar.gz
pinset-windows-x86_64.zip
pinset-macos-aarch64.tar.gz
SHA256SUMS
```

Release 发布后已重新下载全部资产并复算 SHA-256；Linux 归档也已通过公开 URL 的临时
WSL `curl | sh` 安装与 `pinset --version` 验证。CI 构建通过不等同于所有平台均已完成
真实 Node 安装验收。

### Linux x64 / macOS Apple Silicon 安装

当前版本是预发布版，必须使用固定版本 URL：

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/Future-Element/pinset/releases/download/v0.1.0-alpha.1/install.sh |
  sh -s -- --version 0.1.0-alpha.1
```

默认安装到 `$HOME/.local/bin`。安装器：

- 不使用 `sudo`；
- 不修改 shell profile 或 PATH；
- 不安装 Node.js；
- 强制校验 Release 的 SHA-256；
- 只接受包含 `pinset` 与 `pinset-shim` 的预期归档。

安装后按当前 shell 配置 PATH，例如 Bash：

```bash
export PATH="$HOME/.local/bin:$PATH"
pinset --version
```

### 项目使用

```bash
cd /path/to/project
pinset init
pinset use node@24.0.0
pinset current
pinset which node
pinset exec -- node --version
pinset exec -- npm --version
```

建议提交：

```text
pinset.toml
pinset.lock
```

只生成配置和锁、不下载当前平台运行时：

```bash
pinset use node@24.0.0 --no-install
```

之后可以根据已有锁执行：

```bash
pinset install --locked
```

### 说明与已知限制

- 当前只支持项目级 Node 选择，尚未正式支持 `pinset use node@... --global`。
- 当前只接受完整精确版本 `x.y.z`，不支持 `node@24`、`lts`、`latest` 或 `current`。
- Python 和 Flutter provider 尚未发布。
- macOS x64 Node 产物可以写入项目锁，但本版本没有发布 macOS Intel Pinset CLI 归档。
- curl 安装器当前支持 Linux x64 和 macOS Apple Silicon；Windows 使用 Release ZIP。
- shim 必须由用户显式安装到自有目录并加入 PATH；Pinset 不自动编辑 shell profile。
- 当前 shim 需要项目配置；正式全局选择和安全系统 PATH 透传计划在 alpha.2 完成。
- 第一次生成项目锁仍需访问 Node 官方 HTTPS 校验清单。已有锁可以在受限网络中通过用户
  批准的镜像执行锁定安装。
- Node 信任值当前来自官方 HTTPS `SHASUMS256.txt`；PGP 验签属于稳定版前的供应链增强。
- 项目不维护第三方 Homebrew Tap 或 Scoop Bucket。
- 本版本没有配置格式迁移；这是 Pinset 的首个公开预发布版本。

完整命令、配置、架构、使用和故障排查见 [PRD](PRD.md#17-当前版本使用指南)。
