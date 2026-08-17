# Pinset Plans

当前规划版本：`v0.9.0`

当前发布候选：`v0.9.0`

最新已发布版本：`v0.8.0`

更新时间：`2026-08-17`

## 路线原则

Pinset 将继续保持“一个工具统一选择、安装、锁定和路由开发运行时”的产品边界。后续版本每次只引入一个主要运行时，并在该版本中同步补齐可复用的 Provider 能力，避免多个生态同时开发导致配置、锁文件和命令路由失去一致性。

规划顺序为：

1. `v0.3.0`：Go，并完成 Native Provider 通用化。
2. `v0.4.0`：Flutter SDK，同时管理其内置 Dart。
3. `v0.5.0`：CPython。
4. `v0.6.0`：Java，首期仅支持 Eclipse Temurin JDK。
5. `v0.7.0`：Rust 原生 Provider，使用 Rust 官方 stable v2 manifests 与 default profile 工具链。
6. `v0.8.0`：Microsoft .NET SDK Provider，使用官方 release metadata 与逐平台 SHA-512。
7. `v0.9.0`：补齐运行时安装、更新检查、卸载、清理和下载缓存的完整生命周期。

各版本暂不承诺日期。普通 CI 只负责三平台构建和打包；Release 工作流保留质量门禁与发布供应链产物。除编译 Pinset 必需的 Rust 构建工具链外，Actions 不再通过 Pinset 下载 Node、Flutter、JDK、Rust 工具链、.NET SDK 等 Provider 工件。完整验收脚本继续保留，发布后由隔离虚拟机执行，避免持续占用托管 Runner。

## 版本路线

| 版本 | 状态 | 主要范围 |
| --- | --- | --- |
| `v0.1.0-alpha.1` | 已发布 | 项目配置、锁文件、精确版本安装、镜像、shim 和三平台构建 |
| `v0.1.0-alpha.2` | 已发布 | 全局版本、项目覆盖、系统 PATH 回退和中文界面 |
| `v0.1.0-alpha.3` | 已发布 | 浮动版本、本地版本列表、卸载、缓存、诊断和旧配置检测 |
| `v0.1.0-alpha.4` | 已发布 | `global` 命令、下载进度和使用文档 |
| `v0.1.0-alpha.5` | 已发布 | 自动注册 Node/npm/npx/corepack、独立安装和旧 shim 迁移 |
| `v0.1.0-alpha.6` | 已发布 | 完整卸载脚本和跨 Shell 修复 |
| `v0.1.0-beta.1` | 已发布 | 简短安装命令、断点续传、可信镜像、离线导入和三平台 Node 测试 |
| `v0.2.0` | 已发布 | pnpm、Bun Provider，通用 Provider/锁文件/命令路由基础 |
| `v0.2.1` | 已发布 | pnpm、Bun 项目版本切换和安装路径修复 |
| `v0.3.0` | 已发布 | Go Provider、Native Provider 通用化 |
| `v0.4.0` | 已发布 | Flutter SDK Provider、内置 Dart 路由 |
| `v0.4.1` | 已发布 | macOS liblzma 静态链接与发布依赖门禁 |
| `v0.4.2` | 已发布 | 大型运行时下载自动重试与断点续传 |
| `v0.5.0` | 已发布 | CPython Provider、项目 `.venv` 与无激活路由 |
| `v0.6.0` | 已发布 | Eclipse Temurin JDK Provider、Python pip/pip3 路由修复 |
| `v0.6.1` | 已发布 | 检测 PATH 中被系统命令遮挡的 Provider 路由并给出当前 Shell 激活命令 |
| `v0.7.0` | 已发布 | Rust stable Provider、官方 v2 manifest 锁定、default profile 工具链与命令路由 |
| `v0.8.0` | 已发布 | Microsoft .NET SDK Provider、受支持 GA 通道、官方 SHA-512 锁定与 `dotnet` 路由 |
| `v0.9.0` | 开发中 | 跨 Provider 已安装列表、更新检查、安全卸载/清理、缓存统计/校验/修复与 JSON 输出 |

## v0.3.0 — Go Provider 与 Native Provider 通用化

### 目标

- 支持 `pinset list go --available` 查看可安装版本。
- 支持 `pinset use go@<selector>`、`pinset global go@<selector>` 和现有项目锁定流程。
- 支持精确版本、`latest`、主版本和主次版本选择器，并将最终解析结果写入锁文件。
- 路由 `go`、`gofmt` 等 Go 安装包提供的命令。
- 将 Node.js、pnpm、Bun 中仍然分散的安装、激活和命令路由逻辑收敛到 Native Provider 公共能力。

