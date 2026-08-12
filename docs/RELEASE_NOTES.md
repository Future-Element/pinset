# Pinset 发布说明

本文档记录 Pinset 各版本已经交付的主要变化。未来范围与开发状态请参阅
[Plans](PLANS.md)，计划中的功能不会提前写成已发布能力。

## v0.1.0-alpha.5

- 版本日期：2026-08-12
- 发布阶段：Alpha 预发布
- 许可证：MIT License
- Pinset CLI 产物：Linux x64、Windows x64、macOS Apple Silicon
- Node.js 运行时目标：Windows x64、Linux x64、macOS x64、macOS arm64
- GitHub Release：[v0.1.0-alpha.5](https://github.com/Future-Element/pinset/releases/tag/v0.1.0-alpha.5)

### 更新内容

- Node Provider 统一声明 `node`、`npm`、`npx`、`corepack`；选择或安装 Node 后自动注册整组
  命令入口，正常使用不再要求手动执行 `shim install`。
- curl 安装器继续只安装 `pinset` 与通用 `pinset-shim`，不创建语言命令、不安装 Node，确保
  后续扩展其他运行时时安装器仍保持中立。
- 新增 `pinset install node@<selector>`，可预装精确或浮动选择器对应版本，不修改项目配置或
  全局默认。
- 新增 `pinset unset node [--global]`，只清除项目或全局选择并回退上一层，不隐式删除安装和缓存。
- 新增 `pinset import --apply [--from <source>] [--global] [--no-install]`，在用户明确确认后导入
  nvm、`.node-version`、Volta、asdf 或 mise 配置；冲突时拒绝猜测，原文件始终保留。
- 新增 `pinset shim migrate --provider node`，为 alpha.4 用户显式准备新路由目录，同时报告并
  保留旧 shim。
- 新增 Bash、Zsh、Fish 和 PowerShell 的 `pinset activate <shell>`，仅输出通用 PATH 操作，
  不写 shell profile，也不包含 Node 专用逻辑。
- Windows 使用稳定 `.cmd` 路由，Unix 使用指向通用调度器的符号链接；升级调度器后无需复制
  四份命令二进制。
- `doctor` 与 `doctor --json` 现在检查四个 Node 命令的 PATH 候选、生效顺序、Pinset 所有权、
  外部遮蔽、旧 shim 与旧管理器，并给出可逆建议。

### 验证范围

- Windows 格式、严格 Clippy、117 项锁定 workspace 全功能测试、锁定 Release 构建和差异检查通过。
- WSL 在独立 `/tmp` target 中完成同一组 117 项 Linux 测试；离线安装器假归档测试通过。
- 自动化只使用临时目录、假运行时、本地构造归档和假 shim；未在开发机下载或安装真实 Node。
- main 质量门禁 [#31576332545](https://github.com/Future-Element/pinset/actions/runs/31576332545)
  通过；标签 Release 工作流
  [#31576490916](https://github.com/Future-Element/pinset/actions/runs/31576490916) 完成同一质量门禁、
  Linux x64、Windows x64、macOS Apple Silicon 构建并发布五个预期资产。
- 发布后重新下载全部资产，`SHA256SUMS` 四项复算一致；Linux/macOS 归档各只包含 `pinset` 与
  `pinset-shim`，Windows ZIP 只包含两个 `.exe`。Windows CLI 输出 `pinset 0.1.0-alpha.5`，
  并确认 `activate`、`global`、`import`、`install`、`shim`、`unset` 命令存在。

### Linux x64 / macOS Apple Silicon 安装

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/Future-Element/pinset/releases/download/v0.1.0-alpha.5/install.sh |
  sh -s -- --version 0.1.0-alpha.5
```

默认安装到 `$HOME/.local/bin`。安装器不使用 `sudo`、不修改 shell profile 或 PATH、
不安装 Node，并强制校验 Release 的 SHA-256。Windows 使用 Release ZIP。

### 已知限制

- Python、Flutter、PGP 验签和中央包管理器分发尚未交付。
- Pinset 不自动修改 shell profile、系统 PATH、IDE 配置，也不覆盖其他管理器的同名命令。
- macOS x64 Node 可写入锁文件，但当前没有 macOS Intel Pinset CLI 归档。

## v0.1.0-alpha.4

- 版本日期：2026-08-12
- 发布阶段：Alpha 预发布
- 许可证：MIT License
- Pinset CLI 产物：Linux x64、Windows x64、macOS Apple Silicon
- Node.js 运行时目标：Windows x64、Linux x64、macOS x64、macOS arm64
- GitHub Release：[v0.1.0-alpha.4](https://github.com/Future-Element/pinset/releases/tag/v0.1.0-alpha.4)

### 更新内容

- 新增 `pinset global [node@selector]` 一等入口：不带参数只读查看全局默认，带参数设置、
  锁定并默认安装；原有 `pinset use node@... --global` 保持兼容。
- 在项目 Node 覆盖全局默认时，`pinset global` 明确显示两个版本和生效项目配置路径；
  `pinset current` 继续显示当前目录最终生效结果。
- `pinset shim install` 默认寻找与 CLI 同目录的 `pinset-shim` 并安装到
  `$PINSET_HOME/shims`；新增只读 `pinset shim path`，仍不覆盖已有文件或修改 PATH。
- Node 归档下载在交互终端显示进度条、百分比、已下载/总大小；非交互环境输出简洁状态，
  缓存命中不显示伪下载进度。
- 下载完成提示仅在 SHA-256 校验成功后出现；网络、大小限制或哈希错误保持失败关闭。
- README 新增从安装、PATH、语言、全局/项目版本、shim、进度、镜像到诊断的完整使用说明。

### 验证范围

- 本地格式、严格 Clippy、102 项 workspace 全功能测试、锁定 Release 构建和差异检查通过。
- 自动化使用临时目录、本地假 HTTP、构造归档、假 shim 和假运行时，不安装真实 Node。
- 安装器测试覆盖已知内容长度的开始、字节推进、校验完成事件，以及缓存、回退和失败关闭。
- GitHub Release 工作流完成质量门禁和 Linux x64、Windows x64、macOS Apple Silicon
  构建，并发布五个预期资产。
- 发布后重新下载全部资产，`SHA256SUMS` 四项复算一致；三个归档只包含预期 CLI 与 shim，
  Windows CLI 输出 `pinset 0.1.0-alpha.4` 并确认包含 `global` 命令。

### Linux x64 / macOS Apple Silicon 安装

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/Future-Element/pinset/releases/download/v0.1.0-alpha.4/install.sh |
  sh -s -- --version 0.1.0-alpha.4
```

默认安装到 `$HOME/.local/bin`。安装器不使用 `sudo`、不修改 shell profile 或 PATH、
不安装 Node，并强制校验 Release 的 SHA-256。Windows 使用 Release ZIP。

### 已知限制

- Python、Flutter、PGP 验签和中央包管理器分发尚未交付。
- Pinset 不自动修改 shell profile；直接调用 `node` 前仍需把 shim 目录加入 PATH。
- macOS x64 Node 可写入锁文件，但当前没有 macOS Intel Pinset CLI 归档。

## v0.1.0-alpha.3

- 版本日期：2026-08-12
- 发布阶段：Alpha 预发布
- 许可证：MIT License
- Pinset CLI 产物：Linux x64、Windows x64、macOS Apple Silicon
- Node.js 运行时目标：Windows x64、Linux x64、macOS x64、macOS arm64
- GitHub Release：[v0.1.0-alpha.3](https://github.com/Future-Element/pinset/releases/tag/v0.1.0-alpha.3)

### 更新内容

- 支持 `node@24`、`node@24.12`、`node@lts` 和 `node@current`，解析后写入精确版本。
- 新增本地安装列表与显式官方可用版本查询。
- 新增精确版本安全卸载；当前项目或全局仍引用时默认拒绝，`--force` 仍不越过 Pinset
  数据所有权边界。
- 新增按 SHA-256 内容寻址的下载缓存、校验后离线复用、缓存查看和安全清理。
- 新增 `source test node [alias]`，只读检查版本索引和最新稳定版 SHASUMS。
- 新增 `doctor --json` schema 1 机器可读诊断。
- 新增 `.nvmrc`、`.node-version`、Volta、asdf 和 mise 的 `import --dry-run` 只读检测。
- 新增 `exec node@<selector> -- <command>` 一次性选择已安装版本，不修改项目或全局状态。
- 所有新增正常提示、帮助和常见错误均支持英文与简体中文。

### 验证范围

- 97 项 workspace 测试、本地格式、严格 Clippy、锁定 Release 构建、下载缓存并发复验和
  shim 轻依赖检查通过。
- GitHub Release 工作流完成版本/tag 校验、质量门禁和 Linux x64、Windows x64、macOS
  Apple Silicon 构建，并发布五个预期资产。
- 发布后重新下载全部资产，`SHA256SUMS` 四项复算一致；Linux/macOS 归档和 Windows ZIP
  只包含预期的 CLI 与 shim，Windows CLI 输出 `pinset 0.1.0-alpha.3`。
- 自动化仅使用临时目录、假运行时、本地 HTTP 和构造归档；没有安装真实 Node。
- 真实目标系统功能验收由用户在发布后自行执行。

### Linux x64 / macOS Apple Silicon 安装

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/Future-Element/pinset/releases/download/v0.1.0-alpha.3/install.sh |
  sh -s -- --version 0.1.0-alpha.3
```

默认安装到 `$HOME/.local/bin`。安装器不使用 `sudo`、不修改 shell profile 或 PATH、
不安装 Node，并强制校验 Release 的 SHA-256。Windows 使用 Release ZIP。

### 已知限制

- Python、Flutter、PGP 验签和中央包管理器分发尚未交付。
- 一次性 `exec node@<selector>` 只执行已经安装的版本，不隐式安装运行时。
- `import --dry-run` 只检测当前目录，不自动写入或删除旧管理器配置。
- macOS x64 Node 可写入锁文件，但当前没有 macOS Intel Pinset CLI 归档。

## v0.1.0-alpha.2

- 版本日期：2026-08-12
- 发布阶段：Alpha 预发布
- 许可证：MIT License
- Pinset CLI 产物：Linux x64、Windows x64、macOS Apple Silicon
- Node.js 运行时目标：Windows x64、Linux x64、macOS x64、macOS arm64
- GitHub Release：[v0.1.0-alpha.2](https://github.com/Future-Element/pinset/releases/tag/v0.1.0-alpha.2)

### 更新内容

- 新增正式全局 Node 选择：

  ```shell
  pinset use node@24.0.0 --global
  pinset install --global --locked
  ```

  全局状态保存在 `$PINSET_HOME/state/global.toml` 和 `global.lock`，不会写入当前项目或
  `$HOME/pinset.toml`。
- 统一 Node 解析顺序：最近项目配置优先，其次是 Pinset 全局选择，最后才是排除 Pinset
  shim 后的系统 PATH。离开项目后自动恢复全局选择。
- 项目或全局已经声明 Node、但对应安装缺失或损坏时失败关闭，不会静默使用低优先级版本。
- `current`、`which`、`exec`、`doctor` 和 shim 共用同一来源感知解析结果，可区分
  `project`、`global` 与 `system`。
- `node`、`npm`、`npx` 和 `corepack` 使用同一所选 Node 命令目录；子进程 PATH 支持
  `/usr/bin/env node` 启动方式。
- 系统 Node 透传会排除 Pinset shim 目录和当前 shim 文件，防止直接或间接递归。
- 新增英文与简体中文界面。无子命令时持久保存语言：

  ```shell
  pinset --lang zh-CN
  ```

  带子命令时只覆盖本次输出：`pinset --lang en doctor`。也可使用 `PINSET_LANG` 覆盖当前
  进程；持久设置保存在 `$PINSET_HOME/settings.toml`，不修改项目文件。
- 正常提示、诊断、帮助、参数错误和常见运行错误均接入同一语言目录；路径、版本和底层系统
  错误仍保留原始技术值。
- 保持 alpha.1 的 `schema = 1` 项目配置和锁文件兼容，不要求迁移现有项目。
- CI 新增仅手动触发的隔离 Ubuntu 真实运行时验收，普通 PR 不重复下载真实 Node。

### 验证范围

- 78 项 workspace 测试、格式、Clippy、锁定 Release 构建、curl 安装器离线测试与 shim
  轻依赖检查通过。
- Linux x64、Windows x64、macOS arm64 Release 归档构建通过。
- 一次性 Ubuntu Runner 使用临时 `PINSET_HOME` 安装 Node 24.0.0 全局版本与 Node 22.0.0
  项目版本，验证项目覆盖、离开项目恢复全局版本、node/npm/corepack 和中文提示。
- GitHub Release 工作流完成版本/tag 校验、质量门禁和三平台构建，并发布五个预期资产。
- 发布后重新下载全部资产，`SHA256SUMS` 四项复算一致；Linux/macOS 归档和 Windows ZIP
  只包含预期的 CLI 与 shim，Windows CLI 输出 `pinset 0.1.0-alpha.2`。
- 未在开发者 Windows 或 WSL 环境安装真实运行时。
- 尚未完成 Windows 与 macOS 真实 Node 的人工流程验收；三平台构建通过不等同于三平台
  真实运行时安装均已验收。

### Linux x64 / macOS Apple Silicon 安装

该版本发布后使用固定预发布 URL：

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/Future-Element/pinset/releases/download/v0.1.0-alpha.2/install.sh |
  sh -s -- --version 0.1.0-alpha.2
```

默认安装到 `$HOME/.local/bin`。安装器不使用 `sudo`、不修改 shell profile 或 PATH、
不安装 Node，并强制校验 Release 的 SHA-256。

### 全局与项目版本

设置全局版本：

```shell
pinset use node@24.0.0 --global
pinset current
pinset exec -- node --version
```

在项目中覆盖全局版本：

```shell
mkdir demo && cd demo
pinset init
pinset use node@22.0.0
pinset current
pinset exec -- node --version
```

离开项目后，`pinset current` 与 shim 会重新解析到全局版本。若没有项目和全局声明，Pinset
才会安全透传系统 PATH 中的相应 Node 命令。

### 说明与已知限制

- 仍只接受完整精确版本 `x.y.z`，不支持 `node@24`、`lts`、`latest` 或 `current`。
- Python、Flutter、版本列表、卸载、浮动选择器和旧管理器导入尚未发布。
- macOS x64 Node 产物可以写入锁文件，但本版本没有 macOS Intel Pinset CLI 归档。
- curl 安装器支持 Linux x64 和 macOS Apple Silicon；Windows 使用 Release ZIP。
- shim 仍需用户显式安装到自有目录并加入 PATH；Pinset 不修改 shell profile。
- `$HOME/pinset.toml` 仍按普通最近项目配置处理，不会自动迁移或删除。
- 系统 PATH 透传限于当前 Node-first 命令集合，不代表任意工具或 Shell activation。
- Node 信任值仍来自官方 HTTPS `SHASUMS256.txt`；PGP 验签属于稳定版前的供应链增强。
- 项目不维护第三方 Homebrew Tap、Scoop Bucket 或社区镜像 preset。

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
