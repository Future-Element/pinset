# Pinset

[English](README.md) | [简体中文](README.zh-CN.md)

[![CI](https://github.com/Future-Element/pinset/actions/workflows/ci.yml/badge.svg)](https://github.com/Future-Element/pinset/actions/workflows/ci.yml)
[![GitHub Release](https://img.shields.io/github/v/release/Future-Element/pinset?include_prereleases)](https://github.com/Future-Element/pinset/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Pinset 是一个行为可预测、理解项目边界的多语言运行时版本管理器。

它使用一份项目配置和一份精确锁文件管理 Node.js、pnpm、Bun、Go、Python、Java、Rust、.NET 与 Flutter/Dart。进入项目后可以直接运行 `node`、`python`、`cargo`、`flutter` 等命令；Pinset shim 会选择项目锁定的运行时，并在项目受信任时注入选定的 age 加密环境 profile。

```text
pinset.toml  ──用户意图、项目策略、环境 profile
     │
     ├── pinset.lock ──精确版本、平台制品、完整性信息
     │
     └── Pinset shim ──直接路由命令并按策略注入环境
```

## 为什么使用 Pinset

- **一个项目模型**：多语言运行时共用 `pinset.toml` 和 `pinset.lock`，不需要为每种语言叠加一个版本管理器。
- **可复现且可解释**：配置保留 `lts`、`stable`、版本前缀等选择意图，锁文件记录精确版本；`current --explain`、`which --explain` 和 `doctor` 解释最终选择。
- **项目内直接使用**：完成一次 Shell 初始化后，项目中的 `node`、`pnpm`、`python`、`cargo` 等命令自动路由，无需每次添加 `pinset exec`。
- **严格项目边界**：项目默认不继承全局版本，也不静默回退到系统 `PATH`；联网安装和传统版本文件导入都需要显式命令。
- **安全安装**：Provider 执行完整性校验、安全解压和原子安装，并通过所有权收据支持审计、修复、卸载和清理。
- **加密项目环境**：每个 profile 使用独立的 age 密文与 recipient；私钥保存在系统密钥库、口令保护的恢复文件或 CI Secret 中。
- **适合自动化**：提供稳定的 JSON schema 1、reason code、退出码、Shell 补全、离线锁审计和 GitHub Composite Action。

## 支持的 Provider

| Provider | 主要命令 | Windows x64 | Linux x64 | Linux ARM64 | macOS ARM64 |
| --- | --- | :---: | :---: | :---: | :---: |
| Node.js | `node`、`npm`、`npx`、`corepack` | ✓ | ✓ | ✓ | ✓ |
| pnpm | `pnpm` | ✓ | ✓ | ✓ | ✓ |
| Bun | `bun`、`bunx` | ✓ | ✓ | ✓ | ✓ |
| Go | `go`、`gofmt` | ✓ | ✓ | ✓ | ✓ |
| Python | `python`、`python3`、`pip`、`pip3` | ✓ | ✓ | ✓ | ✓ |
| Java（Temurin） | `java`、`javac`、`jar` 与 JDK 工具 | ✓ | ✓ | ✓ | ✓ |
| Rust stable | `rustc`、`cargo`、`rustdoc`、`rustfmt`、Clippy | ✓ | ✓ | ✓ | ✓ |
| .NET SDK | `dotnet` | ✓ | ✓ | ✓ | ✓ |
| Flutter / 内置 Dart | `flutter`、`dart` | ✓ | ✓ | — | ✓ |

Flutter 没有提供符合当前安装模型的官方 Linux ARM64 SDK 归档，因此 Pinset 会明确返回不支持，而不会下载 x64 制品。外部 Android SDK、Visual Studio Build Tools、Windows SDK 等系统依赖只由 `doctor` 诊断，不由 Pinset 安装。

## 安装目录是怎样组织的

安装目录中的 `node.cmd`、`cargo.cmd`、`flutter.cmd` 等文件只是很小的命令路由，不是完整 SDK。真实运行时按照 Provider、版本和平台分别保存在 `PINSET_HOME` 中：

```text
安装目录/
├── pinset(.exe)           CLI
├── pinset-shim(.exe)      轻量命令路由器
├── node(.cmd)             路由到项目选择的 Node.js
├── cargo(.cmd)            路由到项目选择的 Rust
└── ...                    其他内置 Provider 命令

PINSET_HOME/
├── installs/
│   ├── node/<版本>/<平台>/...
│   ├── rust/<版本>/<平台>/...
│   └── flutter/<版本>/<平台>/...
├── downloads/             内容寻址下载缓存
└── state/                 全局选择、信任记录等本机状态
```

因此命令都出现在同一个 PATH 目录是正常设计：这个目录负责稳定路由，SDK 本体仍彼此隔离。可以随时检查实际位置和安装内容：

```sh
pinset paths
pinset paths flutter
pinset list --long
pinset doctor --deep
```

## 安装

### Linux 与 macOS

安装器会下载与当前平台匹配的 GitHub Release，核对 `SHA256SUMS`，并默认安装到 `~/.local/bin`：

```sh
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"
```

安装指定版本或目录：

```sh
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/install.sh | sh -s -- --version 2.1.0
PINSET_INSTALL_DIR=/opt/pinset/bin sh install.sh
```

### Windows PowerShell

```powershell
Invoke-WebRequest https://raw.githubusercontent.com/Future-Element/pinset/main/install.ps1 -OutFile install.ps1
.\install.ps1
Remove-Item .\install.ps1
```

指定版本：

```powershell
.\install.ps1 -Version 2.1.0
```

Windows 与 WSL 是两个独立环境，需要分别安装。安装器只安装 Pinset 和所有内置命令路由，不会预先下载语言运行时。

也可以从 [GitHub Releases](https://github.com/Future-Element/pinset/releases) 手动下载归档。Release 同时提供 checksum、SBOM 与构建来源证明。

## Shell 初始化

Pinset 不会修改 Shell 配置文件。把对应命令加入自己的 Shell 配置，使 Pinset 的路由目录位于其他运行时管理器之前：

```sh
# Bash
eval "$(pinset activate bash)"

# Zsh
eval "$(pinset activate zsh)"
```

```fish
# Fish
pinset activate fish | source
```

```powershell
# PowerShell
pinset activate powershell | Out-String | Invoke-Expression
```

如需补全：

```sh
pinset completions bash
pinset completions zsh
pinset completions fish
```

```powershell
pinset completions powershell | Out-String | Invoke-Expression
```

## 快速开始

### 1. 设置全局默认版本

全局选择只在 Pinset 项目之外生效，或由项目策略显式继承：

```sh
pinset global node@lts pnpm@latest
pinset current node
```

### 2. 创建项目并锁定运行时

```sh
mkdir example && cd example
pinset init
pinset use node@24 pnpm@11 python@3.14 --no-install
pinset install --locked
pinset lock audit
```

提交生成的 `pinset.toml` 和 `pinset.lock`。项目成员 Clone 后只需要运行：

```sh
pinset install --locked
node --version
pnpm --version
python --version
```

`pinset.toml` 保存用户选择与策略，`pinset.lock` 保存精确版本和平台制品。项目配置使用 schema 4，运行时锁继续使用 schema 3；加密环境不会参与运行时制品解析。

### 3. 临时运行其他版本

不修改项目或全局选择：

```sh
pinset x node@22 -- node --version
```

### 4. 导入传统版本文件

Pinset 不会在日常解析中隐式读取 `.nvmrc`、`.node-version`、`.tool-versions` 等文件。迁移时需要显式检测和导入：

```sh
pinset detect --json
pinset import
```

`detect` 只读且不联网；`import` 不删除或修改来源文件。

## 项目策略

项目默认严格：未声明的工具不会继承全局选择，也不会静默使用系统命令。需要时在 `pinset.toml` 中显式调整：

```toml
schema = 4
project-id = "4c5652e4-0000-4000-8000-000000000000"

[policy]
inherit-global = false
system-fallback = false
boundary = "git"
verification-strength = "checksum"
minimum-release-age = "7d"

[tools]
node = "24"
pnpm = "11"
```

验证强度顺序为 `checksum < signed-checksum < provenance`。如果上游证据弱于项目要求，或无法取得发布年龄，Pinset 会失败关闭，不会静默降低策略。

## 加密项目环境

Pinset 2.0 管理项目范围、字符串类型的加密环境变量，但不定位为通用 Secrets Vault。每个 profile 是独立的 age 文件，并拥有独立 recipient。

### 初始化与直接运行

```sh
pinset migrate
pinset env init --profile development --auto --recovery ~/pinset-development-recovery.age
pinset env set DATABASE_URL --profile development
pinset env list --profile development
pinset trust add

# shim 自动选择运行时并注入 development profile
node app.js
```

重要规则：

- `env set` 默认隐藏输入，变量值不会出现在命令参数中。
- `env list` 只显示变量名；查看单个值需要交互式 `env reveal`。
- 没有 `auto-profile` 时，直接 shim 不自动注入环境。
- `PINSET_ENV_PROFILE=ci` 可显式选择 profile。
- `PINSET_ENV_DISABLE=1` 或 `pinset exec --no-env` 可关闭单次注入。
- 进程变量与密文变量同名时默认报错；也可显式使用 `process-wins` 或 `encrypted-wins`。
- 修改 recipient、profile 路径、自动 profile 或冲突策略后必须重新信任；只修改密文值不需要。
- 不会自动扫描 `.env`，也不会创建临时明文 `.env`。

### 导入现有 `.env`

如果项目已经使用 `.env`，可以显式把其中的变量迁移到某个加密 profile：

```sh
pinset env import --from .env --profile development
```

导入支持空值、注释、单/双引号和带引号多行值。同名变量会在目标 profile 中更新；`export`、变量插值、命令替换和 Shell 表达式会被拒绝。Pinset 不会自动查找或删除来源文件，确认迁移成功后仍需由用户自行处理原来的明文 `.env`。

### 换电脑

Clone 项目后，安装锁定运行时、导入恢复身份并重新信任：

```sh
pinset install --locked
pinset env identity import --from ~/pinset-development-recovery.age
pinset trust add
node app.js
```

恢复文件必须保存在仓库外并妥善备份。Linux/SSH 环境没有可用系统密钥库时，必须显式使用口令保护的身份文件；Pinset 不会退化为明文私钥。

### GitHub Actions

将 age 私有身份文本保存为仓库 Secret `PINSET_IDENTITY`。profile 和 `project-id` 不是秘密，可以提交到项目配置：

```yaml
jobs:
  test:
    runs-on: ubuntu-latest
    env:
      PINSET_IDENTITY: ${{ secrets.PINSET_IDENTITY }}
      PINSET_ENV_PROFILE: ci
    steps:
      - uses: actions/checkout@v4
      - uses: Future-Element/pinset@v2.1.0
        with:
          version: 2.1.0
          install: "true"
          trust-project-id: "4c5652e4-0000-4000-8000-000000000000"
      - run: pinset exec -- node app.js
```

身份变量会在业务进程启动前移除。不要把服务端秘密注入 Flutter、Web 或其他会把环境值编译进客户端制品的构建。

## 诊断、修复与更新

```sh
# 查看最终解析和真实路径
pinset current --explain
pinset which node --explain
pinset paths node
pinset doctor --deep

# 检查锁、缓存与安装所有权
pinset lock audit --json
pinset cache verify

# 修复具有匹配所有权收据的损坏安装
pinset install node@24.0.0 --repair

# 自更新不会在后台自动联网
pinset self outdated
pinset self update
```

`doctor --deep` 和安装收据验证的是安装布局、关键入口和统计信息，不宣称对每个已安装文件进行密码学验证。

## 安全边界

Pinset 保护可复现解析、下载完整性、仓库中静态密文和本机信任边界，但不承诺抵御管理员、调试器、恶意项目代码或已经失陷的 CI。已经获得秘密的进程可以启动其他程序，因此 Pinset 不提供“按子命令限制秘密”的伪隔离。

2.0 明确不包含：

- AWS、Azure、GCP KMS 或 OIDC 动态密钥交换；
- 后台守护进程、任务、hooks 或服务管理；
- 任意 age 插件、任意代码 Provider 或通用密码库；
- Nix/Conda 风格依赖求解、远程秘密同步或动态租约；
- Android SDK、Visual Studio Build Tools、Windows SDK 等外部系统组件安装。

## 命令文档

完整参数、状态修改、JSON 支持、退出码和常见错误请查阅：

- [中文命令文档](docs/commands.zh-CN.md)
- [English command reference](docs/commands.md)

也可以运行：

```sh
pinset --help
pinset <命令> --help
```

## 迁移与升级

旧项目升级到 schema 4 前可以先预览：

```sh
pinset migrate --dry-run
pinset migrate
```

迁移会分别报告项目配置、运行时锁和旧安装收据，不会自动创建加密环境文件。Pinset 只发布 `2.0.0-rc.1` 与 `2.0.0` 两个 2.0 发布节点。

## 卸载

删除运行时前先检查引用和所有权：

```sh
pinset uninstall node@24.0.0 --dry-run
pinset uninstall node@24.0.0
pinset prune --dry-run
```

完整卸载 Pinset 时，删除安装目录中的 CLI、shim 和路由命令，再删除自己加入 Shell 配置的初始化行。只有确定不再需要任何受管运行时、缓存、全局选择和本机信任时，才删除 `PINSET_HOME`。项目中的 `pinset.toml`、`pinset.lock`、`pinset.env/*.age` 与 `.venv` 不会自动删除。

## 当前版本与未来规划

### v2.1：批量选择与安装

Pinset 2.1 允许 `global` 和 `use` 接受可变长度的 `SELECTION...` 列表。一个批次可以包含 Pinset 任意不重复、且在最终作用域中满足声明依赖的内置 Provider 集合：Node.js、pnpm、Bun、Go、Python、Java、Rust、.NET 和 Flutter；Dart 仍由选中的 Flutter SDK 提供。下面只是示例，不是固定工具组合：

```sh
pinset global node@lts python@latest rust@stable
pinset use java@lts dotnet@lts flutter@latest
pinset use --global node@lts pnpm@latest bun@latest go@latest python@3.14
```

- 公开语法为 `pinset global [SELECTION...] [--no-install]` 和 `pinset use <SELECTION...> [--no-install] [--global]`。`global` 保留无参数查看模式；`use` 至少需要一个选择，批次数量不固定。
- `--no-install` 对整个批次生效。命令未提及的现有选择保持不变。
- Pinset 先解析所有参数、拒绝重复 Provider，并在写入状态前解析完所有 selector。任一参数、元数据、策略或版本解析失败时，配置、锁文件和安装状态都不变。
- 所有解析结果通过一次原子状态更新写入配置和锁文件。写入前使用完整的结果锁验证项目策略。
- 状态提交后，Pinset 按 Provider 依赖顺序只执行一轮锁定安装。v2.1 不并发下载，以保证输出、共享依赖、缓存所有权和失败恢复具有确定性。
- 如果状态提交后安装失败，已成功安装的运行时保持有效，完整请求状态仍保留在锁文件中。错误会提示使用 `pinset install --locked` 或 `pinset install --global --locked` 重试；Pinset 不会假装已完成的文件系统安装可以原子回滚。
- 帮助、命令补全、中英文命令文档和测试同时覆盖单选择兼容性与多选择行为。`install <tool@精确版本>` 仍是单个显式选择命令；基于锁的 `install --locked` 会安装完整作用域。

更后续的 2.x 会在保持失败关闭和本地优先边界的前提下评估 KMS/OIDC、更广的平台制品和更强的来源证明。路线图不承诺具体版本或发布日期。

## 贡献与许可证

提交变更前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。安全问题请按照 [SECURITY.md](SECURITY.md) 报告。

Pinset 使用 [MIT License](LICENSE)。
