# Pinset PRD

文档版本：`v0.1.0-beta.1`
产品阶段：`Node-first Beta`
更新时间：`2026-08-12`

## 1. 产品简介

Pinset 是一个本地优先、跨平台的运行时版本管理 CLI。它用一致的命令完成版本选择、锁定、安装、执行、镜像、缓存和诊断，减少在 nvm/fnm、uv、FVM 等工具之间切换的成本。

后续可以增加其他 Provider。`v0.1.0-beta.1` 只支持 Node.js，Python 和 Flutter 暂不纳入。

### 1.1 目标

- 一套一致命令管理全局和项目运行时；
- 项目配置与精确锁文件可提交、可复现、可解释；
- 下载、安装、缓存和卸载有可靠的安全检查；
- 不接管 shell profile，不覆盖外部管理器，不隐藏回退；
- 架构对 Provider 开放，安装器本身保持运行时中立。

### 1.2 目标用户

- 同时维护多个 Node 项目的开发者；
- Windows、WSL、Linux 和 macOS 混合环境用户；
- 需要国内镜像、企业内网镜像或离线安装的团队；
- 希望把运行时版本随项目提交并在 CI 中复现的团队；
- 后续需要统一管理多种语言运行时的个人或组织。

### 1.3 本版本不做

- 不在 Beta 接入 Python、Flutter、Java 等新 Provider；
- 不替代 npm、pnpm、yarn 等包管理器；
- 不管理项目依赖包或全局 npm 包；
- 不自动修改 shell profile、系统 PATH、IDE 配置或系统注册表；
- 不维护第三方 Homebrew Tap、Scoop Bucket 或其他外部包仓库；
- 不删除 nvm、fnm、Volta、系统 Node 或用户文件；
- 不提供后台服务、GUI、账号同步或云端状态。

## 2. 设计约定

### 2.1 可预测

项目配置优先于 Pinset 全局配置，全局配置优先于系统 PATH。项目或全局已经选择了版本但本机没有安装时，Pinset 直接报错，不会换成其他 Node。

### 2.2 精确锁定

用户可以输入主版本、主次版本、LTS 或 Current；锁文件必须保存精确版本、目标平台、归档、校验值和校验来源。

### 2.3 本地优先

配置、状态、安装和缓存全部保存在本机。项目文件可通过普通版本控制同步，不依赖 Pinset 服务器。

### 2.4 安全默认

- 默认只接受 HTTPS 下载源；
- 自定义源默认只替换归档传输，不替换官方校验元数据；
- SHA-256 不匹配立即停止，不继续 fallback；
- 安装在临时目录完成校验和解压后再原子提交；
- 不覆盖无法证明由 Pinset 所有的命令或目录；
- 只删除经过路径和所有权检查的 Pinset 文件。

### 2.5 运行时中立

curl 安装器只安装 `pinset` 与 `pinset-shim`。Node Provider 在用户选择或安装 Node 时注册 `node`、`npm`、`npx`、`corepack`；未来其他 Provider 使用同一注册模型。

## 3. 支持矩阵

### 3.1 Pinset Release

| 平台 | Beta 状态 | 归档 |
| --- | --- | --- |
| Linux x64 | 支持 | `pinset-linux-x86_64.tar.gz` |
| Windows x64 | 支持 | `pinset-windows-x86_64.zip` |
| macOS Apple Silicon | 支持 | `pinset-macos-aarch64.tar.gz` |
| Linux arm64 | 未发布 | 可从源码构建，尚无正式归档 |
| macOS Intel | 未发布 | 可从源码构建，尚无正式归档 |

### 3.2 Node 官方归档目标

| 目标 | 状态 |
| --- | --- |
| `windows-x86_64` | 支持 ZIP |
| `linux-x86_64` | 支持 TAR.XZ |
| `macos-x86_64` | Provider 支持 |
| `macos-aarch64` | Provider 支持 |

WSL 使用 Linux 归档，不能运行 Windows `.exe`，也不能复用 Windows 中的 Node 安装。

## 4. 配置和数据

### 4.1 项目配置 `pinset.toml`

表达用户意图，可提交：

```toml
schema = 1

[tools]
node = "24"
```

### 4.2 项目锁文件 `pinset.lock`

表达解析结果，可提交。至少包含：

- schema；
- Provider 和精确版本；
- 每个目标平台的归档 URL、格式和 SHA-256；
- 校验元数据来源；
- 生成信息。

`install --locked` 必须拒绝配置与锁文件不一致的状态。

### 4.3 全局状态

位于 `PINSET_HOME/state/global.toml` 和 `global.lock`。其数据结构与项目选择模型一致，但不会在 `$HOME` 或当前目录伪造项目文件。

### 4.4 Provider

Provider 声明：

- 支持的选择器；
- 官方版本元数据；
- 平台归档和必须路径；
- 运行时命令集合；
- 安装、验证和命令路由规则。

Node Provider 声明 `node`、`npm`、`npx`、`corepack`。

### 4.5 通用命令路由

`pinset-shim` 是与运行时无关的轻量调度器。命令入口只负责把命令名、当前目录和环境交给 Pinset 的统一解析逻辑，不复制网络、解压或 Provider 业务代码。

