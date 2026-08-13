# Pinset

[![CI](https://github.com/Future-Element/pinset/actions/workflows/ci.yml/badge.svg)](https://github.com/Future-Element/pinset/actions/workflows/ci.yml)
[![Release](https://github.com/Future-Element/pinset/actions/workflows/release.yml/badge.svg)](https://github.com/Future-Element/pinset/actions/workflows/release.yml)

Pinset 是一个本地优先的多语言运行时版本管理 CLI。目标是用一致的命令管理 Node.js、Go、Python、Flutter 等运行时，减少在 fnm、nvm、Go 工具链、uv、FVM 之间切换的学习和维护成本。

`v0.2.0` 在已验证的 Node.js 管理基础上新增独立的 pnpm 与 Bun Provider。

`v0.2.1` 修复 pnpm 10 独立可执行包被错误套用 pnpm 11 `dist/` 叠加层规则的问题，并覆盖项目 pnpm 10 与 Bun 1.2 的组合安装。

`v0.3.0` 新增 Go Provider，并将 Provider 的元数据、安装器和受管环境策略收敛到统一清单。

当前开发中的 `v0.4.0` 新增 Flutter stable SDK Provider，并将 SDK 内置 Dart 作为同一个锁定与路由单元。

- 全局 Node 默认版本、项目级 Node 覆盖，以及离开项目后恢复全局版本；
- `node@24.0.0`、`node@24`、`node@24.12`、`node@lts`、`node@current`；
- pnpm 10/11 与 Bun 1.x 的精确、主版本、主次版本、`latest`/`current` 选择器；
- Go 的精确、主版本、主次版本、`latest`/`current` 选择器；
- Flutter stable 的精确、主版本、主次版本、`latest`/`current` 选择器，以及对应的内置 Dart 版本；
- Windows x64、Linux x64、macOS Apple Silicon 的 Pinset Release；
- `node`、`npm`、`npx`、`corepack`、`pnpm`、`bun`、`bunx`、`go`、`gofmt`、`flutter`、`dart` 统一路由；
- 可提交的 `pinset.toml` 和 `pinset.lock`；
- Node SHA-256、npm SHA-512 SRI 与 registry ECDSA 签名校验、安全解压、事务安装、并发安装锁、断点续传和内容寻址缓存；
- 国内或企业镜像、有序回退、可选的可信元数据镜像和离线缓存导入；
- 中英文提示、旧 Node 管理器与 `.fvmrc` 配置导入、诊断、安全卸载；
- 三平台自动构建、真实 Node/pnpm/Bun/Go/Flutter/Dart 验收、CycloneDX SBOM 和 GitHub 构建来源证明。

## 快速安装

Linux x64、macOS Apple Silicon 和 WSL：

```bash
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/install.sh | sh
```

这条命令只安装 `pinset` 和通用路由器 `pinset-shim` 到 `$HOME/.local/bin`，不会自动安装 Node，也不会修改 shell profile。安装脚本会根据平台下载对应 Release 归档，并强制校验同一 Release 中的 `SHA256SUMS`。

将 Pinset 加入当前终端的 PATH：

```bash
export PATH="$HOME/.local/bin:$PATH"
pinset --version
```

长期使用可自行把 `export PATH=...` 写入 `~/.bashrc` 或 `~/.zshrc`。Pinset 不会擅自修改这些文件。

固定安装最新已发布的 `v0.3.0`：

```bash
curl -fsSL https://github.com/Future-Element/pinset/releases/download/v0.3.0/install.sh | sh
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
```

`pinset.lock` 保存解析后的精确版本、平台归档、完整性值和来源信息。Node、Go 和 Flutter 使用官方 SHA-256；pnpm/Bun 使用 npm 包的 SHA-512 SRI，并在生成锁文件前验证 registry ECDSA 签名。建议把 `pinset.toml` 与 `pinset.lock` 一起提交。

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

Flutter 与 Dart 始终从同一套 SDK 的 `bin` 路由。受管命令获得对应的 `FLUTTER_ROOT`；未显式设置时，Pinset 还会使用 `FLUTTER_SUPPRESS_ANALYTICS=true`，用户已有值保持不变。`.fvmrc` 可通过 `pinset import --dry-run` 预览，并通过 `pinset import --apply --from fvm` 显式导入，原文件与 FVM 缓存不会被修改。

为保持锁文件可复现，受管 SDK 不允许原地执行 `flutter upgrade`、`flutter downgrade` 或 `flutter channel`。请选择新版本，例如 `pinset use flutter@3.47`，由 Pinset 解析并安装另一套 SDK。

Pinset 不管理 Android SDK/NDK、JDK、Xcode、CocoaPods、模拟器、设备、Flutter/Dart 项目依赖或 pub 缓存。

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
pinset exec go@1.25 -- go version
pinset exec -- flutter --version
pinset exec -- dart --version
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

正常执行 `global`、`use` 或 `install` 后，各 Provider 会注册自己的通用路由：Node 注册四个命令，pnpm 注册 `pnpm`，Bun 注册 `bun` 和 `bunx`，Go 注册 `go` 和 `gofmt`，Flutter 注册 `flutter` 和 `dart`。curl 安装器本身仍然保持运行时中立，只安装 Pinset 和通用调度器。

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
pinset shim install --provider flutter
pinset shim migrate --provider node
```

这些命令不会覆盖同名的用户文件、fnm/nvm/Volta 入口或系统命令。`doctor` 会报告 PATH 中的遮挡、旧 shim 和外部管理器。

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
pinset cache clean
```

`cache clean` 会删除 Pinset 识别的完整归档和断点文件，但保留缓存目录中的未知文件。

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

## 查询、诊断、迁移和卸载

### 常用查询

```shell
pinset list node
pinset list node --available
pinset current
pinset which node
pinset doctor
pinset doctor --json
```

### 从旧管理器迁移

只读扫描 `.nvmrc`、`.node-version`、Volta、asdf 和 mise：

```shell
pinset import --dry-run
```

确认后导入项目或全局选择，原文件始终保留：

```shell
pinset import --apply --from nvm
pinset import --apply --from volta --global --no-install
```

多个旧配置给出不同版本时，需要通过 `--from` 选择来源。

### 卸载一个 Node 版本

```shell
pinset uninstall node@20.19.0
```

只能卸载精确版本。当前项目或全局仍引用该版本时默认拒绝；`--force` 只跳过引用保护，不会扩大到 Pinset 数据目录外，也不会删除没有有效 Pinset 安装收据的目录。

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

完整卸载不会搜索或删除项目中的 `pinset.toml`、`pinset.lock`，不会删除系统 Node、nvm/fnm/Volta 文件，也不会修改 shell profile。手动添加的 PATH 行和 `PINSET_*` 环境变量需要自行移除。

## 命令速查

| 命令 | 用途 |
| --- | --- |
| `pinset init` | 在当前目录创建最小 `pinset.toml` |
| `pinset global [<tool>@选择器]` | 查看或设置全局运行时默认版本 |
| `pinset use <tool>@选择器 [--global]` | 选择、锁定并默认安装运行时 |
| `pinset unset <tool> [--global]` | 清除选择，不卸载运行时 |
| `pinset install [<tool>@选择器]` | 安装锁定版本或独立预装一个版本 |
| `pinset current [<tool>]` | 显示当前解析结果、来源和路径 |
| `pinset which <命令>` | 显示某个命令最终使用的可执行文件 |
| `pinset exec [<tool>@精确版本] -- <命令>` | 通过所选运行时执行命令 |
| `pinset list <tool> [--available]` | 查看本地或官方可用版本 |
| `pinset uninstall <tool>@精确版本` | 安全卸载一个 Pinset 管理的运行时 |
| `pinset cache list/clean/import` | 管理下载缓存和离线归档 |
| `pinset source ...` | 管理镜像、回退与连通性测试 |
| `pinset doctor [--json]` | 只读诊断配置、安装和 PATH |
| `pinset import --dry-run/--apply` | 检测或导入旧 Node 管理器配置与 `.fvmrc` |
| `pinset activate <shell>` | 输出当前 shell 的临时 PATH 激活代码 |
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

项目要求 Rust 1.85 或更高版本。Pull Request 的 Quality 工作流在 GitHub Actions Ubuntu 虚拟机执行：

```shell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --locked -p pinset-cli -p pinset-shim
```

真实运行时验收只在 GitHub Actions 隔离 Runner 中执行。工作流会在 Linux、Windows、macOS 安装 Node、pnpm、Bun、Go 和两套 Flutter SDK，验证 available、全局/项目组合、项目覆盖、跨 Provider 子进程 PATH、`GOROOT`、默认 `GOTOOLCHAIN=local`、Flutter/Dart 同源、`FLUTTER_ROOT`、`.fvmrc` 导入、SDK 重用、原地升级拦截，以及受管命令的 `pinset exec`/shim 调用方式。

Linux/WSL 构建产物：

```text
target/release/pinset
target/release/pinset-shim
```

Windows 构建产物是 `.exe`，不能直接作为 WSL/Linux 程序使用；需要在目标系统构建，或配置对应 Linux 交叉编译工具链。

## Beta 限制

- 当前开发版本支持 Node.js、pnpm、Bun、Go 和 Flutter stable SDK（含内置 Dart）；Python、Java、Rust 和其他 Provider 尚未开放。
- Pinset Release 暂无 Linux arm64、macOS Intel 安装包。
- 项目不维护第三方 Homebrew Tap 或 Scoop Bucket；使用 curl、Release 归档或源码构建。
- Pinset 会校验 Node 官方 HTTPS `SHASUMS256.txt` 和 Go 官方下载索引中的 SHA-256，但 Beta 尚未验证 Node 清单的上游 OpenPGP 签名；pnpm/Bun 则校验 npm SHA-512 SRI 和 registry ECDSA 签名。
- Pinset 不自动修改 shell profile、系统 PATH 或 IDE 配置。
- 这是预发布版本，配置 schema 仍可能在稳定版前调整；任何迁移都会在 Release Notes 中说明。

## 文档

- [PRD](docs/PRD.md)：产品目标、用户流程、功能和安全约束；
- [Plans](docs/PLANS.md)：版本路线和后续计划；
- [Release Notes](docs/RELEASE_NOTES.md)：每个公开版本的用户可见变化和限制；
- [贡献指南](CONTRIBUTING.md)；
- [安全策略](SECURITY.md)。

## 许可证

Pinset 使用 [MIT License](LICENSE) 开源。