### 技术范围

- 使用 Go 官方版本元数据、归档和 SHA-256 校验信息。
- 按平台、架构和归档类型选择工件，复用安全解压、缓存、并发锁和原子安装流程。
- 由 Pinset 为受管子进程设置正确的 `GOROOT` 和命令路径。
- 保留用户已有的 `GOPATH`、`GOMODCACHE`、`GOCACHE` 等工作区与缓存配置。
- 默认只在 Pinset 管理的子进程中使用 `GOTOOLCHAIN=local`，防止 Go 根据项目声明静默下载另一套工具链；如果用户显式设置该变量，则尊重用户设置并由 `doctor` 给出可诊断提示。
- Provider 元数据统一描述版本选择器、平台目标、下载工件、校验方式、命令入口、环境变量和安装后验证。

### 不包含

- Go module 依赖管理、代理服务或私有模块凭据管理。
- 自动修改 `go.mod`、`go.work`。
- 交叉编译工具链、TinyGo、GCC 或系统 C 工具链管理。

### 验收条件

- Ubuntu、Windows、macOS 虚拟机均能执行 available、global、use 和项目锁文件场景。
- 精确版本和模糊选择器解析结果一致、可复现。
- `go version` 与锁文件版本一致，项目版本能够覆盖全局版本。
- 已安装版本可离线复用，损坏缓存和不匹配校验值会明确失败。
- Node.js、pnpm、Bun 既有行为没有回归。

## v0.4.0 — Flutter SDK Provider

实现状态：核心 Provider、锁文件、安装与路由已完成并发布；Actions 覆盖三平台构建和 Flutter 自动化测试，真实 SDK 大包验收转由发布后的隔离虚拟机执行。

### 目标

- 管理 Flutter SDK，并将其捆绑的 Dart 视为同一个不可拆分的运行时单元。
- 支持 `pinset list flutter --available`、`use`、`global` 和项目锁定。
- 路由 `flutter`、`dart`，并确保两者始终来自同一套 Flutter SDK。
- 首期支持 stable 渠道、精确版本、主版本和主次版本选择器。

### 技术范围

- 锁文件记录 Flutter 版本、渠道、Dart 版本、平台工件和校验信息。
- 处理 Flutter SDK 的 `bin/cache` 初始化与复用，避免只复制命令入口而遗漏实际运行依赖。
- 阻止受管 SDK 通过 `flutter upgrade`、`flutter channel`、`flutter downgrade` 原地改变版本；版本变化必须通过 Pinset 重新解析和安装。

### 不包含

- Android SDK、Android NDK、JDK、Xcode、CocoaPods、模拟器或设备管理。
- Flutter/Dart 项目依赖和 pub 缓存管理。
- beta、dev、master 等非 stable 渠道的首期支持。

### 验收条件

- 三平台虚拟机可安装并运行匹配版本的 `flutter --version` 和 `dart --version`。
- 项目版本正确覆盖全局版本，Flutter 与 Dart 不会交叉引用不同安装目录。
- 首次 cache 初始化、已有 cache 复用和离线重用都有自动化场景。
- 尝试原地改变受管 SDK 时给出明确、可操作的提示。

## v0.5.0 — CPython Provider

实现状态：核心 Provider、官方元数据、锁文件、安装、项目 `.venv`、shim/`exec` 自动路由和三平台 Release 验收已完成并发布。

兼容性修复：`v0.6.0` 补齐 `pip`/`pip3` shim，并统一通过所选解释器的 `python -m pip` 执行，覆盖全局 Python 和项目 `.venv`。

### 目标

- 首期只管理 CPython 解释器，不扩展到 PyPy、GraalPy 等实现。
- 支持 `pinset list python --available`、`use`、`global` 和项目锁定。
- 项目选择 Python 时创建项目根目录 `.venv`，路由 `python`、`python3`、`pip`、`pip3`，无需手动激活虚拟环境。
- 支持 `pinset exec -- <项目环境命令>` 运行 `.venv` 中的脚本，例如 `pip`、`pytest`。

### 技术范围

