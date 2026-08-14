# Pinset PRD

文档版本：`v0.8.0`
产品阶段：`Microsoft .NET SDK Provider released`
更新时间：`2026-08-14`

## 1. 产品简介

Pinset 是一个本地优先、跨平台、完全独立的运行时版本管理 CLI。它只读取自己的配置和锁文件，用一致的命令完成版本选择、锁定、安装、执行、镜像、缓存和诊断。

`v0.8.0` 在现有 Node.js、pnpm、Bun、Go、Flutter、CPython、Eclipse Temurin JDK 和 Rust stable Provider 基础上新增 Microsoft .NET SDK Provider。Pinset 直接使用官方 release metadata 锁定受支持 GA 通道的四平台 SDK 与 SHA-512，不依赖或接管其他 .NET SDK 管理器。

### 1.1 目标

- 一套一致命令管理全局和项目运行时；
- 项目配置与精确锁文件可提交、可复现、可解释；
- 下载、安装、缓存和卸载有可靠的安全检查；
- 不接管 shell profile，不覆盖用户命令，不隐藏回退；
- 不扫描或导入其他运行时管理器的配置和状态；
- 架构对 Provider 开放，安装器本身保持运行时中立。

### 1.2 目标用户

- 同时维护多个 Node 项目的开发者；
- Windows、WSL、Linux 和 macOS 混合环境用户；
- 需要国内镜像、企业内网镜像或离线安装的团队；
- 希望把运行时版本随项目提交并在 CI 中复现的团队；
- 后续需要统一管理多种语言运行时的个人或组织。

### 1.3 本版本不做

- 不接入 .NET preview/RC/nightly、runtime-only、ASP.NET Core Runtime、Desktop Runtime、workloads 或 Visual Studio，也不扩展到其他 JDK 厂商、JRE、OpenJ9、EA、JavaFX 或 GraalVM；
- 不管理 Android SDK/NDK、Xcode、CocoaPods、模拟器、设备、pub 依赖或 pub 缓存；
- 不管理 npm、pnpm、Bun 中的项目依赖或全局包；
- 不管理项目依赖包或全局 npm 包；
- 不自动修改 shell profile、系统 PATH、IDE 配置或系统注册表；
- 不维护第三方 Homebrew Tap、Scoop Bucket 或其他外部包仓库；
- 不删除系统运行时、用户文件或非 Pinset 所有的 `.venv`；
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
- 归档完整性不匹配立即停止，不继续 fallback；
- 安装在临时目录完成校验和解压后再原子提交；
- 不覆盖无法证明由 Pinset 所有的命令或目录；
- 只删除经过路径和所有权检查的 Pinset 文件。

### 2.5 运行时中立

curl 安装器只安装 `pinset` 与 `pinset-shim`。Node Provider 注册 `node`、`npm`、`npx`、`corepack`，pnpm Provider 注册 `pnpm`，Bun Provider 注册 `bun`、`bunx`，Go Provider 注册 `go`、`gofmt`，Python Provider 注册 `python`、`python3`、`pip`、`pip3`，Flutter Provider 注册 `flutter`、`dart`，Java Provider 注册 `java`、`javac`、`jar`、`javadoc`、`javap`、`keytool`、`jshell`，Rust Provider 注册 `rustc`、`cargo`、`rustdoc`、`rustfmt`、`cargo-fmt`、`clippy-driver`、`cargo-clippy`，.NET Provider 注册 `dotnet`。

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

### 3.3 Go 官方归档目标

| 目标 | 状态 |
| --- | --- |
| `windows-x86_64` | 支持 ZIP |
| `linux-x86_64` | 支持 TAR.GZ |
| `macos-x86_64` | Provider 支持 TAR.GZ |
| `macos-aarch64` | Provider 支持 TAR.GZ |

Go available 列表只显示稳定、具有全部上述工件且每个工件都带有效 SHA-256 的版本。

### 3.4 CPython 发行目标

