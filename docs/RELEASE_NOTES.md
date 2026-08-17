# Pinset Release Notes

## v0.9.0（开发中）

- 阶段：运行时生命周期管理；
- `pinset list` 可一次列出全部 Provider 的已安装版本，`list`、`current` 与 `which` 增加稳定 JSON 输出；
- 新增 `pinset outdated`，分别检查当前项目和全局选择的最新稳定版本；
- `pinset uninstall` 增加 `--dry-run` 与 `--json`；新增 `pinset prune` 清理未被全局、当前项目或显式附加项目引用的受管版本；
- pnpm/Bun 精确版本卸载改为本地 SemVer 与支持窗口校验，不再为删除或预览本地安装访问 npm registry；
- 新增 `pinset cache info|verify|repair`，并为 `cache clean` 和 `cache repair` 增加安全预览；
- 新增 Bash、Zsh、Fish 与 PowerShell 命令补全输出，覆盖 Provider、嵌套子命令和常用参数；
- 修复 `current rust` 这类 Provider 名称与首个命令名称不同的查询；
- Release Quality 使用无网络的安装收据、假命令与缓存夹具统一验证全部 Provider 的生命周期契约；发布后隔离虚拟机脚本增加 JSON 列表、更新检查、卸载/清理预览、缓存统计和补全验收；
- 继续保持普通 CI 只构建和打包 Pinset，真实大型运行时仅在发布后隔离虚拟机验收；
- 1.0 之前不开发配置或锁文件迁移框架，也不承诺跨预发布版本迁移。

## v0.8.0

- 发布日期：`2026-08-14`；
- 阶段：Microsoft .NET SDK Provider；
- 新增 `pinset list dotnet --available`、`global`、`use`、`install`、`current`、`which`、`exec`、shim 和卸载生命周期；
- 支持 `latest`/`current`、`lts`、主版本、`major.minor` 通道和精确 `x.y.zzz` SDK 版本选择器；
- 使用 Microsoft 官方 release index 和通道 metadata，只接受仍处于 active/maintenance 阶段的 GA LTS/STS SDK，排除 preview、RC、go-live 和 EOL；
- 锁定 Windows x64、Linux x64、macOS x64/arm64 的官方 SDK 归档与 SHA-512，并验证 URL、RID、格式和四平台完整性；
- 路由 `dotnet`，为受管进程设置匹配 SDK 的 `DOTNET_ROOT`，保留 `DOTNET_CLI_HOME`、`NUGET_PACKAGES`、NuGet 与遥测配置；
- 除编译 Pinset 必需的 Rust 构建工具链外，GitHub Actions 不再通过 Pinset 下载或执行任何 Provider 真实运行时；普通 CI 只保留三平台构建与打包，Release 保留 Quality、SBOM 和构建来源证明，完整真实验收脚本供发布后在隔离虚拟机执行。

## v0.7.0