### 4.6 安装源

安装源是本机设置，保存在 `PINSET_HOME/sources.toml`，不进入项目锁文件。每个 Provider 包含：

- 内置不可修改的 `official`；
- 一个 active 来源；
- 零个或多个有序 fallback；
- 自定义源的 URL、HTTP 例外和元数据信任标记。

### 4.7 下载缓存

- 完整归档：`PINSET_HOME/downloads/sha256/<hash>.archive`；
- 断点文件：`PINSET_HOME/downloads/partial/<hash>.part`；
- 相同 SHA-256 跨来源、跨项目复用；
- 未知文件和非普通文件不会被当作缓存处理。

## 5. Node 功能

### 5.1 初始化

`pinset init` 在当前目录创建最小 `pinset.toml`。如果文件已经存在，命令会报错，不会覆盖。

### 5.2 版本选择

接受：

- 精确版本 `node@x.y.z`；
- 主版本 `node@x`；
- 主次版本 `node@x.y`；
- `node@lts`；
- `node@current`。

浮动选择必须访问当前可信元数据索引，并写入精确锁定结果。

### 5.3 全局版本

```shell
pinset global node@24
pinset global
pinset use node@24 --global
```

要求：

- 设置时默认安装；
- `--no-install` 只写选择和锁；
- 无参数只读显示全局选择，并在项目覆盖存在时解释最终生效版本；
- 不在当前目录创建项目文件。

### 5.4 项目版本

```shell
pinset use node@22
pinset install --locked
pinset unset node
```

要求：

- 使用最近的上级 `pinset.toml`；
- 同时更新配置与锁文件；
- 默认安装当前目标；
- `unset` 只移除选择，不卸载运行时；
- 离开项目后自动恢复全局选择。

### 5.5 独立安装和一次性执行

```shell
pinset install node@20
pinset exec node@20.19.0 -- node --version
```

独立安装不修改项目或全局选择。一次性执行只接受已经安装的精确版本，也不会修改持久设置。

### 5.6 统一解析

`current`、`which`、`exec`、shim 和 `doctor` 必须使用同一解析函数：

1. 最近项目选择；
2. Pinset 全局选择；
3. 排除 Pinset 路由入口后的系统 PATH。

命令子进程 PATH 必须把所选 Node 的 `bin` 目录置于前面，保证 npm 脚本中的 `/usr/bin/env node` 或 Windows wrapper 能找到同一版本。

### 5.7 命令路由

正常 `global`、`use`、`install` 成功后自动准备 Node Provider 的四个入口。要求：

- Unix 使用可验证的符号链接；
- Windows 使用内容和所有权可验证的 `.cmd` wrapper；
- 一组入口写入前先验证全部目标；
- 任一同名文件非 Pinset 所有时整组停止；
- 不覆盖外部管理器或用户命令；
- `activate` 只输出当前 shell 的 PATH 调整，不写 profile；
- `shim install` 用于修复路由，日常使用不需要先运行它。

### 5.8 版本列表

```shell
pinset list node
pinset list node --available
```

本地列表读取安装收据，可用版本列表读取可信版本索引。损坏或缺少收据的目录不会显示为 Pinset 安装。

### 5.9 安装事务

安装流程：

1. 验证工具、版本和目标路径段；
2. 对 `tool + version + target` 获取跨进程文件锁；
3. 检查已有安装收据；
4. 检查内容寻址缓存；
5. 按来源下载或断点续传；
6. 校验下载大小和 SHA-256；
7. 在随机临时目录安全解压；
8. 验证必需命令；
9. 写完整安装收据；
10. 原子发布到最终目录。

ZIP/TAR.XZ 必须拒绝路径穿越、绝对路径、特殊文件、逃逸符号链接、冲突路径、条目数超限和展开大小超限。

### 5.10 下载体验

- TTY 在一行内刷新进度；
- 进度宽度根据终端列数和 Unicode 显示宽度调整；
- 长文件名中间截断；
- 最后一列留空，避免终端自动换行；
- 非 TTY 只输出有限的开始/完成事件；
- 断点续传验证 `Content-Range`；
- 服务器忽略 Range 时从头安全下载；
- 校验失败删除不可信断点文件。

### 5.11 镜像和回退

普通镜像只替换归档 URL，SHA-256 仍从 Node 官方 HTTPS `SHASUMS256.txt` 获取。使用 `--trust-metadata` 后，自定义 HTTPS 镜像也可以提供 `index.json` 和 SHASUMS，`source list` 会标记这类来源。

`--allow-insecure` 只用于可信的 HTTP 内网服务，不能与 `--trust-metadata` 同时使用。

只有网络和传输错误可以 fallback。大小限制、哈希不匹配、格式错误和安全解压错误必须停止。

### 5.12 离线缓存导入

```shell
pinset cache import <archive> --sha256 <hash>
```

要求：

- 输入必须是普通非符号链接文件；
- 限制最大输入大小；
- 流式复制并计算 SHA-256；
- 不匹配时不写缓存；
- 使用临时文件和 no-clobber 原子提交；
- 并发提交已存在相同哈希时重新验证；
- `cache clean` 同时清理识别的完整归档与断点文件，保留未知项。