| 目标 | 上游平台 | 状态 |
| --- | --- | --- |
| `windows-x86_64` | `x86_64-pc-windows-msvc` | 支持 TAR.GZ |
| `linux-x86_64` | `x86_64-unknown-linux-gnu` | 支持 TAR.GZ |
| `macos-x86_64` | `x86_64-apple-darwin` | Provider 支持 TAR.GZ |
| `macos-aarch64` | `aarch64-apple-darwin` | Provider 支持 TAR.GZ |

Python available 列表只显示稳定、同时具备四个 `install_only` 工件且每个工件带有效 SHA-256 的 CPython 发行版。

### 3.5 Eclipse Temurin JDK 发行目标

| 目标 | Adoptium OS/Arch | 状态 |
| --- | --- | --- |
| `windows-x86_64` | `windows` / `x64` | 支持 ZIP |
| `linux-x86_64` | `linux` / `x64` | 支持 TAR.GZ |
| `macos-x86_64` | `mac` / `x64` | Provider 支持 TAR.GZ |
| `macos-aarch64` | `mac` / `aarch64` | Provider 支持 TAR.GZ |

Java available 列表只显示 Eclipse vendor、JDK image、HotSpot、normal heap、GA release type 且同时具备四个目标、有效 SHA-256 与签名链接的 Temurin 发布。

### 3.6 Rust stable 发行目标

| 目标 | Rust target triple | 状态 |
| --- | --- | --- |
| `windows-x86_64` | `x86_64-pc-windows-msvc` | 支持 TAR.XZ |
| `linux-x86_64` | `x86_64-unknown-linux-gnu` | 支持 TAR.XZ |
| `macos-x86_64` | `x86_64-apple-darwin` | Provider 支持 TAR.XZ |
| `macos-aarch64` | `aarch64-apple-darwin` | Provider 支持 TAR.XZ |

Rust available 列表来自官方 stable manifests；生成可安装锁时，所选版本必须在同一 v2 release manifest 中具备四个目标的组合工具链归档、有效 SHA-256 和完整 default profile 组件。

### 3.7 Microsoft .NET SDK 发行目标

| 目标 | .NET RID | 状态 |
| --- | --- | --- |
| `windows-x86_64` | `win-x64` | 支持 ZIP |
| `linux-x86_64` | `linux-x64` | 支持 TAR.GZ |
| `macos-x86_64` | `osx-x64` | Provider 支持 TAR.GZ |
| `macos-aarch64` | `osx-arm64` | Provider 支持 TAR.GZ |

.NET available 列表只显示 active/maintenance 支持阶段的 GA LTS/STS SDK；每个 SDK 必须具备四个官方 RID 归档、规范 Microsoft URL 和有效 SHA-512。

## 4. 配置和数据

### 4.1 项目配置 `pinset.toml`

表达用户意图，可提交：

```toml
schema = 2

[tools]
node = "24.0.0"
pnpm = "11.21.0"
bun = "1.3.14"
go = "1.25.1"
python = "3.14.7+20260807"
java = "21.0.8+9"
rust = "1.97.1"
dotnet = "10.0.400"
```

### 4.2 项目锁文件 `pinset.lock`

表达解析结果，可提交。至少包含：

- schema；
- Provider 和精确版本；
- 每个目标平台的归档 URL、格式和完整性值；
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

Node Provider 声明 `node`、`npm`、`npx`、`corepack`；pnpm Provider 声明 `pnpm`；Bun Provider 声明 `bun`、`bunx`；Go Provider 声明 `go`、`gofmt`、`GOROOT` 和工具链切换策略；Python Provider 声明 `python`、`python3`、`pip`、`pip3`、项目 `.venv` 和受管环境策略；Flutter Provider 声明 `flutter`、`dart`、`FLUTTER_ROOT` 和受管 SDK 原地变更保护；Java Provider 声明 JDK 命令、平台 `JAVA_HOME` 和环境诊断策略；Rust Provider 声明 stable v2 manifest、default profile、工具链命令和用户环境保留策略；.NET Provider 声明官方 GA SDK metadata、`dotnet` 和 `DOTNET_ROOT`。

### 4.5 通用命令路由