- 发布日期：`2026-08-14`；
- GitHub Release：[v0.7.0](https://github.com/Future-Element/pinset/releases/tag/v0.7.0)
- 阶段：Rust stable Provider；
- 新增 `pinset list rust --available`、`global`、`use`、`install`、`current`、`which`、`exec`、shim 和卸载生命周期；
- 支持 `stable`/`latest`/`current`、主版本、主次版本和精确 stable 版本选择器；
- 使用 Rust 官方 `manifests.txt` 和经过 SHA-256 校验的 v2 release manifest，锁定 Windows x64、Linux x64、macOS x64/arm64 的组合工具链归档与 SHA-256；
- 安装官方 `default` profile，路由 `rustc`、`cargo`、`rustdoc`、`rustfmt`、`cargo-fmt`、`clippy-driver` 和 `cargo-clippy`；
- 保持锁文件 schema 2，记录 stable channel、manifest date、manifest SHA-256、profile 和组件边界；
- 不接管外部 Rust 管理器或其目录，不修改 shell profile，并保留用户的 `CARGO_HOME`、`RUSTUP_HOME` 与 `RUSTFLAGS`；
- Quality 使用官方 manifest 夹具和假归档；发布时完成 Linux、Windows、macOS 的真实 Rust 下载、编译和命令路由验证。自 v0.8.0 起，真实运行时验收移出 GitHub Actions，改由发布后隔离虚拟机执行。

## v0.6.1

- 发布日期：`2026-08-14`
- GitHub Release：[v0.6.1](https://github.com/Future-Element/pinset/releases/tag/v0.6.1)
- 阶段：Provider 命令路由优先级补丁；
- 修复路由目录已存在于 PATH、但 `java` 等命令被更早的系统路径遮挡时仍显示为已生效的问题；
- Provider 注册后检查每个受管命令真正命中的首个 PATH 文件，并明确列出 `java=/usr/bin/java` 一类遮挡来源；
- 根据当前 zsh、bash、fish 或 PowerShell 输出可直接执行的 `pinset activate` 命令；
- 安装脚本在安装目录已位于 PATH 但不是首项时提示将其前置，仍不自动修改 shell profile。

## v0.6.0

- 发布日期：`2026-08-14`
- GitHub Release：[v0.6.0](https://github.com/Future-Element/pinset/releases/tag/v0.6.0)
- 阶段：Eclipse Temurin JDK Provider；
- 修复 v0.5.0 Python Provider 只注册 `python`/`python3`、导致全局和直接调用缺少 `pip` 的问题；新增 `pip`/`pip3` shim，并统一通过所选解释器执行 `python -m pip`；
- 新增 `pinset list java --available`、`global`、`use`、`install`、`current`、`which`、`exec`、shim 和卸载生命周期；
- 首期只接受 Eclipse Temurin、JDK、HotSpot、normal heap、GA 正式版本，支持 `latest`/`current`、`lts`、Feature、Update 和精确 `+build` 选择器；
- 使用 Adoptium API v3 锁定 Windows x64、Linux x64、macOS x64/arm64 的最终 GitHub Release 归档、SHA-256、release identity 和签名链接；
- 路由 `java`、`javac`、`jar`、`javadoc`、`javap`、`keytool`、`jshell`，受管子进程获得匹配 JDK 的 `JAVA_HOME`，macOS 正确使用 `Contents/Home`；
- 保持锁文件 schema 2，通过 Provider metadata 明确记录分发、vendor、JDK/JRE、JVM、GA/EA 等身份，不要求旧锁文件迁移；
- 保留用户的 Java 相关环境选项并由 `doctor` 提示其影响，不修改 shell profile、系统 JDK、项目 toolchain 或 `cacerts`；
- Quality 使用小型夹具和假归档；workflow dispatch 与 Release 在 Linux、Windows、macOS 虚拟机下载一个真实 Temurin JDK，编译并运行最小 Java 程序。Flutter SDK 仍不进入 GitHub Actions 真实下载。

## v0.5.0

- 发布日期：`2026-08-14`
- GitHub Release：[v0.5.0](https://github.com/Future-Element/pinset/releases/tag/v0.5.0)
- 阶段：CPython Provider / 独立项目虚拟环境；
- 新增 CPython Provider，支持精确版本、主版本、主次版本、`latest`/`current` 和 `pinset list python --available`；
- 使用官方 `python-build-standalone` 版本注册表和 `install_only` 工件，锁文件记录 CPython 版本、构建 ID、发行变体、四平台归档身份和 SHA-256；
- 项目选择 Python 后自动创建 Pinset 自有的 `.venv`，`python`、`python3` 和 `pinset exec -- <环境命令>` 自动路由到项目环境，无需手动激活；
- 新增 `pinset venv create/status/recreate`；未标记、版本不匹配、符号链接或损坏的 `.venv` 不会被接管，只有显式 `recreate` 可在所有权验证后重建；
- 受管 Python 项目进程设置 `VIRTUAL_ENV` 并移除 `PYTHONHOME`；全局 Python 仍路由独立安装的基础解释器；
- 移除其他运行时管理器的配置检测和迁移命令；Pinset 只读取自己的配置、锁文件和状态；
- `doctor --json` schema 更新为 2，不再输出其他管理器配置检测结果；
- Windows x64 已使用真实 `3.14.7+20260807` 工件验证下载、安装、`.venv` 创建、shim 无激活路由、pip、环境重用和显式重建；Release 工作流要求 Linux、Windows 与 macOS 构建和真实 Python 验收全部通过后才创建 GitHub Release。

## v0.4.2

- 发布日期：`2026-08-14`
- 阶段：大型运行时下载可靠性补丁
- GitHub Release：[v0.4.2](https://github.com/Future-Element/pinset/releases/tag/v0.4.2)
- 同一下载源遇到请求或响应体读取中断时自动进行最多 3 次下载尝试；
- 重试复用按完整性摘要隔离的 `.part` 文件，并使用经过 `Content-Range` 校验的 HTTP Range 请求续传，避免重新下载完整 Flutter SDK；
- 校验和不匹配、超限和不安全缓存条目仍然立即失败，不会作为网络抖动重试；
- 自动化测试使用小型本地 HTTP 故障注入归档，不在 GitHub Actions 下载 Flutter 大包。

## v0.4.1

- 发布日期：`2026-08-14`
- 阶段：macOS 发布包可移植性修复
- GitHub Release：[v0.4.1](https://github.com/Future-Element/pinset/releases/tag/v0.4.1)
- 修复 macOS Apple Silicon 发布二进制动态依赖构建机 Homebrew `liblzma`、导致未安装 `xz` 的用户无法启动 Pinset 的问题；
- 将 `xz2/lzma-sys` 静态链接进 Pinset，用户无需安装 Homebrew 或系统级 `xz`；
- 在 CI 和 Release 的 macOS 构建后使用 `otool -L` 检查动态依赖，只允许 macOS 系统库与系统 Framework，阻止构建机包管理器路径进入发布归档。

## v0.4.0

- 发布日期：`2026-08-13`
- 阶段：Flutter stable SDK Provider
- GitHub Release：[v0.4.0](https://github.com/Future-Element/pinset/releases/tag/v0.4.0)
- 新增 Flutter stable SDK Provider，支持精确版本、主版本、主次版本、`latest`/`current` 与 `list flutter --available`；
- 将 Flutter 与其内置 Dart 锁定为同一运行时单元，记录渠道、Dart 版本、release hash、四平台归档身份和 SHA-256；
- 新增 `flutter`、`dart` 路由和受管 `FLUTTER_ROOT`，保留用户显式设置的 `FLUTTER_SUPPRESS_ANALYTICS`；
- 支持预览和显式导入 `.fvmrc`，不修改旧文件、不接管 FVM 缓存；
- 在启动受管 SDK 前拒绝 `flutter upgrade`、`flutter downgrade` 和 `flutter channel`；
- GitHub Actions 保留三平台构建、Quality、Flutter 元数据/锁文件/安装与路由自动化测试，但不再下载单个超过 1.8 GiB 的真实 Flutter SDK；完整 Flutter/Dart 验收脚本保留给发布后的隔离虚拟机测试。

## v0.3.0

- 发布日期：`2026-08-13`
- 阶段：Go Provider / Native Provider 通用化
- GitHub Release：[v0.3.0](https://github.com/Future-Element/pinset/releases/tag/v0.3.0)
- 新增 `pinset list go --available`、`go@latest`、主版本、主次版本和精确版本选择；
- 新增 Go 官方下载 JSON、四平台归档身份和逐工件 SHA-256 锁定；
- 新增 `go`、`gofmt` 命令路由，以及受管 `GOROOT` 和默认 `GOTOOLCHAIN=local`；
- Provider 清单开始统一描述元数据、安装器、命令目录和受管环境策略；
- GitHub Actions 三平台真实运行时验收扩展到 Go；

## v0.2.1

- 发布日期：`2026-08-13`
- 阶段：多 Provider Beta 补丁
- GitHub Release：[v0.2.1](https://github.com/Future-Element/pinset/releases/tag/v0.2.1)
- 修复 pnpm 10 官方平台包为单个独立可执行文件、没有 pnpm 11 `dist/` 叠加层时的安装失败；
- 增加全局 `pnpm@latest`、项目 `pnpm@10` 与项目 `bun@1.2` 组合安装的三平台虚拟机回归验收；
- 不改变 pnpm 11 的 `@pnpm/exe` overlay、npm registry 签名或 SHA-512 校验。

## v0.2.0

- 发布日期：`2026-08-12`
- 阶段：多 Provider Beta / GitHub Release
- 许可证：MIT
- Pinset 归档：Linux x64、Windows x64、macOS Apple Silicon
- 运行时：Node.js、pnpm 10/11、Bun 1.x
- GitHub Release：[v0.2.0](https://github.com/Future-Element/pinset/releases/tag/v0.2.0)

### 更新内容

- 新增独立 pnpm Provider：稳定版 10/11，命令 `pnpm`；
- 新增独立 Bun Provider：稳定版 1.x，命令 `bun`/`bunx`，x64 自动选择 AVX2 或 baseline；
- 新增 `pinset list pnpm --available` 与 `pinset list bun --available`；
- 项目/全局配置与锁文件升级为 schema 2，可同时保存 Node、pnpm、Bun，并继续读取 schema 1；
- npm 平台包在锁定阶段验证 registry ECDSA 签名，安装阶段验证 SHA-512 SRI；
- 安装器新增安全 `.tar.gz` 解压，缓存新增 SHA-512 分仓与 `cache import --integrity`；
- 子进程使用多 Provider 组合 PATH，并排除 Pinset shim，支持工具之间安全调用；
- 本地版本列表和卸载扩展到 pnpm/Bun。

该版本已由 Linux、Windows 与 macOS GitHub Actions Release Runner 完成真实 Node/pnpm/Bun 安装与执行验收。

## v0.1.0-beta.1

- 发布日期：`2026-08-12`
- 阶段：Node-first Beta / GitHub prerelease
- 许可证：MIT
- Pinset 归档：Linux x64、Windows x64、macOS Apple Silicon
- Node 目标：Windows x64、Linux x64、macOS x64、macOS arm64
- GitHub Release：[v0.1.0-beta.1](https://github.com/Future-Element/pinset/releases/tag/v0.1.0-beta.1)

### 更新内容

#### 安装命令

Linux x64、macOS Apple Silicon 和 WSL 的推荐入口变为：

```bash
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/install.sh | sh
```

安装器内置当前推荐版本，也支持通过 `--version` 或 `PINSET_VERSION` 安装指定版本。下载的 Release 归档仍会使用同一版本的 `SHA256SUMS` 校验。

固定 Beta Release：

```bash
curl -fsSL https://github.com/Future-Element/pinset/releases/download/v0.1.0-beta.1/install.sh | sh
```

curl 仍然只安装 `pinset` 与通用 `pinset-shim`，不安装 Node，不创建 node/npm/npx/corepack 文件，不修改 shell profile。

#### 下载进度不再逐行刷屏

- TTY 进度只在同一行刷新；
- 根据终端列数动态调整进度条；
- 使用 Unicode 显示宽度正确处理中文；
- 长归档名从中间截断；
- 保留终端最后一列，避免自动折行；
- 非交互环境保持有限的开始/完成日志。

#### 断点续传和并发安装

- 相同 `tool + version + target` 使用跨进程文件锁；
- 多个 Pinset 进程同时安装同一 Node 时只执行一次下载和提交；
- 未完成归档保存在按预期 SHA-256 命名的断点缓存中；
- 重试发送 HTTP Range，并验证 `Content-Range` 起点；
- 服务端忽略 Range 或拒绝范围时安全从头下载；
- SHA-256 不匹配会删除不可信断点并停止安装。

#### 离线缓存导入

新增：

```shell
pinset cache import <archive> --sha256 <64位SHA-256>
```

导入时会检查文件类型和大小，并重新计算 SHA-256。成功后可通过 `pinset install --locked` 离线使用。`cache clean` 会同时清理完整归档和已识别的断点文件。

#### 国内和企业镜像

普通自定义镜像仍只替换归档传输：

```shell
pinset source add node cn-mirror --base-url https://mirror.example/node/
pinset source use node cn-mirror
pinset source fallback node official
```

新增可信元数据镜像，可同时读取 Node `index.json` 和 `SHASUMS256.txt`：

```shell
pinset source add node cn-trusted \
  --base-url https://mirror.example/node/ \
  --trust-metadata
pinset source use node cn-trusted
```

`--trust-metadata` 只允许 HTTPS，并在 `source list` 中标记。使用后，版本和校验信息都由该镜像提供，因此只适合经过审阅或由组织管理的镜像。

#### Node 版本管理

- 全局版本：`pinset global node@lts`；
- 项目版本：`pinset init && pinset use node@22`；
- 独立预装：`pinset install node@20`；
- 精确、主版本、主次版本、LTS、Current 选择器；
- 项目 > 全局 > 系统 PATH 的统一解析；
- `node`、`npm`、`npx`、`corepack` 自动命令路由；
- `pinset exec` 和 PATH 直接执行使用相同 Node bin；
- 检测并导入旧 nvm/Volta/asdf/mise 配置；
- 单版本安全卸载和完整 Pinset 卸载；
- 英文与简体中文提示。

#### 发布文件和校验

Release 工作流新增：

- Linux、Windows、macOS 各自隔离构建；
- 每个平台安装真实 Node 24.0.0 全局版本和 22.0.0 项目版本；
- 每个平台验证 node/npm/npx/corepack 的 `pinset exec` 与 PATH 直接调用；
- 发布 `pinset-cli.cdx.json`、`pinset-core.cdx.json`、`pinset-shim.cdx.json`；
- 对三平台归档生成 GitHub 构建来源证明；
- 对安装器、卸载器、校验和与 SBOM 生成来源证明；
- `SHA256SUMS` 覆盖全部公开资产。

### 升级

重复运行短安装命令会原子替换 Pinset 的两个二进制，不会删除配置、缓存或已安装 Node：

```bash
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/install.sh | sh
```

原有 `pinset.toml`、`pinset.lock`、全局选择和 Node 安装继续使用 schema 1，无需项目迁移。来自早期 `$PINSET_HOME/shims` 布局的用户可以执行：

```shell
pinset shim migrate --provider node
pinset doctor
```

### 完整卸载

Linux、macOS、WSL 先预览：

```bash
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/uninstall.sh |
  sh -s -- --dry-run
```

确认后：

```bash
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/uninstall.sh |
  sh -s -- --yes
```

Windows 从 Release 下载 `uninstall.ps1`，依次执行 `./uninstall.ps1 -DryRun` 与 `./uninstall.ps1 -Yes`。

### 验证记录

发布前完成了以下本地检查：

- Rust 格式检查；
- workspace 全目标、全 feature 严格 Clippy；
- workspace 全 feature 测试；
- locked release build；
- POSIX 安装/卸载脚本隔离测试；
- PowerShell 卸载器隔离测试；
- shim 依赖图和 `git diff --check`。

这些本地检查使用临时目录、假归档、本地 HTTP 和假运行时，没有在开发机安装真实 Node。Rust workspace 共 127 项测试通过。

main Quality [run 31584619265](https://github.com/Future-Element/pinset/actions/runs/31584619265) 成功。tag Release [run 31584751431](https://github.com/Future-Element/pinset/actions/runs/31584751431) 的 Release Quality、Linux、Windows、macOS 和发布任务全部成功；每个平台都在临时 `PINSET_HOME` 中测试了 Node 24.0.0 全局版本、Node 22.0.0 项目版本，以及通过 Pinset 和 PATH 调用 node/npm/npx/corepack。

发布后重新下载全部 10 个公开资产：

- `SHA256SUMS` 的 9 个条目全部复算一致；
- Linux/macOS 归档各只包含 `pinset` 与 `pinset-shim`；
- Windows ZIP 只包含两个 `.exe`，公开 CLI 输出 `pinset 0.1.0-beta.1`；
- 3 份 CycloneDX JSON 均可解析；
- Linux、Windows、macOS 三个归档的 GitHub attestation 均验证成功；
- 公开安装/卸载脚本内容与 tag 一致，`install.sh` 默认版本为 `0.1.0-beta.1`。

### 已知限制

- 当前只完整支持 Node.js，未接入 Python 和 Flutter；
- Pinset 暂无 Linux arm64 和 macOS Intel 官方归档；
- 不维护 Homebrew Tap、Scoop Bucket 或其他第三方分发仓库；
- Pinset 校验 Node HTTPS SHASUMS 中的 SHA-256，但尚未验证上游 OpenPGP 签名；
- 不自动修改 shell profile、系统 PATH 或 IDE；
- 这是预发布版本，schema 和兼容承诺将在稳定版前冻结。

## v0.1.0-alpha.6

- 发布日期：`2026-08-12`
- GitHub Release：[v0.1.0-alpha.6](https://github.com/Future-Element/pinset/releases/tag/v0.1.0-alpha.6)

新增 Linux/macOS/WSL 与 Windows 完整卸载脚本；支持 dry-run、二次确认、自定义 `PINSET_HOME` 额外授权、Provider 路由所有权验证，以及清理 Pinset 安装的全部语言运行时。卸载器不搜索项目文件、不改 profile、不删除外部管理器或系统运行时。

同时修复 alpha.5 在不同 Shell 和平台测试中发现的兼容问题。公开 Release 包含三平台归档、安装器、两个卸载器和 SHA256SUMS。

## v0.1.0-alpha.5

- 发布日期：`2026-08-12`
- GitHub Release：[v0.1.0-alpha.5](https://github.com/Future-Element/pinset/releases/tag/v0.1.0-alpha.5)

Node Provider 自动注册 `node`、`npm`、`npx`、`corepack`，正常使用不再要求先执行 `shim install`。新增独立版本安装、项目/全局选择清除、旧配置导入、旧 shim 迁移和 Bash/Zsh/Fish/PowerShell 临时激活。

curl 安装器只安装 Pinset 与通用调度器，保持对未来 Provider 中立。

## v0.1.0-alpha.4

- 发布日期：`2026-08-12`
- GitHub Release：[v0.1.0-alpha.4](https://github.com/Future-Element/pinset/releases/tag/v0.1.0-alpha.4)

新增 `pinset global` 命令、项目覆盖说明、shim 路径查看和下载进度条，并补充全局与项目 Node 的使用文档。

## v0.1.0-alpha.3

- 发布日期：`2026-08-12`
- GitHub Release：[v0.1.0-alpha.3](https://github.com/Future-Element/pinset/releases/tag/v0.1.0-alpha.3)

新增主版本/主次版本/LTS/Current 浮动选择、本地和官方版本列表、精确版本卸载、内容寻址缓存、来源测试、`doctor --json`、旧管理器只读检测和一次性精确版本执行。

## v0.1.0-alpha.2

- 发布日期：`2026-08-12`
- GitHub Release：[v0.1.0-alpha.2](https://github.com/Future-Element/pinset/releases/tag/v0.1.0-alpha.2)

新增全局 Node 选择、项目 > 全局 > 系统 PATH 的统一解析、Node 子进程 PATH 修复，以及英文/简体中文界面。Ubuntu 隔离 Runner 首次测试了真实的全局与项目 Node 切换。

## v0.1.0-alpha.1

- 发布日期：`2026-08-11`
- GitHub Release：[v0.1.0-alpha.1](https://github.com/Future-Element/pinset/releases/tag/v0.1.0-alpha.1)

交付 Node project MVP：精确版本、`pinset.toml`、`pinset.lock`、官方 SHASUMS SHA-256、镜像、安全 ZIP/TAR.XZ 解压、事务安装、初版 shim，以及 Linux x64、Windows x64、macOS Apple Silicon 自动 Release。