### 5.13 卸载

单版本卸载只接受精确版本并验证 Pinset 收据。项目或全局仍引用时默认拒绝；`--force` 只允许引用暂时失效，不能跳过路径和所有权保护。

完整卸载脚本：

- 默认要求二次确认；
- 支持 dry-run；
- 删除 Pinset CLI、通用路由、受管 Provider 入口和整个标准 `PINSET_HOME`；
- 自定义 `PINSET_HOME` 需要额外授权；
- 不扫描项目，不改 profile，不删外部管理器或系统运行时。

### 5.14 迁移

检测 `.nvmrc`、`.node-version`、Volta、asdf 和 mise。`--dry-run` 只查看结果；`--apply` 写入 Pinset 并保留旧文件。存在冲突时由 `--from` 选择来源。

### 5.15 诊断

`doctor` 和 schema 1 的 `doctor --json` 报告：

- `PINSET_HOME`；
- 项目和全局配置/锁文件；
- 生效来源和安装状态；
- 四个 Node 命令的 PATH 候选和遮挡；
- Pinset 路由所有权；
- 旧 shim 和已知旧管理器；
- 可执行的修复建议。

诊断命令只读，不修改状态。

## 6. 国际化

Beta 支持：

- `en`；
- `zh-CN`。

无子命令的 `pinset --lang <lang>` 持久保存到 `PINSET_HOME/settings.toml`；有子命令时只覆盖当前进程。`PINSET_LANG` 可做环境级临时覆盖。

正常提示、帮助、诊断、常见错误和进度信息应接入统一目录；底层操作系统错误、路径、URL 和哈希保留原值。

## 7. 安全和供应链

### 7.1 Node 归档

- 从可信元数据源获得预期 SHA-256；
- 下载源不得改变锁文件中的预期哈希；
- 所有归档在解压前校验；
- Beta 尚未验证 Node 上游 `SHASUMS256.txt.sig` 的 OpenPGP 签名，计划在稳定版加入。

### 7.2 Pinset Release

- tag 必须与 workspace 版本和安装器默认版本一致；
- Linux、Windows、macOS 在独立 Runner 构建；
- Release 归档包含且只包含 CLI 与通用 shim；
- Release 发布 SHA256SUMS；
- 发布 CycloneDX JSON SBOM；
- GitHub Actions 为归档和发布元数据生成构建来源证明；
- Release tag 使用维护者签名；
- GitHub Actions 引用固定完整 commit SHA。

### 7.3 密钥和凭据

- URL 写入收据前移除用户名、密码、query 和 fragment；
- 日志不输出 GitHub token、镜像凭据或代理密码；
- Pinset 不保存包管理器凭据。

## 8. 架构

### 8.1 Workspace

- `pinset-core`：配置、锁文件、Provider、来源、安装、缓存、解析和安全检查；
- `pinset-cli`：命令解析、交互、国际化、进度和用户输出；
- `pinset-shim`：轻量命令调度入口。

### 8.2 数据目录

默认：

- Linux/macOS/WSL：`$XDG_DATA_HOME/pinset` 或 `$HOME/.local/share/pinset`；
- Windows：`%LOCALAPPDATA%\pinset`。

测试和 CI 使用随机临时 `PINSET_HOME`。除完整卸载的专项测试外，不删除真实用户目录。

## 9. Beta 验收

### 9.1 本地检查

- `cargo fmt --all -- --check`；
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`；
- `cargo test --workspace --all-features`；
- 锁定 release build；
- POSIX 安装/卸载脚本隔离测试；
- PowerShell 卸载脚本隔离测试；
- shim 轻依赖检查；
- `git diff --check`。

这些检查只使用临时目录、假归档和本地服务，不在开发机安装真实 Node。

### 9.2 Release 检查

每个支持平台必须在隔离 Runner 中：

- 构建 locked release 二进制；
- 设置中文并验证持久化；
- 安装一个全局 Node 和一个项目 Node；
- 验证项目覆盖与离开项目后的全局恢复；
- 验证 `pinset exec` 下的 node/npm/npx/corepack；
- 验证 PATH 直接调用的 node/npm/npx/corepack；
- 运行安装器/卸载器隔离测试；
- 生成并发布归档、校验、SBOM 和来源证明。

任一平台失败都不发布 Release。

## 10. 稳定版前待办

`v0.1.0` 计划完成：

- 验证 Node 上游 SHASUMS OpenPGP 签名，并定义密钥轮换策略；
- 冻结 schema 1 兼容承诺和迁移策略；
- 完成 Beta 用户在代理、镜像、离线和旧管理器迁移场景的反馈修复；
- 决定 Linux arm64 与 macOS Intel 正式归档策略；
- 完成发布回滚、撤回和安全公告流程；
- Node 稳定版完成前不增加新 Provider。

Python 和 Flutter 排在 `v0.1.0` 之后。实现时复用现有 Provider、来源、缓存、路由和安全机制，不在 CLI 中增加语言专用的零散逻辑。