`pinset-shim` 是与运行时无关的轻量调度器。命令入口只负责把命令名、当前目录和环境交给 Pinset 的统一解析逻辑，不复制网络、解压或 Provider 业务代码。

### 4.6 安装源

安装源是本机设置，保存在 `PINSET_HOME/sources.toml`，不进入项目锁文件。每个 Provider 包含：

- 内置不可修改的 `official`；
- 一个 active 来源；
- 零个或多个有序 fallback；
- 自定义源的 URL、HTTP 例外和元数据信任标记。

### 4.7 下载缓存

- 完整归档：`PINSET_HOME/downloads/{sha256|sha512}/<hash>.archive`；
- 断点文件：`PINSET_HOME/downloads/partial/{sha256|sha512}/<hash>.part`；
- 相同完整性值跨来源、跨项目复用；
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
- 不覆盖用户命令；
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
6. 校验下载大小和锁文件声明的 SHA-256 或 SHA-512；
7. 在随机临时目录安全解压；
8. 验证必需命令；
9. 写完整安装收据；
10. 原子发布到最终目录。

ZIP/TAR.XZ/TAR.GZ 必须拒绝路径穿越、绝对路径、特殊文件、逃逸符号链接、冲突路径、条目数超限和展开大小超限。

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

普通镜像只替换归档 URL，SHA-256 仍从 Node 官方 HTTPS `SHASUMS256.txt` 或 Go 官方 HTTPS 下载索引获取。使用 `--trust-metadata` 后，自定义 HTTPS 镜像也可以提供对应元数据，`source list` 会标记这类来源。

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
- 不扫描项目，不改 profile，不删系统运行时或用户文件。

### 5.14 独立配置边界

Pinset 只读取 `pinset.toml`、`pinset.lock` 和 `PINSET_HOME` 内的自有状态，不扫描、不解释、不导入其他运行时管理器的项目声明。用户必须通过 `pinset init` 和 `pinset use` 显式建立 Pinset 状态。

### 5.15 诊断

`doctor` 和 schema 2 的 `doctor --json` 报告：

- `PINSET_HOME`；
- 项目和全局配置/锁文件；
- 生效来源和安装状态；
- 所有已开放 Provider 命令的 PATH 候选和遮挡；
- Pinset 路由所有权；
- 旧版 Pinset shim 和路由所有权问题；
- 可执行的修复建议。

诊断命令只读，不修改状态。

## 6. 国际化

Beta 支持：

- `en`；
- `zh-CN`。

无子命令的 `pinset --lang <lang>` 持久保存到 `PINSET_HOME/settings.toml`；有子命令时只覆盖当前进程。`PINSET_LANG` 可做环境级临时覆盖。

正常提示、帮助、诊断、常见错误和进度信息应接入统一目录；底层操作系统错误、路径、URL 和哈希保留原值。

## 6.1 pnpm 与 Bun Provider

pnpm Provider 支持稳定版 10 和 11，Bun Provider 支持稳定版 1.x。两者均可使用精确、主版本、主次版本、`latest` 和 `current` 选择器，并通过 `list <tool> --available` 查看满足全部发布目标的稳定版本。

- pnpm 从 `@pnpm/exe` 的精确 `optionalDependencies` 解析 `@pnpm/win-x64`、`@pnpm/linux-x64`、`@pnpm/macos-arm64`；
- Bun 从 `bun` 的精确 `optionalDependencies` 解析 `@oven` 平台包；
- Bun x64 同时锁定 AVX2 与 baseline 包，当前机器安装目标通过 CPU 能力选择；
- 平台包必须使用官方 npm HTTPS tarball、SHA-512 SRI 和可由 npm registry 当前公钥验证的 ECDSA 签名；
- npm 包以 `.tar.gz` 安全解压，strip `package/` 后验证 `pnpm(.exe)` 或 `bin/bun(.exe)`；
- Bun 安装后创建受安装目录约束的 `bunx` 硬链接，文件系统不支持时退回复制；
- pnpm/Bun 不要求 Node 已选择或已安装，也不通过 Corepack 安装。