- 以 Astral `python-build-standalone` 作为跨平台预构建分发来源。
- 锁文件除 Python 版本外，还记录构建 ID、发行变体、来源和校验值，避免同一 CPython 版本对应不同构建却无法复现。
- 全局解释器安装与项目 `.venv` 分离；项目环境由锁定解释器通过标准库 `venv` 创建。
- `.venv` 写入 Pinset 所有权标记；缺少标记、版本不匹配、符号链接或损坏环境均拒绝接管。
- `pinset venv create/status/recreate` 提供显式生命周期；只有 `recreate` 可以在验证所有权后删除并重建环境。
- Pinset 只读取自己的 `pinset.toml` 和 `pinset.lock`，不识别或导入其他运行时管理器的声明。

### 不包含

- Python 包依赖解析、项目依赖锁定、包安装策略或全局包管理。
- 非 Pinset 创建的虚拟环境接管，以及其他运行时管理器的配置迁移。
- 从源码编译 CPython 或管理系统级 C/C++ 构建依赖。

### 验收条件

- 三平台虚拟机可安装并运行锁定版本的解释器。
- 全局版本、项目版本、离线缓存和损坏工件场景均被覆盖。
- 锁文件能精确区分 Python 版本相同但构建不同的工件。
- 全局及项目 `python`/`python3`/`pip`/`pip3` 均指向同一个选中解释器；项目命令自动指向 `.venv`，进程获得 `VIRTUAL_ENV` 且不继承 `PYTHONHOME`。
- 未标记 `.venv` 不会被采用或删除，普通 `venv create` 不重建现有环境，`venv recreate` 才执行受保护的重建。

## v0.6.0 — Java Provider

实现状态：核心 Provider、官方元数据、锁文件、安装、命令路由、Python pip 兼容性修复和三平台 Release 验收已完成并发布。

### 目标

- 首期只支持 Eclipse Temurin JDK、HotSpot、GA 正式版本。
- 支持 `pinset list java --available`、`use`、`global` 和项目锁定。
- 路由 `java`、`javac`、`jar`、`javadoc`、`javap`、`keytool` 和可用时的 `jshell`，并设置正确的 `JAVA_HOME`。
- 支持 `latest`/`current`、`lts`、Feature 版本、主次前缀、Update 版本和包含 `+build` 的精确版本。

### 技术范围

- 通过 Adoptium API v3 获取 Temurin GA 发布、LTS Feature、四个平台归档和最终 GitHub Release URL。
- 使用 Java 专用版本模型排序 `x.y.z+build`，构建号参与精确身份和新旧比较，不套用忽略构建元数据的普通 SemVer 规则。
- 继续使用锁文件 schema 2 的 Provider metadata，记录 distribution、vendor、image type、JVM implementation、heap size、release type、Feature、release name、OpenJDK version 和逐平台签名链接，无需迁移旧锁文件。
- 安装阶段强制校验 Adoptium API 提供的 SHA-256；记录 GPG 签名链接，但在完成密钥固定、轮换和吊销策略前不把系统 `gpg` 作为运行依赖。
- Windows 使用 ZIP，Linux/macOS 使用 `tar.gz`；macOS 的 `JAVA_HOME` 指向归档内 `Contents/Home`，其他平台指向安装根目录。
- 只为 Pinset 启动的受管子进程设置 `JAVA_HOME` 和组合 PATH，不修改 shell profile 或系统环境变量；保留 `CLASSPATH`、`JAVA_TOOL_OPTIONS`、`JDK_JAVA_OPTIONS`、`_JAVA_OPTIONS` 并由 `doctor` 提示其影响。

### 不包含

- 多厂商 JDK、JRE、OpenJ9、早期访问版、nightly、JavaFX、GraalVM 和自定义 JVM 构建。
- Maven、Gradle、Ant 或 Java 项目依赖管理。
- 自动修改项目的 Gradle/Maven toolchain 配置。
- 接管或删除系统 JDK、修改 `cacerts`，以及导入 SDKMAN、jEnv、asdf 等外部管理器状态。

### 验收条件

- 三平台虚拟机可安装并运行 `java -version` 与 `javac -version`。
- 三平台可编译并运行一个最小 Java 程序，`java.home`、`JAVA_HOME`、PATH 和锁文件都指向同一安装。
- LTS、最新 GA、Feature、Update 和精确 build 选择器行为稳定，锁文件保存最终工件 URL、SHA-256 和签名链接。
- 项目选择优先于全局选择；已选 JDK 命令缺失时明确失败，不静默回退到系统 Java。
- 旧 schema 2 项目继续可用，Java 分发、JDK/JRE、HotSpot/OpenJ9 和 GA/EA 身份没有歧义。

