# Pinset Plans

当前规划版本：`v0.5.0`

当前发布候选：无

最新已发布版本：`v0.4.2`

更新时间：`2026-08-14`

## 路线原则

Pinset 将继续保持“一个工具统一选择、安装、锁定和路由开发运行时”的产品边界。后续版本每次只引入一个主要运行时，并在该版本中同步补齐可复用的 Provider 能力，避免多个生态同时开发导致配置、锁文件和命令路由失去一致性。

规划顺序为：

1. `v0.3.0`：Go，并完成 Native Provider 通用化。
2. `v0.4.0`：Flutter SDK，同时管理其内置 Dart。
3. `v0.5.0`：CPython。
4. `v0.6.0`：Java，首期仅支持 Eclipse Temurin JDK。
5. `v0.7.0`：Rust 原生 Provider，具体分发来源另行评估。

各版本暂不承诺日期。GitHub Actions 负责三平台构建、质量检查和体积适合自动化的真实运行时验收；Flutter SDK 等超大工件保留完整验收脚本，发布后由隔离虚拟机执行，避免持续占用托管 Runner。

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
| `v0.5.0` | 开发中 | CPython Provider、项目 `.venv` 与无激活路由 |
| `v0.6.0` | 规划中 | Eclipse Temurin JDK Provider |
| `v0.7.0` | 规划中 | 原生 Rust Provider |

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

实现状态：核心 Provider、官方元数据、锁文件、安装、项目 `.venv`、shim/`exec` 自动路由和 Windows 真实运行时验收已完成；三平台 CI 验收仍待工作流运行。

### 目标

- 首期只管理 CPython 解释器，不扩展到 PyPy、GraalPy 等实现。
- 支持 `pinset list python --available`、`use`、`global` 和项目锁定。
- 项目选择 Python 时创建项目根目录 `.venv`，路由 `python`、`python3`，无需手动激活虚拟环境。
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
- 项目 `python`/`python3` 与 `pinset exec -- pip` 均自动指向 `.venv`，进程获得 `VIRTUAL_ENV` 且不继承 `PYTHONHOME`。
- 未标记 `.venv` 不会被采用或删除，普通 `venv create` 不重建现有环境，`venv recreate` 才执行受保护的重建。

## v0.6.0 — Java Provider

### 目标

- 首期只支持 Eclipse Temurin JDK、HotSpot、GA 正式版本。
- 支持 `pinset list java --available`、`use`、`global` 和项目锁定。
- 路由 `java`、`javac`、`jar`、`javadoc`、`jshell`、`keytool` 等 JDK 命令，并设置正确的 `JAVA_HOME`。

### 技术范围

- 通过 Adoptium API 获取版本、操作系统、架构、包类型和工件信息。
- 校验 SHA-256，并评估将上游 GPG 签名纳入强校验链。
- 将锁文件升级到能够明确表达 distribution、package type、JVM implementation 和 release kind 的 schema；旧锁文件必须保持可读并提供清晰迁移路径。

### 不包含

- 多厂商 JDK、JRE、OpenJ9、早期访问版和自定义 JVM 构建。
- Maven、Gradle、Ant 或 Java 项目依赖管理。
- 自动修改项目的 Gradle/Maven toolchain 配置。

### 验收条件

- 三平台虚拟机可安装并运行 `java -version` 与 `javac -version`。
- `JAVA_HOME`、`PATH` 和锁文件都指向同一安装。
- LTS、最新 GA、精确版本和主版本选择器行为稳定。
- 旧 schema 项目继续可用，新 schema 的厂商和包类型没有歧义。

## v0.7.0 — 原生 Rust Provider

### 目标

- 保持 Pinset 独立边界，由 Pinset 自己完成项目声明、版本锁定、下载、校验、安装、路由和诊断。
- 具体官方分发、组件粒度与 target 支持在进入开发前单独立项，不预先承诺兼容外部管理器状态。

### 不包含

- 导入、复用或修改其他 Rust 工具链管理器的配置与安装目录。
- Cargo 包依赖、crate 发布或项目构建管理。

### 验收条件

- 三平台虚拟机中可由 Pinset 独立安装、锁定并路由项目工具链。
- Rust 接入不会改变其他 Native Provider 的下载和安装语义。

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

- 本地可执行格式化、Clippy、测试和隔离运行时验收，但不得写入真实 Pinset 状态或用户项目。
- 临时验收完成后清理明确验证过的测试目录；无法清理时必须报告残留位置。
- Pull Request 的 Quality 工作流在 GitHub Actions Ubuntu 虚拟机执行格式、Clippy、单元测试和脚本测试。
- Provider 变更在合并和发布前必须通过 Ubuntu、Windows、macOS 三平台构建和 Quality 检查；体积适合的运行时继续执行三平台真实验收。
- Flutter SDK 等超大工件不在 GitHub Actions 下载；发布后使用保留的完整验收脚本在隔离虚拟机验证，发现问题后通过补丁版本修复。
- 单平台本地通过不等于三平台通过，最终结论以 GitHub Actions 记录和各目标平台验收为准。

## 发布流程

1. 在功能分支完成代码、文档、版本号和变更记录。
2. 经用户明确授权后暂存、提交、推送并创建 Pull Request。
3. 等待 Pull Request Quality 工作流通过。
4. 对新增或修改的 Provider 触发三平台构建；体积适合的运行时执行真实验收，超大运行时记录发布后虚拟机验收边界。
5. 经用户明确确认后合并到 `main`。
6. 创建签名版本标签并触发 Release 工作流。
7. 确认 release assets、checksums、SBOM/provenance 和各平台安装脚本可用后宣布发布完成。

## 跨版本加固项

- 继续加强 Node.js 上游签名验证和供应链证据。
- 为配置与锁文件 schema 建立明确的兼容、迁移和拒绝策略。
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
| Java 厂商、JDK/JRE 和 JVM 类型产生歧义 | 首期仅 Temurin JDK/HotSpot/GA，并在新 schema 中显式记录分发属性 |
| Rust 组件与 target 范围过大 | 开发前单独冻结原生分发来源和最小组件边界，不接管其他管理器状态 |
| 本地运行验证污染用户开发环境 | 使用临时 `PINSET_HOME`、临时项目和临时 shim 目录隔离验收 |
| 超大 SDK 持续占用托管 Runner | Actions 不下载 Flutter 等超大工件；保留完整脚本供发布后虚拟机验收 |