多个 Provider 同时选择时，子进程 PATH 依次包含当前命令目录和其他已选择、已安装 Provider 的命令目录，再追加继承 PATH；Pinset shim 目录必须排除，以防脚本中的跨工具调用递归进入 shim。

## 6.2 Go Provider

Go Provider 支持精确版本、主版本、主次版本、`latest` 和 `current` 选择器，并通过 `pinset list go --available` 查看满足全部发布目标的稳定版本。

- 官方元数据来自 `https://go.dev/dl/?mode=json&include=all`；
- 版本索引必须同时提供 filename、OS、arch、version、size、kind 和 SHA-256；
- 只接受 `kind=archive` 且身份与内置目标规划完全一致的工件；
- Windows 使用 ZIP，Linux/macOS 使用 TAR.GZ，均 strip 顶层 `go/` 后验证 `bin/go` 和 `bin/gofmt`；
- 锁文件记录规范化的精确 `x.y.z`，历史补丁零版本仍映射到上游省略 `.0` 的归档名；
- 受管进程的 `GOROOT` 必须指向所选安装目录；
- 用户未显式设置 `GOTOOLCHAIN` 时注入 `local`，用户显式设置时保留原值并由诊断提示可能绕过锁定；
- 保留用户的 `GOPATH`、`GOMODCACHE`、`GOCACHE`，不修改 `go.mod` 或 `go.work`；
- 不管理 Go module 依赖、代理凭据、交叉编译器、TinyGo 或系统 C 工具链。

## 6.3 Flutter SDK Provider

Flutter Provider 首期只接收官方 stable 渠道，支持精确版本、主版本、主次版本、`latest` 和 `current` 选择器，并通过 `pinset list flutter --available` 同时显示 Flutter 与捆绑 Dart 的精确版本。

- 官方元数据来自 Google Storage 上按 Linux、Windows、macOS 分发的三个 Flutter release JSON；
- 只有同时具备 Windows x64、Linux x64、macOS x64 和 macOS arm64 工件，且 release hash、Flutter 版本和 Dart 版本一致的 stable 发布才可用；
- 锁文件记录 stable 渠道、捆绑 Dart 版本、release hash、逐平台规范归档路径和 SHA-256；
- Windows/macOS 使用 ZIP，Linux 使用 TAR.XZ，均 strip 顶层 `flutter/` 后验证 `bin/flutter`、`bin/dart` 和内置 Dart SDK；
- Flutter 官方归档使用独立的 3 GiB 下载、12 GiB 解压和 250,000 条目安全上限，其他 Provider 继续使用较低默认限额；
- `flutter` 与 `dart` 必须解析到同一安装目录，受管进程获得对应 `FLUTTER_ROOT`；用户未显式设置时注入 `FLUTTER_SUPPRESS_ANALYTICS=true`；
- `flutter upgrade`、`flutter downgrade`、`flutter channel` 在受管 SDK 启动前被拒绝，版本变化必须重新通过 Pinset 选择和安装；
- 不管理 Flutter/Dart 项目依赖、pub 缓存、移动端/桌面端平台 SDK、模拟器或设备。

## 6.4 CPython Provider 与项目虚拟环境

CPython Provider 支持精确版本、主版本、主次版本、`latest` 和 `current` 选择器，并通过 `pinset list python --available` 显示稳定发行版及上游构建 ID。

- 官方元数据来自 Astral 维护的 `python-build-standalone` 版本注册表；
- 只接受四个支持目标都存在的 `install_only` TAR.GZ 工件；
- 锁文件精确记录 `x.y.z+YYYYMMDD` 发行身份、CPython 版本、构建 ID、variant、归档 URL 和 SHA-256；
- 全局选择路由基础解释器；项目选择通过锁定解释器的标准库 `venv` 创建项目根目录 `.venv`；
- 全局及项目中的 `python`、`python3`、`pip`、`pip3` 自动解析为同一个选中解释器；`pip`/`pip3` 统一执行 `python -m pip`，项目中指向 `.venv`；
- `pinset exec -- <命令>` 还可运行 `.venv/bin` 或 `.venv/Scripts` 下的其他项目脚本；
- 受管 Python 进程设置 `VIRTUAL_ENV` 并移除 `PYTHONHOME`，不要求执行虚拟环境激活脚本；
- `.venv/.pinset-venv.toml` 记录 schema、精确发行版和目标；无标记、标记不匹配、符号链接或损坏环境都拒绝接管；
- `pinset venv create` 只创建或验证，`status` 只读，`recreate` 才能在所有权验证后删除并重建；
- 不解析 Python 包依赖、不决定包安装策略、不管理全局包，也不读取其他管理器的声明。