## v0.7.0 — 原生 Rust Provider

实现状态：核心 Provider、元数据解析、锁文件、安装、命令路由和三平台验收脚本已完成并发布。

### 目标

- 保持 Pinset 独立边界，由 Pinset 自己完成项目声明、版本锁定、下载、校验、安装、路由和诊断。
- 支持 `pinset list rust --available`、精确版本、主版本、主次版本和 `stable`/`latest`/`current` 选择器；配置接受浮动选择器，锁文件只保存精确 stable 版本。
- 路由 `rustc`、`cargo`、`rustdoc`、`rustfmt`、`cargo-fmt`、`clippy-driver` 和 `cargo-clippy`。

### 技术范围

- 版本发现只读取 Rust 官方 `https://static.rust-lang.org/manifests.txt` 中的 stable 精确版本，不纳入 beta 或 nightly。
- 每个精确版本读取并 SHA-256 校验官方 v2 `channel-rust-x.y.z.toml`，从清单锁定四平台 `rust` 组合归档的规范 URL 与 SHA-256。
- 安装官方 `default` profile 对应组件：`rustc`、`cargo`、`rust-std`、`rust-docs`、`rustfmt` 和 `clippy`；四个发布目标为 Windows x64、Linux x64、macOS x64 和 macOS arm64。
- 锁文件 schema 保持为 2，Provider metadata 记录 stable channel、manifest date、manifest SHA-256、profile 和组件边界。
- 受管进程只组合所选工具链的 PATH；保留用户的 `CARGO_HOME`、`RUSTUP_HOME` 与 `RUSTFLAGS`，不修改 shell profile 或外部管理器状态。

### 不包含

- beta、nightly、自定义 channel、额外 target 和按组件增删能力。
- 导入、复用或修改其他 Rust 工具链管理器的配置与安装目录。
- Cargo 包依赖、crate 发布或项目构建管理。
- C/C++ 编译器、原生链接器、系统 SDK 和交叉编译环境。

### 验收条件

- `list rust --available` 能从官方清单列出 stable 版本；选中的版本若缺少任一支持目标则明确失败。
- 三平台虚拟机中可由 Pinset 独立安装、锁定并路由项目工具链，且能用 `rustc` 编译并运行最小程序。
- `rustc`、`cargo`、`rustfmt` 的全局选择、项目覆盖、离开项目后的全局恢复和已安装工具链复用均通过真实验收。
- manifest 与归档 SHA-256、官方 URL、target、profile 或组件身份不一致时锁文件验证失败。
- Rust 接入不会改变其他 Native Provider 的下载和安装语义。

## v0.8.0 — Microsoft .NET SDK Provider

实现状态：核心 Provider、官方元数据解析、锁文件、安装、命令路由和手动虚拟机验收脚本已完成并发布；真实 SDK 验收不进入 GitHub Actions。

### 目标

- 首期只管理 Microsoft 官方 .NET SDK，不拆分管理 .NET Runtime、ASP.NET Core Runtime、Desktop Runtime 或 workloads。
- 支持 `pinset list dotnet --available`、`pinset global dotnet@<selector>`、`pinset use dotnet@<selector>`、锁定安装、卸载、`which`、`current` 和 `exec`。
- 支持 `latest`/`current`、`lts`、主版本、`major.minor` 通道和精确 SDK 版本；锁文件只保存精确 `x.y.zzz` SDK 版本。
- 路由 `dotnet`，并只为 Pinset 受管子进程设置与所选 SDK 一致的 `DOTNET_ROOT`。

### 技术范围

- 版本发现来自 Microsoft 官方 `releases-index.json` 与各通道 `releases.json`，只接受 `active`/`maintenance` 支持阶段的 GA `lts`/`sts` 通道，排除 preview、RC、go-live 和 EOL。
- 四个 Provider 目标为 Windows x64、Linux x64、macOS x64 和 macOS arm64；Windows 使用 ZIP，其余平台使用 `tar.gz`。
- 锁文件记录 SDK 版本、通道、release type、support phase、runtime release version、发布日期、官方归档 URL 和逐平台 SHA-512。
- 安装后校验归档根目录的 `dotnet`/`dotnet.exe`，复用现有安全解压、内容寻址缓存、并发锁和原子提交。
- 保留用户的 `DOTNET_CLI_HOME`、`NUGET_PACKAGES`、NuGet 配置和遥测选项，不修改 shell profile、系统 SDK、项目文件或 workload 状态。

