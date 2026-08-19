# Pinset

[English](README.md) | [简体中文](README.zh-CN.md)

[![CI](https://github.com/Future-Element/pinset/actions/workflows/ci.yml/badge.svg)](https://github.com/Future-Element/pinset/actions/workflows/ci.yml)
[![GitHub Release](https://img.shields.io/github/v/release/Future-Element/pinset?include_prereleases)](https://github.com/Future-Element/pinset/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Pinset 是一个行为可预测、理解项目边界的多语言运行时版本管理器。

它通过统一的配置与锁文件模型管理 Node.js、pnpm、Bun、Go、Python、Java、Rust、.NET 和 Flutter/Dart。传统版本文件只会由显式的 `detect` 与 `import` 迁移命令读取，不会成为日常运行时解析的隐式回退来源。

## 核心特性

- 使用一个 CLI 管理全局默认版本与可复现的项目版本。
- `pinset.toml` 保留用户请求的选择器，`pinset.lock` 记录精确版本和各平台制品。
- 项目内默认严格路由，只有显式策略才允许继承全局版本或回退系统命令。
- 通过 `current --explain`、`which --explain` 与 `doctor` 解释完整解析过程。
- 通过一个轻量、与运行时无关的 shim 路由命令。
- 解析摘要之前，先使用内嵌 OpenPGP 信任根验证 Node.js 发布清单。
- Provider 完整性校验、安全解压、原子安装、带所有权检查的卸载，以及内容寻址下载缓存。
- 原生支持英文和简体中文输出、自动化用 JSON schema 1 与 Shell 补全。
- 支持项目所有的 Python `.venv`，无需激活 Shell 环境。
- 支持只读检测和显式导入仓库内的传统版本配置。

## 安装与升级

### Linux 与 macOS

安装脚本会下载匹配平台的 GitHub Release 归档，校验其在 `SHA256SUMS` 中的记录，并默认把 `pinset` 与 `pinset-shim` 安装到 `~/.local/bin`。

```sh
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/install.sh | sh
```

在当前 Shell 中，把该目录放到系统运行时目录之前：

```sh
export PATH="$HOME/.local/bin:$PATH"
```

再次运行同一安装脚本即可升级。也可以安装指定版本或使用其他绝对目录：

```sh
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/install.sh | sh -s -- --version 1.5.1
PINSET_INSTALL_DIR=/opt/pinset/bin sh install.sh
```

### Windows

从 [GitHub Releases](https://github.com/Future-Element/pinset/releases) 下载 `pinset-windows-x86_64.zip`，把 `pinset.exe` 与 `pinset-shim.exe` 解压到长期保留的目录，再把该目录放到用户 `PATH` 的靠前位置。

```powershell
$pinsetBin = 'C:\Tools\pinset'
$env:PATH = "$pinsetBin;$env:PATH"
pinset --version
```

升级时，用新版 Release 归档中的两个二进制一起替换旧文件。Windows 与 WSL 的安装相互独立。

### 手动下载

[Releases 页面](https://github.com/Future-Element/pinset/releases)会发布全部受支持归档、`SHA256SUMS`、SBOM 与构建来源证明。解压前请先验证归档摘要。

## Shell 初始化

Pinset 不会修改 Shell 配置文件。请自行加入对应初始化命令，使 Pinset 路由目录拥有更高优先级。

### Bash

```sh
eval "$(pinset activate bash)"
```

### Zsh

```sh
eval "$(pinset activate zsh)"
```

### Fish

```fish
pinset activate fish | source
```

### PowerShell

```powershell
pinset activate powershell | Out-String | Invoke-Expression
```

把对应命令加入 Shell 配置文件即可在后续会话中生效。如果其他运行时管理器或系统命令位于 `PATH` 更前面，请运行 `pinset doctor`。

## Shell 补全

按需生成补全脚本：

```sh
pinset completions bash > ~/.local/share/bash-completion/completions/pinset
pinset completions zsh > "${fpath[1]}/_pinset"
pinset completions fish > ~/.config/fish/completions/pinset.fish
```

```powershell
pinset completions powershell | Out-String | Invoke-Expression
```

## 快速开始

设置 Pinset 项目之外使用的全局默认版本：

```sh
pinset global node@lts
pinset global pnpm@latest
pinset global go@1.25
pinset current node
```

为项目独立锁定版本：

```sh
mkdir example && cd example
pinset init
pinset use node@22
pinset use pnpm@10
pinset use python@3.14
pinset install --locked
pinset exec -- node --version
```

请提交 `pinset.toml` 和 `pinset.lock`。`latest`、`lts`、`stable` 或版本前缀等选择器会保留在 `pinset.toml`，锁文件则记录其精确解析版本。`pinset outdated` 会区分“兼容选择器内可更新”与“必须修改选择器才能升级”，`pinset update --dry-run` 可预览兼容的锁更新。

schema 3 项目默认严格：只要存在 `pinset.toml`，未声明的工具就不会继承全局选择，也不会使用系统 `PATH`。需要时必须明确选择：

```toml
[policy]
inherit-global = true
system-fallback = false
boundary = "git"
```

默认解析边界是最近的 Git 根目录；只有父级配置自身明确设置 `boundary = "filesystem"` 时，才允许跨过 Git 边界。使用 `pinset current node --explain` 或 `pinset which node --explain` 可以查看每个候选，以及那些只作为显式迁移输入的传统配置文件。schema 1/2 状态仍可读取；可通过 `pinset migrate --dry-run` 预览，再运行 `pinset migrate` 完成仅格式层面的升级。

迁移现有仓库时，先在本地检查传统配置，再导入并安装其中无歧义的选择：

```sh
pinset detect --json
pinset import
```

`detect` 不联网、也不写文件。`import --no-install` 会写入 Pinset 配置和精确锁，但不下载运行时。扫描范围从工作目录到最近的 Git 仓库根目录，并且不会修改或删除来源文件。

查看可用版本与已安装版本：

```sh
pinset list node --available
pinset list pnpm --available
pinset list
```

## Provider 与平台

| Provider | 命令 | Windows x64 | Linux x64 | Linux ARM64 | macOS ARM64 |
| --- | --- | :---: | :---: | :---: | :---: |
| Node.js | `node`、`npm`、`npx`、`corepack` | ✓ | ✓ | ✓ | ✓ |
| pnpm | `pnpm` | ✓ | ✓ | ✓ | ✓ |
| Bun | `bun`、`bunx` | ✓ | ✓ | ✓ | ✓ |
| Go | `go`、`gofmt` | ✓ | ✓ | ✓ | ✓ |
| Python | `python`、`python3`、`pip`、`pip3` | ✓ | ✓ | ✓ | ✓ |
| Java（Temurin） | `java`、`javac`、`jar` 与 JDK 工具 | ✓ | ✓ | ✓ | ✓ |
| Rust stable | `rustc`、`cargo`、`rustdoc`、`rustfmt`、`clippy-driver` | ✓ | ✓ | ✓ | ✓ |
| .NET SDK | `dotnet` | ✓ | ✓ | ✓ | ✓ |
| Flutter / 内置 Dart | `flutter`、`dart` | ✓ | ✓ | — | ✓ |

Flutter 没有发布符合 Pinset 安装模型的官方 Linux ARM64 SDK 归档，因此 Pinset 会返回明确的不支持目标错误，不会回退到 x64。macOS Intel 不是 Pinset v1.0 的发布目标。

## 命令文档

请查阅完整的[中文命令文档](docs/commands.zh-CN.md)或[英文命令文档](docs/commands.md)。其中记录了每个命令及二级命令、状态修改、JSON 支持、退出码与常见错误。

产品定位与取舍见带官方来源的 [Pinset v1.5 横向对比](docs/comparison.zh-CN.md)。

## 未来规划

路线图只表达方向，不承诺具体版本或发布日期。

| 状态 | 方向 |
| --- | --- |
| Planned | 扩展锁审计、失败原因码与兼容性迁移诊断。 |
| Exploring | 评估通过 Homebrew、Winget、Scoop 等渠道进行官方分发。 |
| Exploring | 在上游运行时提供合适制品时扩展平台和架构。 |
| Exploring | 根据真实需求增加 Provider 或 Provider 扩展机制。 |
| Ongoing | 持续加强供应链验证、缓存行为与诊断能力。 |

## 卸载

需要所有权与引用检查时，请先通过 Pinset 删除运行时：

```sh
pinset uninstall node@22.0.0 --dry-run
pinset uninstall node@22.0.0
pinset prune --dry-run
```

完整卸载 Pinset 时，请从安装目录删除 `pinset` 和 `pinset-shim`，再删除自己加入 Shell 配置文件的初始化行。仅当你也要删除 Pinset 所有的运行时、缓存和全局状态时，才删除 `PINSET_HOME`（Unix 通常为 `~/.local/share/pinset`）。项目内的 `pinset.toml`、`pinset.lock` 与 `.venv` 不会自动删除。

## 贡献

提交变更前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。安全问题请按照 [SECURITY.md](SECURITY.md) 报告。

## 许可证

Pinset 使用 [MIT License](LICENSE)。