## 6.5 Eclipse Temurin JDK Provider

Java Provider 支持 `latest`/`current`、`lts`、Feature 主版本、主次前缀、Update 版本和包含 `+build` 的精确版本，并通过 `pinset list java --available` 显示完整锁定身份。

- 官方元数据来自 Adoptium API v3 的 available releases 与 GA assets；
- 只接受 Eclipse Temurin、JDK、HotSpot、normal heap、GA、project=jdk 的四平台归档，不接受安装器包；
- Java 使用专用版本模型，`+build` 参与身份和排序；锁文件中的规范版本形如 `21.0.8+9`；
- schema 2 Provider metadata 记录 distribution、vendor、image type、JVM implementation、heap size、release type、Feature、release name、OpenJDK version 和逐平台签名链接；
- 每个工件锁定最终 GitHub Release URL、包名、归档格式和 Adoptium SHA-256；签名链接进入锁文件，但在完成密钥治理前不调用系统 `gpg`；
- macOS 解压后以 `Contents/Home` 为 `JAVA_HOME`，Windows/Linux 以安装根目录为 `JAVA_HOME`；命令 PATH 始终来自同一个 JDK；
- `CLASSPATH`、`JAVA_TOOL_OPTIONS`、`JDK_JAVA_OPTIONS`、`_JAVA_OPTIONS` 保留用户值，`doctor` 在选择 Java 时报告其潜在影响；
- 不修改 shell profile、系统环境、Gradle/Maven toolchain 或 `cacerts`，不接管系统 JDK 和外部管理器状态。

## 6.6 Rust stable Provider

Rust Provider 支持 `stable`/`latest`/`current`、主版本、主次版本和精确 stable 版本，并通过 `pinset list rust --available` 显示官方清单中的版本与发布日期。

- 版本发现来自 `https://static.rust-lang.org/manifests.txt`，精确发布元数据来自经过官方 `.sha256` 校验的 v2 `channel-rust-x.y.z.toml`；
- 只接受 stable channel 和同时具备四个支持目标的官方 `rust` 组合归档；
- 安装官方 `default` profile，组件边界固定为 `rustc`、`cargo`、`rust-std`、`rust-docs`、`rustfmt` 与 `clippy`；
- 锁文件记录 manifest date、manifest SHA-256、profile、组件、规范归档 URL 与每个目标的 SHA-256；
- 路由 `rustc`、`cargo`、`rustdoc`、`rustfmt`、`cargo-fmt`、`clippy-driver` 和 `cargo-clippy`；
- 保留用户的 `CARGO_HOME`、`RUSTUP_HOME` 与 `RUSTFLAGS`，不创建或修改外部管理器配置，不修改 shell profile；
- 不管理 Cargo 依赖、crate 发布、原生链接器、系统 SDK、交叉编译环境，也不提供 beta/nightly、额外 target 或组件增删。

## 6.7 Microsoft .NET SDK Provider

.NET Provider 支持 `latest`/`current`、`lts`、主版本、`major.minor` 通道和精确 `x.y.zzz` SDK 版本，并通过 `pinset list dotnet --available` 显示 release type、support phase 与发布日期。