### 不包含

- preview/RC/nightly、EOL 通道、Linux arm64、Alpine/musl 和其他首期未列出的 RID。
- .NET Runtime-only、ASP.NET Core Runtime、Windows Desktop Runtime、workloads、templates 或 Visual Studio 管理。
- NuGet 包解析、restore 策略、`global.json` 自动修改，以及导入其他 .NET 管理器的安装目录。

### 验收条件

- 官方 available 列表只出现仍受支持且四个平台归档完整的 GA SDK；浮动选择器稳定解析为精确 SDK。
- 锁文件拒绝非 Microsoft 官方 URL、错误 RID/格式、缺失平台、EOL/preview metadata 和不匹配的 SHA-512 身份。
- Windows、Linux、macOS 隔离虚拟机可安装并执行 `dotnet --version`，且 `DOTNET_ROOT`、PATH、锁文件和实际 host 来自同一安装。
- 可用所选 SDK 编译并运行最小 Console 项目，验证全局选择、项目选择、离开项目后的全局恢复和已安装 SDK 复用。
- GitHub Actions 仅编译、测试 Pinset 自身并打包，不下载或执行真实 .NET SDK；真实验收由发布后的虚拟机脚本完成。

## v0.9.0 — 运行时生命周期管理

实现状态：开发中。v0.8.0 已有单 Provider 的已安装列表、精确卸载和基础缓存清理；本版本在不改变既有命令语义的前提下补齐跨 Provider 生命周期。

### 目标

- `pinset list` 一次列出所有 Provider 的已安装版本；`pinset list <tool>` 和 `--available` 保持兼容。
- `pinset outdated` 分别检查当前项目和全局选择，并可限制 Provider、作用域或输出 JSON。
- `pinset uninstall` 增加 `--dry-run` 与 `--json`，实际删除前继续验证精确版本、引用与 Pinset 安装收据。
- `pinset prune` 清理未被全局、当前项目或 `--project` 显式附加项目引用的已安装版本。
- `pinset cache info|verify|repair|clean` 覆盖空间统计、内容寻址校验、受损归档移除与安全预览。
- `which`、`current`、`list`、`outdated`、`uninstall`、`prune` 和缓存查询提供稳定 JSON 输出。
- 提供 Bash、Zsh、Fish 和 PowerShell 的顶层命令、Provider、嵌套子命令和常用参数补全输出。

### 安全边界

- `prune` 不扫描整台机器；默认只保护全局与当前项目，其他项目必须显式传入 `--project <目录>`。
- `--project` 必须指向存在且可找到 `pinset.toml` 的项目目录，路径拼写错误时停止清理。
- 清理只处理带匹配 `.pinset-install.toml` 收据的版本目录；未知、不完整、符号链接或外部目录不接管、不删除。
- `cache verify` 重新计算完整归档摘要；`cache repair` 仅删除与内容寻址身份不符的完整归档，保留有效归档和可续传部分文件。
- `--dry-run` 不写配置、不删除安装或缓存，JSON 中显式记录 `dry_run`。
- 精确版本卸载只依赖本地版本语法、支持窗口和安装收据，不请求 Provider 元数据。
- `outdated` 只读取 Provider 官方元数据，不安装或下载运行时归档。

### 不包含

- 新语言或新 Provider、系统运行时卸载、依赖包管理以及外部运行时管理器导入。
- 自动扫描用户磁盘寻找所有 `pinset.toml`，或在后台维护项目索引。
- 1.0 之前的配置/锁文件迁移框架和兼容承诺；1.0 发布时再冻结公开协议。
- 在 GitHub Actions 下载 Flutter、JDK、Rust、.NET SDK 等真实大型运行时。

### 验收条件

- 所有 Provider 的收据和命令夹具均通过同一套 `list/current/which` 生命周期契约，JSON 字段稳定且不受界面语言影响。
- 当前项目、全局和额外项目引用会阻止 `prune` 删除相应版本，未引用版本可预览后安全删除。
- 受损缓存被 `verify` 以非零状态报告，`repair --dry-run` 不改文件，实际修复只删除受损归档。
- `current rust --json` 等工具名与命令名不同的 Provider 查询可用。
- 普通 CI 仅做三平台构建；Release Quality 使用小型收据、命令和缓存夹具覆盖生命周期命令，真实运行时及其轻量生命周期查询继续由发布后隔离虚拟机验收。

