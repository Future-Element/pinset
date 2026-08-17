# Pinset

[![CI](https://github.com/Future-Element/pinset/actions/workflows/ci.yml/badge.svg)](https://github.com/Future-Element/pinset/actions/workflows/ci.yml)
[![Release](https://github.com/Future-Element/pinset/actions/workflows/release.yml/badge.svg)](https://github.com/Future-Element/pinset/actions/workflows/release.yml)

Pinset 是一个本地优先、完全独立的多语言运行时版本管理 CLI。它只读取自己的项目配置与锁文件，用一致的命令管理 Node.js、pnpm、Bun、Go、Python、Flutter、Java、Rust 和 .NET SDK 等运行时。

`v0.2.0` 在已验证的 Node.js 管理基础上新增独立的 pnpm 与 Bun Provider。

`v0.2.1` 修复 pnpm 10 独立可执行包被错误套用 pnpm 11 `dist/` 叠加层规则的问题，并覆盖项目 pnpm 10 与 Bun 1.2 的组合安装。

`v0.3.0` 新增 Go Provider，并将 Provider 的元数据、安装器和受管环境策略收敛到统一清单。

`v0.4.0` 新增 Flutter stable SDK Provider，并将 SDK 内置 Dart 作为同一个锁定与路由单元。

`v0.4.1` 将 XZ 解压依赖静态链接进 macOS 发布二进制，不要求用户安装 Homebrew `xz`。

`v0.4.2` 为中断的大型运行时下载增加同源自动重试和断点续传；短暂网络中断时会在同一条命令内继续下载 Flutter SDK。

`v0.5.0` 新增 CPython Provider，并为项目创建 Pinset 自有的 `.venv`；`python`、`python3` 和 `pinset exec -- <环境命令>` 会自动路由到项目环境，无需手动激活虚拟环境。

`v0.6.0` 新增 Eclipse Temurin JDK Provider，并修复 Python Provider 缺少 `pip`/`pip3` 直接路由的问题；Java 首期仅支持 JDK/HotSpot/GA，锁定精确 build、四平台归档和 SHA-256，并为受管进程设置 `JAVA_HOME`。

`v0.6.1` 修复 macOS 等系统中 Pinset 路由目录虽然已在 PATH、却被更早的 `/usr/bin/java` 等系统命令遮挡时未给出提示的问题。Provider 注册会检查真正生效的命令路径，并按当前 Shell 输出可直接执行的激活命令。

`v0.7.0` 新增 Rust stable Provider，使用 Rust 官方 v2 release manifest 锁定 default profile 工具链、四平台归档和 SHA-256，不依赖或接管其他 Rust 工具链管理器。

`v0.8.0` 新增 Microsoft .NET SDK Provider，只列出仍受支持的 GA LTS/STS 通道，使用官方 release metadata 锁定四平台 SDK 归档和 SHA-512，并为受管进程设置 `DOTNET_ROOT`。

`v0.9.0` 不新增语言，重点补齐所有 Provider 的安装生命周期：跨 Provider 已安装列表、更新检查、卸载与清理预览、缓存空间统计、内容校验、受损缓存修复和 JSON 输出。

- 全局 Node 默认版本、项目级 Node 覆盖，以及离开项目后恢复全局版本；
- `node@24.0.0`、`node@24`、`node@24.12`、`node@lts`、`node@current`；
- pnpm 10/11 与 Bun 1.x 的精确、主版本、主次版本、`latest`/`current` 选择器；
- Go 的精确、主版本、主次版本、`latest`/`current` 选择器；
- CPython 的精确、主版本、主次版本、`latest`/`current` 选择器，以及构建 ID 精确锁定；
- Flutter stable 的精确、主版本、主次版本、`latest`/`current` 选择器，以及对应的内置 Dart 版本；
- Eclipse Temurin JDK 的 `latest`/`current`、`lts`、Feature、Update 和精确 `+build` 选择器；
- Rust stable 的精确、主版本、主次版本和 `stable`/`latest`/`current` 选择器；
- .NET SDK 的精确、主版本、通道和 `lts`/`latest`/`current` 选择器；
- Windows x64、Linux x64、macOS Apple Silicon 的 Pinset Release；
- `node`、`npm`、`npx`、`corepack`、`pnpm`、`bun`、`bunx`、`go`、`gofmt`、`python`、`python3`、`pip`、`pip3`、`flutter`、`dart`、`java`、`javac`、`jar`、`javadoc`、`javap`、`keytool`、`jshell`、`rustc`、`cargo`、`rustdoc`、`rustfmt`、`cargo-fmt`、`clippy-driver`、`cargo-clippy`、`dotnet` 统一路由；
- 可提交的 `pinset.toml` 和 `pinset.lock`；
- Node SHA-256、npm SHA-512 SRI 与 registry ECDSA 签名校验、安全解压、事务安装、并发安装锁、断点续传和内容寻址缓存；
- 国内或企业镜像、有序回退、可选的可信元数据镜像和离线缓存导入；
- 中英文提示、诊断、安全卸载，以及明确拒绝接管非 Pinset 所有的目录和命令；
- 三平台自动构建、Provider 元数据/锁文件/安装逻辑测试、CycloneDX SBOM 和 GitHub 构建来源证明；真实运行时下载与验收统一由发布后的隔离虚拟机执行。

## 快速安装

Linux x64、macOS Apple Silicon 和 WSL：

```bash
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/install.sh | sh
```

这条命令只安装 `pinset` 和通用路由器 `pinset-shim` 到 `$HOME/.local/bin`，不会自动安装 Node，也不会修改 shell profile。安装脚本会根据平台下载对应 Release 归档，并强制校验同一 Release 中的 `SHA256SUMS`。

将 Pinset 加入当前终端的 PATH，并确保它位于 `/usr/bin`、Homebrew 和其他运行时管理器之前：

```bash
export PATH="$HOME/.local/bin:$PATH"
pinset --version
```

也可以只为当前 Shell 激活路由：

```bash
eval "$(pinset activate zsh)"   # macOS 默认 Zsh
eval "$(pinset activate bash)"  # Bash / WSL
```

长期使用可自行把 `export PATH=...` 写入 `~/.bashrc` 或 `~/.zshrc`。Pinset 不会擅自修改这些文件。

`v0.8.0` 发布后可固定安装该版本：

```bash
curl -fsSL https://github.com/Future-Element/pinset/releases/download/v0.8.0/install.sh | sh
```

自定义安装目录：

```bash
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/install.sh |
  sh -s -- --install-dir "$HOME/bin"
```

### Windows

从 [GitHub Releases](https://github.com/Future-Element/pinset/releases) 下载 `pinset-windows-x86_64.zip`，解压到一个长期保留的目录，例如 `C:\Tools\pinset`，再把这个目录加入 PATH：

```powershell
$pinsetBin = 'C:\Tools\pinset'
$env:PATH = "$pinsetBin;$env:PATH"
pinset --version
```

需要永久加入用户 PATH 时，可在 Windows“环境变量”设置中添加该目录。Windows 与 WSL 是两个独立系统，二进制、`PINSET_HOME`、Node 安装和 PATH 不能混用。

## 五分钟开始使用

### 1. 设置中文

持久设置简体中文：

```shell
pinset --lang zh-CN
```

恢复英文：

```shell
pinset --lang en
```

只临时改变一次命令的输出：

```shell
pinset --lang zh-CN doctor
```

### 2. 设置全局 Node 版本

```shell
pinset global node@lts
pinset global
node --version
npm --version
npx --version
corepack --version
```

`pinset global node@lts` 会解析为一个精确版本，写入全局配置和锁文件，并安装当前平台的 Node。全局状态保存在 `PINSET_HOME/state/global.toml` 与 `global.lock`，不会在当前目录创建项目文件。

只锁定、不立即下载：

```shell
pinset global node@24 --no-install
pinset install --global --locked
```

兼容入口仍可使用：

```shell
pinset use node@24 --global
```

### 3. 在项目中覆盖全局版本

```shell
cd my-project
pinset init
pinset use node@22
pinset current
node --version
```

项目中会生成或更新：

```toml
# pinset.toml
schema = 2

[tools]
node = "22.0.0"
pnpm = "11.21.0"
bun = "1.3.14"
go = "1.25.1"
flutter = "3.47.0"
python = "3.14.7+20260807"
java = "21.0.8+9"
rust = "1.97.1"
dotnet = "10.0.400"
```

`pinset.lock` 保存解析后的精确版本、平台归档、完整性值和来源信息。Node、Go、Flutter、Python、Java 和 Rust 使用上游元数据提供的 SHA-256；pnpm/Bun 与 .NET SDK 使用 SHA-512，其中 pnpm/Bun 还会在生成锁文件前验证 registry ECDSA 签名。Java 记录 Temurin 分发属性、OpenJDK release identity 和逐平台签名链接；Rust 记录官方 manifest 日期、manifest SHA-256、profile 和组件边界；.NET 记录通道、支持阶段与 runtime release identity。建议把 `pinset.toml` 与 `pinset.lock` 一起提交。

在项目目录中，项目版本优先于全局版本；离开项目目录后自动恢复全局版本。如果项目或全局声明了版本但安装缺失，Pinset 会明确报错，不会静默换成系统运行时。

只生成配置和锁文件：

```shell
pinset use node@22 --no-install
pinset install --locked
```

清除项目覆盖并恢复全局版本：

```shell
pinset unset node
```

清除全局默认并恢复系统 PATH 中的 Node：

```shell
pinset unset node --global
```

`unset` 不会卸载已安装版本或清除缓存。

## Node 版本与执行模型

### 支持的选择器

| 写法 | 含义 |
| --- | --- |
| `node@24.0.0` | 精确版本 |
| `node@24` | 该主版本下最新可用版本 |
| `node@24.12` | 该主次版本下最新可用版本 |
| `node@lts` | 最新 LTS 版本 |
| `node@current` | 最新 Current 版本 |

浮动选择器只用于输入；Pinset 在锁文件中始终记录精确版本。

## pnpm 与 Bun

pnpm 和 Bun 与 Node 一样既可设为项目版本，也可设为全局默认，而且都支持查看官方可用版本：

```shell
pinset list pnpm --available
pinset list bun --available

pinset use pnpm@11
pinset use bun@1.3
pinset install --locked

pinset current pnpm
pinset current bun
pnpm --version
bun --version
bunx --version
```

当前支持范围：

| Provider | 稳定版本 | 命令 | 分发来源 |
| --- | --- | --- | --- |
| pnpm | 10、11 | `pnpm` | `@pnpm/exe` 对应的官方平台包 |
| Bun | 1.x | `bun`、`bunx` | `bun` 对应的 `@oven` 官方平台包 |

pnpm 与 Bun 是独立运行时，不依赖已选择的 Node，也不通过 Corepack 安装。Bun 在 x64 上会根据当前 CPU 自动选择 AVX2 或 baseline 包，并把变体写入安装目标。多个 Provider 同时选择时，Pinset 为子进程构造组合 PATH；所选运行时目录排在前面，Pinset 自己的 shim 目录会被排除，因此 pnpm 脚本调用 Node、Node 脚本调用 Bun 时不会递归进入 shim。

精确版本可离线解析选择器，但生成可安装锁仍需读取官方 npm 单版本元数据并验证包签名。`list --available` 使用 npm abbreviated metadata，过滤预发布版本以及缺少任一受支持平台包的版本。

## Go

Go 与其他 Provider 使用相同的项目、全局、available、安装、卸载和命令路由流程：

```shell
pinset list go --available
pinset global go@latest
pinset use go@1.25
pinset current go
go version
gofmt -w main.go
```

Pinset 从 Go 官方下载 JSON 索引读取稳定版本和 Windows x64、Linux x64、macOS x64/arm64 工件，锁定官方 SHA-256 后再安装。历史上省略补丁零的版本会规范化为 `x.y.0`，但仍使用对应的官方归档名。

受管 Go 命令会获得与所选安装一致的 `GOROOT`。如果调用 Pinset 时没有显式设置 `GOTOOLCHAIN`，Pinset 为受管进程设置 `GOTOOLCHAIN=local`，避免 `go.mod` 或 `go.work` 绕过 Pinset 锁定并静默下载另一套工具链；显式用户设置会被保留。

Pinset 不修改 `go.mod`/`go.work`，也不管理 Go module 依赖、`GOPATH`、`GOMODCACHE` 或 `GOCACHE`。

## Python 与项目 `.venv`

Python 使用与其他 Provider 相同的选择、锁定、安装和缓存流程：

```shell
pinset list python --available
pinset global python@3.14
pinset use python@3.14
pinset current python
pinset which python
pip --version
pip3 --version
```

Pinset 从官方版本注册表选择稳定 CPython 的 `install_only` 发行包，并把 CPython 精确版本、上游构建 ID、发行变体、四个平台工件和 SHA-256 写入锁文件。项目配置中的版本形如 `3.14.7+20260807`，因此同一个 CPython 版本的不同上游构建不会互相覆盖。

项目执行 `pinset use python@...` 或 `pinset install --locked` 后，Pinset 使用锁定解释器的标准库 `venv` 创建项目根目录 `.venv`。只要 Pinset 的 shim 目录已一次性加入 PATH，直接运行 `python`、`python3`、`pip` 或 `pip3` 就会进入最近项目的 `.venv`，不需要执行 `activate`：

```shell
python -c "import sys; print(sys.prefix)"
pip --version
pinset exec -- pip3 --version
pinset exec -- pytest
```

`pip` 与 `pip3` 都通过当前选中解释器的 `python -m pip` 执行，不依赖平台相关的 pip 启动脚本，也不会误用系统 Python 的 pip。`pinset exec -- <命令>` 还会查找 `.venv/bin` 或 `.venv/Scripts` 中的其他项目脚本，设置进程级 `VIRTUAL_ENV`，并清除可能把解释器引向别处的 `PYTHONHOME`。

虚拟环境必须包含 Pinset 所有权标记，且与项目锁定的 Python 发行版和当前目标一致。Pinset 不会采用、覆盖或删除未标记的 `.venv`。显式生命周期命令为：

```shell
pinset venv status
pinset venv create
pinset venv recreate
```

`create` 只创建缺失环境或验证现有环境；`recreate` 才会在验证 Pinset 所有权后删除并重建 `.venv`。Pinset 不解析 Python 包依赖、不决定包安装策略，也不读取或导入其他运行时管理器的项目声明。

## Java / Eclipse Temurin JDK

Java Provider 首期固定为 Eclipse Temurin JDK、HotSpot、GA 正式版本：

```shell
pinset list java --available
pinset global java@lts
pinset use java@21
pinset current java
java -version
javac -version
```

支持 `latest`/`current`、`lts`、Feature（如 `21`）、主次前缀、Update（如 `21.0.8`）和精确 build（如 `21.0.8+9`）。Pinset 使用 Java 专用版本排序，构建号是锁定身份的一部分。

锁文件记录 Temurin 分发、Eclipse vendor、JDK image、HotSpot、GA、OpenJDK release name、最终四平台归档 URL、SHA-256 和签名链接。签名链接用于供应链记录；在完成 GPG 密钥固定、轮换和吊销策略前，Pinset 不要求用户安装或调用系统 `gpg`。

受管命令包括 `java`、`javac`、`jar`、`javadoc`、`javap`、`keytool` 和可用时的 `jshell`。Pinset 只在受管子进程中设置 `JAVA_HOME` 并调整 PATH：macOS 指向 JDK bundle 的 `Contents/Home`，Windows/Linux 指向安装根目录。用户已有的 `CLASSPATH`、`JAVA_TOOL_OPTIONS`、`JDK_JAVA_OPTIONS`、`_JAVA_OPTIONS` 会保留，`doctor` 会提示它们可能影响运行结果。

Pinset 不管理 Maven、Gradle、Ant、Java 依赖、Gradle/Maven toolchain、`cacerts` 或系统 JDK，也不导入 SDKMAN、jEnv、asdf 等外部管理器状态。

## Rust stable 工具链

Rust 与其他 Provider 使用相同的 available、全局、项目、安装和命令路由流程：

```shell
pinset list rust --available
pinset global rust@latest
pinset use rust@1.97
pinset current rustc
rustc --version
cargo --version
```

Pinset 从 Rust 官方 `manifests.txt` 发现 stable 版本，先校验精确版本 v2 release manifest 的 SHA-256，再锁定 Windows x64、Linux x64、macOS x64/arm64 的组合工具链归档和逐目标 SHA-256。支持精确版本、主版本、主次版本、`stable`、`latest` 和 `current`；锁文件始终保存精确版本。

首期固定安装官方 `default` profile，包含 `rustc`、`cargo`、`rust-std`、`rust-docs`、`rustfmt` 和 `clippy`，并路由 `rustc`、`cargo`、`rustdoc`、`rustfmt`、`cargo-fmt`、`clippy-driver`、`cargo-clippy`。Pinset 保留用户已有的 `CARGO_HOME`、`RUSTUP_HOME` 和 `RUSTFLAGS`。

Pinset 不管理 Cargo 依赖、crate 发布、C/C++ 链接器、系统 SDK 或交叉编译环境；首期不支持 beta/nightly、自定义 channel、额外 target、组件增删，也不读取、导入或修改其他 Rust 工具链管理器的状态。

## Microsoft .NET SDK

.NET Provider 首期只管理 Microsoft 官方 GA SDK：

```shell
pinset list dotnet --available
pinset global dotnet@lts
pinset use dotnet@10
pinset current dotnet
dotnet --version
```

支持 `latest`/`current`、`lts`、主版本、`major.minor` 通道和精确 `x.y.zzz` SDK 版本。available 列表只包含仍处于 `active` 或 `maintenance` 阶段、且四个支持目标归档完整的 LTS/STS 通道；preview、RC、go-live 和 EOL 不会进入解析结果。

Pinset 使用 Microsoft 官方 `releases-index.json` 和通道 `releases.json` 锁定 Windows x64、Linux x64、macOS x64/arm64 归档及 SHA-512。受管子进程获得与所选 SDK 一致的 `DOTNET_ROOT`；用户已有的 `DOTNET_CLI_HOME`、`NUGET_PACKAGES`、NuGet 配置和遥测选项保持不变。

首期不管理 runtime-only、ASP.NET Core Runtime、Desktop Runtime、workloads、templates、Visual Studio 或 NuGet 依赖，也不会自动修改 `global.json`、项目文件、shell profile 或系统 SDK。

## Flutter 与内置 Dart

Flutter 使用与其他 Provider 相同的项目、全局、available、安装、卸载和路由流程：

```shell
pinset list flutter --available
pinset global flutter@latest
pinset use flutter@3.44
pinset current flutter
flutter --version
dart --version
```

`list flutter --available` 会同时显示 Flutter stable 版本与它捆绑的 Dart 版本。Pinset 读取 Flutter 官方 Linux、Windows、macOS release JSON，只接受四个受支持目标都存在且 release hash、Flutter 版本和 Dart 版本一致的发布，并把逐平台 SHA-256 写入锁文件。

Flutter SDK 归档明显大于其他 Provider。Pinset 仅为已通过内置工件身份校验的 Flutter 锁使用专用下载和解压安全上限；已安装 SDK 可直接重用，下载归档可通过 `pinset cache clean` 清理。

Flutter 与 Dart 始终从同一套 SDK 的 `bin` 路由。受管命令获得对应的 `FLUTTER_ROOT`；未显式设置时，Pinset 还会使用 `FLUTTER_SUPPRESS_ANALYTICS=true`，用户已有值保持不变。

为保持锁文件可复现，受管 SDK 不允许原地执行 `flutter upgrade`、`flutter downgrade` 或 `flutter channel`。请选择新版本，例如 `pinset use flutter@3.47`，由 Pinset 解析并安装另一套 SDK。

Flutter Provider 不会因为 Flutter 项目自动选择或安装 JDK。Pinset 不管理 Android SDK/NDK、Xcode、CocoaPods、模拟器、设备、Flutter/Dart 项目依赖或 pub 缓存。

### 解析优先级

```text
最近的项目 pinset.toml / pinset.lock
              ↓
Pinset 全局 state/global.toml / global.lock
              ↓
排除 Pinset 路由入口后的系统 PATH
```

查看最终结果：

```shell
pinset current
pinset current node
pinset which node
pinset which npm
```

### 通过 Pinset 执行命令

即使还没有启用直接命令路由，也可以使用：

```shell
pinset exec -- node --version
pinset exec -- npm ci
pinset exec -- go version
pinset exec -- python -m pip --version
pinset exec -- pytest
pinset exec go@1.25 -- go version
pinset exec -- flutter --version
pinset exec -- dart --version
pinset exec -- java -version
pinset exec -- javac -version
pinset exec -- npx vite
```

一次性使用某个已经安装的精确版本，不修改项目或全局状态：

```shell
pinset exec node@24.0.0 -- node --version
```

只预装某个版本，不改变项目或全局选择：

```shell
pinset install node@20
```

### 直接运行 Provider 命令

正常执行 `global`、`use` 或 `install` 后，各 Provider 会注册自己的通用路由：Node 注册四个命令，pnpm 注册 `pnpm`，Bun 注册 `bun` 和 `bunx`，Go 注册 `go` 和 `gofmt`，Python 注册 `python`、`python3`、`pip` 和 `pip3`，Flutter 注册 `flutter` 和 `dart`，Java 注册 JDK 命令，Rust 注册其 default profile 命令。curl 安装器本身仍然保持运行时中立，只安装 Pinset 和通用调度器。

如果使用源码构建、定制安装目录，或当前终端还没有路由目录，可以临时激活：

```bash
eval "$(pinset activate bash)"  # Bash / WSL
eval "$(pinset activate zsh)"   # Zsh
```

PowerShell：

```powershell
pinset activate powershell | Out-String | Invoke-Expression
```

查看路由目录或修复路由：

```shell
pinset shim path
pinset shim install --provider node
pinset shim install --provider pnpm
pinset shim install --provider bun
pinset shim install --provider go
pinset shim install --provider python
pinset shim install --provider flutter
pinset shim migrate --provider node
```

这些命令不会覆盖同名的用户文件或系统命令。`doctor` 会报告 PATH 中的遮挡、旧版 Pinset shim 和路由所有权问题。

## 下载、镜像与国内网络

### 普通镜像

镜像必须保持 Node 官方目录结构。例如：

```shell
pinset source add node cn-mirror --base-url https://mirror.example/node/
pinset source use node cn-mirror
pinset source fallback node official
pinset source test node cn-mirror
pinset source list node
```

默认情况下，自定义镜像只提供归档下载；版本索引和 `SHASUMS256.txt` 仍来自 Node 官方 HTTPS 地址。这样镜像不能替换校验元数据，只能加速大文件传输。

网络或传输失败时 Pinset 会按 fallback 顺序尝试下一来源。SHA-256 不匹配属于安全失败，会立即停止，不会换源重试来掩盖异常。

### 可信元数据镜像

如果官方 `index.json` 或 `SHASUMS256.txt` 在所在网络也很慢，可以信任一个 HTTPS 镜像提供这些文件：

```shell
pinset source add node cn-trusted \
  --base-url https://mirror.example/node/ \
  --trust-metadata
pinset source use node cn-trusted
pinset source test node cn-trusted
```

`--trust-metadata` 表示版本发现、校验清单和归档都信任该镜像，因此只应对你审阅或控制的镜像使用。可信元数据源强制 HTTPS，不能与 `--allow-insecure` 同时使用。

仅在明确可信的内网环境中才允许 HTTP：

```shell
pinset source add node lan \
  --base-url http://192.168.1.10/node/ \
  --allow-insecure
```

安装源是本机配置，保存在 `PINSET_HOME/sources.toml`，不写进项目锁文件。

### 进度条和断点续传

交互终端中的下载进度在同一行刷新，并根据终端宽度和中英文字符宽度自动缩短文件名：

```text
正在下载 node-v24.0.0-linux-x64.tar.xz [==========          ] 50% 15.0 MiB/30.0 MiB
```

网络中断后再次执行相同安装命令，Pinset 会从按算法分仓的内容寻址 `.part` 文件继续下载，并校验服务端 `Content-Range`。服务端不支持 Range 时会安全地从头下载。只有完整 SHA-256 或 SHA-512 校验通过后才会进入安装和完成提示。

### 离线缓存

查看和清理缓存：

```shell
pinset cache list
pinset cache info
pinset cache verify
pinset cache repair --dry-run
pinset cache clean --dry-run
```

`cache verify` 会重新计算完整归档摘要，发现受损项时返回非零状态；`cache repair` 只删除受损完整归档。`cache clean` 会删除 Pinset 识别的完整归档和断点文件，但保留缓存目录中的未知文件。`repair` 和 `clean` 都可先用 `--dry-run` 预览。

从联网机器复制 Node 官方归档和已审阅的 SHA-256 后，可在离线机器导入：

```shell
pinset cache import ./node-v24.0.0-linux-x64.tar.xz \
  --sha256 <64位SHA-256>
pinset install --locked
```

导入只接受普通文件，限制大小，并在原子写入内容寻址缓存前重新计算 SHA-256。哈希应来自已审阅的 `pinset.lock` 或上游校验清单。

pnpm/Bun 的 npm SRI 可直接从锁文件导入：

```shell
pinset cache import ./platform-package.tgz \
  --integrity 'sha512-<base64>'
pinset install --locked
```

## 查询、诊断、配置和卸载

### 常用查询

```shell
pinset list
pinset list node
pinset list node --available
pinset list --json
pinset outdated
pinset current rust --json
pinset which node --json
pinset doctor
pinset doctor --json
```

### 独立配置边界

Pinset 只把 `pinset.toml`、`pinset.lock` 和 `PINSET_HOME` 内的状态视为配置来源，不扫描、不解释也不导入其他运行时管理器的声明。迁移项目时请显式执行 `pinset init` 和 `pinset use <tool>@<selector>`，让 Pinset 重新解析并生成自己的可复现锁文件。

### 卸载与清理运行时版本

```shell
pinset uninstall node@20.19.0 --dry-run
pinset prune --dry-run
pinset prune --project ../another-project --dry-run
```

`uninstall` 只能接受精确版本。当前项目或全局仍引用该版本时默认拒绝；`--force` 只跳过引用保护，不会扩大到 Pinset 数据目录外，也不会删除没有有效 Pinset 安装收据的目录。

`prune` 默认保护全局和当前项目选择，只清理未引用且收据完整的版本。Pinset 不扫描整台磁盘寻找项目；需要保护其他项目时显式传入一个或多个 `--project <目录>`，目录不存在或无法找到 `pinset.toml` 时会停止清理。两个命令都支持 `--dry-run` 和 `--json`。

### 完整卸载 Pinset

Linux、macOS、WSL 先预览：

```bash
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/uninstall.sh |
  sh -s -- --dry-run
```

确认后删除 Pinset CLI、路由、配置、缓存和 Pinset 安装的全部语言运行时：

```bash
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/uninstall.sh |
  sh -s -- --yes
```

Windows 从 Release 下载 `uninstall.ps1`：

```powershell
./uninstall.ps1 -DryRun
./uninstall.ps1 -Yes
```

完整卸载不会搜索或删除项目中的 `pinset.toml`、`pinset.lock` 或 `.venv`，不会删除系统运行时或用户文件，也不会修改 shell profile。手动添加的 PATH 行和 `PINSET_*` 环境变量需要自行移除。

## 命令速查

| 命令 | 用途 |
| --- | --- |
| `pinset init` | 在当前目录创建最小 `pinset.toml` |
| `pinset global [<tool>@选择器]` | 查看或设置全局运行时默认版本 |
| `pinset use <tool>@选择器 [--global]` | 选择、锁定并默认安装运行时 |
| `pinset unset <tool> [--global]` | 清除选择，不卸载运行时 |
| `pinset install [<tool>@选择器]` | 安装锁定版本或独立预装一个版本 |
| `pinset current [<tool>] [--json]` | 显示当前解析结果、来源和路径 |
| `pinset which <命令> [--json]` | 显示某个命令最终使用的可执行文件 |
| `pinset exec [<tool>@精确版本] -- <命令>` | 通过所选运行时执行命令 |
| `pinset list [<tool>] [--available] [--json]` | 查看全部/指定本地版本或官方可用版本 |
| `pinset outdated [<tool>] [--json]` | 检查项目和全局选择的稳定版更新 |
| `pinset uninstall <tool>@精确版本 [--dry-run]` | 安全卸载一个 Pinset 管理的运行时 |
| `pinset prune [--project <目录>] [--dry-run]` | 清理未引用的受管运行时版本 |
| `pinset cache list/info/verify/repair/clean/import` | 管理、校验和修复下载缓存 |
| `pinset source ...` | 管理镜像、回退与连通性测试 |
| `pinset doctor [--json]` | 只读诊断配置、安装和 PATH |
| `pinset venv create/status/recreate` | 管理 Pinset 自有的项目 Python `.venv` |
| `pinset activate <shell>` | 输出当前 shell 的临时 PATH 激活代码 |
| `pinset completions <shell>` | 生成命令、Provider、嵌套子命令和常用参数补全脚本 |
| `pinset shim ...` | 查看、修复或迁移命令路由 |

所有命令的准确参数以 `pinset <命令> --help` 为准。

## Release 完整性验证

Release 提供：

- Linux x64、Windows x64、macOS Apple Silicon 归档；
- `install.sh`、`uninstall.sh`、`uninstall.ps1`；
- `SHA256SUMS`；
- `pinset-cli.cdx.json`、`pinset-core.cdx.json`、`pinset-shim.cdx.json` CycloneDX SBOM；
- GitHub Actions 生成的构建来源证明。

Linux 示例：

```bash
grep ' pinset-linux-x86_64.tar.gz$' SHA256SUMS | sha256sum -c -
gh attestation verify pinset-linux-x86_64.tar.gz \
  -R Future-Element/pinset
```

来源证明关联 Release 归档与构建它的仓库、提交和工作流，但不等于安全审计；仍应结合 SHA-256、SBOM 和自己的信任策略判断。

## 开发验证

项目要求 Rust 1.85 或更高版本。普通 push/Pull Request 的 CI 只执行 Linux、Windows、macOS release 构建与打包。发布工作流在 GitHub Actions Ubuntu 虚拟机执行以下质量门禁：

```shell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --locked -p pinset-cli -p pinset-shim
```

普通 CI 只在 Linux、Windows、macOS 隔离 Runner 中构建并打包 Pinset 自身；发布工作流额外执行格式、Clippy、单元测试、小型脚本测试、SBOM 和来源证明。除编译 Pinset 必需的 Rust 构建工具链外，任何 GitHub Actions 工作流都不再通过 Pinset 下载或执行待验证的 Node、Flutter、JDK、Rust 工具链、.NET SDK 等 Provider 工件。单元测试使用元数据夹具、小型假归档和安装收据覆盖选择器、锁文件、安装安全、跨 Provider 生命周期、路由和环境变量；完整真实验收脚本保留给发布后的隔离虚拟机，用于验证各 Provider 的 available、全局/项目覆盖、SDK 编译运行、生命周期查询和已安装版本复用。

Linux/WSL 构建产物：

```text
target/release/pinset
target/release/pinset-shim
```

Windows 构建产物是 `.exe`，不能直接作为 WSL/Linux 程序使用；需要在目标系统构建，或配置对应 Linux 交叉编译工具链。

## Beta 限制

- 当前版本支持 Node.js、pnpm、Bun、Go、CPython、Flutter stable SDK（含内置 Dart）、Eclipse Temurin JDK、Rust stable 和 Microsoft .NET SDK；其他 Java 分发、Rust beta/nightly 及 .NET preview/RC 尚未开放。
- Pinset Release 暂无 Linux arm64、macOS Intel 安装包。
- 项目不维护第三方 Homebrew Tap 或 Scoop Bucket；使用 curl、Release 归档或源码构建。
- Pinset 会校验 Node 官方 HTTPS `SHASUMS256.txt` 和 Go 官方下载索引中的 SHA-256，但 Beta 尚未验证 Node 清单的上游 OpenPGP 签名；pnpm/Bun 则校验 npm SHA-512 SRI 和 registry ECDSA 签名。
- Pinset 不自动修改 shell profile、系统 PATH 或 IDE 配置。
- 这是预发布版本，配置 schema 仍可能在 1.0 前直接调整；1.0 之前不提供迁移框架或兼容承诺。

## 文档

- [PRD](docs/PRD.md)：产品目标、用户流程、功能和安全约束；
- [Plans](docs/PLANS.md)：版本路线和后续计划；
- [Release Notes](docs/RELEASE_NOTES.md)：每个公开版本的用户可见变化和限制；
- [贡献指南](CONTRIBUTING.md)；
- [安全策略](SECURITY.md)。

## 许可证

Pinset 使用 [MIT License](LICENSE) 开源。