- 版本发现来自 Microsoft 官方 `releases-index.json` 和各通道 `releases.json`；
- 只接受 active/maintenance 阶段的 GA LTS/STS 通道，同时要求四个支持 RID 的 SDK 归档完整；
- 锁文件记录 channel、release type、support phase、runtime release version、release date、规范归档 URL 和逐平台 SHA-512；
- Windows 使用 ZIP，Linux/macOS 使用 `tar.gz`，安装根目录必须包含 `dotnet`/`dotnet.exe`；
- 路由 `dotnet`，并为受管进程设置匹配安装根目录的 `DOTNET_ROOT`；
- 保留用户的 `DOTNET_CLI_HOME`、`NUGET_PACKAGES`、NuGet 配置和遥测选项，不修改 `global.json`、项目文件、shell profile 或系统 SDK；
- 不管理 runtime-only、ASP.NET Core Runtime、Desktop Runtime、workloads、templates、Visual Studio 或 NuGet 依赖。

## 7. 安全和供应链

### 7.1 Provider 归档

- 从可信元数据源获得预期 SHA-256；
- 下载源不得改变锁文件中的预期哈希；
- 所有归档在解压前校验；
- Beta 尚未验证 Node 上游 `SHASUMS256.txt.sig` 的 OpenPGP 签名，计划在稳定版加入。
- pnpm/Bun 使用 npm 平台包的 SHA-512 SRI，并在锁定阶段验证 npm registry ECDSA 签名；安装阶段重新计算 SHA-512。
- Go 使用官方下载 JSON 中逐工件提供的 SHA-256；锁文件中的规范 URL、目标和归档格式必须与内置 Go Provider 一致。
- Flutter 使用官方 release JSON 中逐工件提供的 SHA-256；三个平台索引必须对 Flutter 版本、Dart 版本和 release hash 达成一致。
- Python 使用官方版本注册表中的逐工件 SHA-256，并校验平台、variant、归档格式和规范 GitHub Release 身份。
- Java 使用 Adoptium API 逐工件 SHA-256，严格校验 Temurin GitHub Release 仓库、Feature、目标平台、包名和签名链接身份。
- Rust 先验证官方 v2 release manifest 的 SHA-256，再使用其中的逐目标 SHA-256 校验组合工具链归档；清单日期、渠道、profile、组件、URL 和 target identity 都必须与内置 Provider 一致。
- .NET 使用 Microsoft 官方 release metadata 中的逐 RID SHA-512，锁文件同时校验 SDK 版本、通道、支持阶段、URL、RID 和归档格式。

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

### 9.1 Quality 检查

- `cargo fmt --all -- --check`；
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`；
- `cargo test --workspace --all-features`；
- 锁定 release build；
- POSIX 安装/卸载脚本隔离测试；
- PowerShell 卸载脚本隔离测试；
- shim 轻依赖检查；
- `git diff --check`。

本项目不在维护者本机或 WSL 运行构建、测试、Pinset 或运行时安装；普通 CI 只执行三平台 release 构建和打包，格式、Clippy、单元测试与小型脚本测试在 Release Quality 门禁执行，真实运行时下载与验收仅在发布后的隔离虚拟机执行。单平台结果不能替代三平台结果。

### 9.2 Release 检查

GitHub Actions 的每个支持平台必须：

- 构建 locked release 二进制；
- 生成并发布归档、校验、SBOM 和来源证明。

任一必需的 Quality、构建或打包任务失败都不发布 Release。除编译 Pinset 必需的 Rust 构建工具链外，GitHub Actions 不通过 Pinset 下载任何 Provider 真实运行时；Node、pnpm、Bun、Go、Python、Flutter、Java、Rust 和 .NET 的 available、全局/项目覆盖、环境变量、编译运行与安装复用，统一由发布后的隔离虚拟机使用完整验收脚本验证。

## 10. 稳定版前待办

稳定版前计划完成：

- 验证 Node 上游 SHASUMS OpenPGP 签名，并定义密钥轮换策略；
- 冻结 schema 1 读取兼容、schema 2 写入承诺和迁移策略；
- 完成 Beta 用户在代理、镜像和离线场景的反馈修复；
- 决定 Linux arm64 与 macOS Intel 正式归档策略；
- 完成发布回滚、撤回和安全公告流程；
- 完成 Flutter/Dart 的发布后隔离虚拟机真实运行时验收。

后续 Provider 继续保持 Pinset 独立安装与状态边界，复用现有 Provider、来源、缓存、路由和安全机制，不在 CLI 中增加语言专用的零散逻辑。