## Provider 架构约束

从 `v0.3.0` 开始，新 Provider 必须复用同一套核心生命周期：

1. 解析 Provider 和版本选择器。
2. 获取、缓存并规范化 available 版本元数据。
3. 解析平台目标与具体工件。
4. 将最终版本和工件身份写入锁文件。
5. 下载、校验、安全解压和原子安装。
6. 构建仅作用于受管命令的环境变量和命令路径。
7. 验证安装结果，并通过 `doctor` 输出可操作诊断。

Provider 特有逻辑应留在 Provider 内，核心层只处理公共生命周期。配置、锁文件、安装目录、下载缓存、并发锁、代理/镜像、命令路由和错误模型不得为每种语言复制一套实现。

## 验证边界

Pinset 的运行时安装会写入用户目录、下载缓存并改变命令解析结果，因此本机运行验收必须使用独立的临时 `PINSET_HOME`、项目目录和 shim 目录。

- 本项目开发不在维护者本机或 WSL 执行 Pinset、构建、测试或运行时下载，避免污染真实工具链和用户状态。
- 普通 push/Pull Request CI 只在 GitHub Actions 隔离虚拟机执行三平台 release 构建和打包；真实运行时验收不进入 Actions。
- 格式、Clippy、单元测试和小型脚本测试作为 Release 工作流的质量门禁执行。
- Provider 变更在发布前必须通过 Ubuntu、Windows、macOS 三平台构建和 Release Quality 检查。
- 所有真实运行时均不在 GitHub Actions 下载；发布后使用保留的完整验收脚本在隔离虚拟机验证，发现问题后通过补丁版本修复。
- 单平台本地通过不等于三平台通过，最终结论以 GitHub Actions 记录和各目标平台验收为准。

## 发布流程

1. 在功能分支完成代码、文档、版本号和变更记录。
2. 经用户明确授权后暂存、提交、推送并创建 Pull Request。
3. 等待 Pull Request 的 Linux、Windows、macOS 三平台构建通过；真实运行时验收统一记录为发布后虚拟机验收边界。
4. 经用户明确确认后合并到 `main`。
5. 创建签名版本标签并触发包含 Quality 门禁的 Release 工作流。
6. 确认 release assets、checksums、SBOM/provenance 和各平台安装脚本可用后宣布发布完成。

## 跨版本加固项

- 继续加强 Node.js 上游签名验证和供应链证据。
- 1.0 发布时冻结配置、锁文件和 CLI 协议；1.0 之前不开发迁移框架，也不承诺跨预发布版本迁移。
- 完善代理、镜像、断点续传、离线缓存和受损缓存恢复体验。
- 增加 Provider 契约测试，确保 available、resolve、install、activate、doctor 的行为一致。
- 将平台扩展与新 Provider 分开规划，避免在同一个小版本中同时扩大运行时和目标平台范围。
- 根据真实用户反馈决定后续是否支持更多 Java 发行版、Flutter 渠道和 Python 实现。

## 主要风险

| 风险 | 应对 |
| --- | --- |
| Provider 数量增加导致核心逻辑出现大量分支 | 在 `v0.3.0` 先完成 Native Provider 生命周期和元数据模型收敛 |
| Go 自动工具链下载绕过项目锁定 | 受管子进程默认使用 `GOTOOLCHAIN=local`，同时尊重并诊断用户显式配置 |
| Flutter SDK 会原地更新并维护可变 cache | 将 SDK 和 cache 生命周期显式建模，阻止受管安装原地切换版本 |
| Python 第三方预构建工件无法只靠语言版本复现 | 锁定 build ID、variant、来源和校验值 |
| Java 厂商、JDK/JRE 和 JVM 类型产生歧义 | 首期仅 Temurin JDK/HotSpot/GA，并在 schema 2 Provider metadata 中显式记录分发属性 |
| Rust 组件与 target 范围过大 | 开发前单独冻结原生分发来源和最小组件边界，不接管其他管理器状态 |
| 本地运行验证污染用户开发环境 | 使用临时 `PINSET_HOME`、临时项目和临时 shim 目录隔离验收 |
| 超大 SDK 持续占用托管 Runner | Actions 不下载 Flutter 等超大工件；保留完整脚本供发布后虚拟机验收 |
