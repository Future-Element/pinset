# Pinset PRD

文档状态：`产品基线`
当前发布版本：`v0.1.0-alpha.4`
更新时间：2026-08-12

## 文档关系

- 本文件是产品、功能契约、技术架构和用户操作的统一基线，定义 Pinset 是什么、服务谁、
  如何工作以及明确不做什么。
- [Plans](PLANS.md) 定义各版本范围、实施顺序和发布门禁。
- [发布说明](RELEASE_NOTES.md) 只记录已经交付的用户可见变化。
- 本文件中的“当前”均指 `v0.1.0-alpha.4`；标记为“计划”的契约尚不能当作可用命令。

## 1. 产品定位

Pinset 是一个本地优先、跨平台、可验证的多语言运行时版本管理 CLI。

它用统一的项目配置、锁文件、安装来源和命令解析规则管理 Node.js、CPython 与 Flutter，
减少开发者同时维护 fnm/nvm、uv/pyenv、FVM 等工具时产生的版本、PATH、镜像和平台差异。

一句话表达：

> 一个配置锁定项目运行时，一个命令解释当前到底用了哪个版本。

Pinset 的差异化不是支持最多语言或插件，而是：

- 项目配置只包含数据，不执行项目钩子或任意脚本；
- 锁文件从首版开始记录精确产物身份和校验信息；
- 项目、全局、旧管理器和系统 PATH 的选择结果唯一且可解释；
- 镜像只改变传输位置，不能替换可信哈希或 canonical 产物身份；
- Windows、macOS、Linux 使用同一产品语义，不把 Windows 作为后补平台；
- 日常使用无需账户、云服务、管理员权限或隐藏遥测。

## 2. 目标用户

核心用户：

- 同时维护多个 Node.js 项目，并需要在版本之间可靠切换的开发者。
- 同时使用 Node.js、Python 和 Flutter，厌倦每种语言维护一套版本管理器的个人开发者。
- 在 Windows、macOS、Linux 或 WSL 间切换，需要团队项目配置一致的开发者。
- 需要国内镜像、企业代理、离线缓存或可审计下载来源的用户。
- 已安装 nvm、fnm、uv、FVM、mise、asdf 或 vfox，希望渐进迁移而不是破坏原环境的用户。

后续用户：

- 需要通过同一锁文件在本地和 CI 复现运行时的小型团队。
- 对供应链来源、许可证、哈希和构建证明有审计要求的团队。

首版不以大型企业集中策略下发、容器编排或无限语言插件生态为主要用户场景。

## 3. 用户问题

- 每种运行时管理器都有不同命令、配置文件、Shell 初始化和安装目录。
- 同一项目在不同开发者或不同操作系统上可能实际使用不同运行时版本。
- PATH 被多个管理器修改后，用户难以知道 `node`、`python` 或 `flutter` 来自哪里。
- 本地终端、CI 与 IDE 可能使用不同 SDK，问题只在部分环境出现。
- 国内或企业网络下载缓慢，但切换镜像常常同时改变了信任来源。
- 旧项目缺少可提交的精确产物锁，未来无法确认当时使用了什么归档和校验值。
- 迁移新工具时，用户担心旧管理器、系统运行时和项目配置被自动删除或改写。
- 下载中断、损坏归档或路径穿越可能留下半安装或污染后续选择。

## 4. 产品目标

1. 用统一命令管理 Node.js、CPython 和 Flutter 的安装与版本选择。
2. 让项目通过可提交的配置和锁文件获得跨平台可复现运行时。
3. 让用户设置本机全局默认版本，并由最近项目配置进行明确覆盖。
4. 让 `current`、`which` 和 `doctor` 解释请求版本、精确版本、来源和真实可执行文件。
5. 让下载、校验、解压和安装具备失败关闭、事务提交和安全重试能力。
6. 让官方源、国内镜像和企业代理共享同一可信产物身份与校验值。
7. 与现有管理器渐进共存，不静默删除、覆盖或执行不可逆迁移。
8. 在 Windows、macOS、Linux 和 WSL 上保持一致的配置与命令语义。

## 5. 非目标

Pinset 不做：

- 管理 npm/pnpm/yarn、pip/uv、pub 等包依赖。
- 自动创建或激活 Python 虚拟环境。
- 充当任务运行器、环境变量管理器、Secret Vault 或容器管理器。
- 读取项目配置时执行脚本、钩子、插件或远程代码。
- 自动修改 shell profile、IDE 配置或系统级 PATH。
- 自动卸载 nvm、fnm、pyenv、uv、FVM、mise、asdf、vfox 或系统运行时。
- 默认选择未由用户批准的第三方镜像。
- 依赖账号、云同步、订阅或遥测才能使用核心功能。
- 首版覆盖所有语言、CPU 架构、历史版本和操作系统组合。
- 维护第三方 Homebrew Tap 或 Scoop Bucket。

## 6. 产品结构

### CLI

负责初始化、版本选择、锁定安装、来源配置、查询、诊断和生命周期管理。CLI 的写操作必须
明确，查询和诊断默认只读。

### Project Contract

项目根目录使用：

```text
pinset.toml
pinset.lock
```

`pinset.toml` 表达用户希望使用的工具与版本；`pinset.lock` 保存解析后的精确版本、跨平台
产物、canonical 身份、哈希和验证信息。二者应提交版本控制。

### Global State

用户级默认选择保存在 `$PINSET_HOME/state`，只影响当前用户和当前操作系统，不写入项目，
也不依赖 `$HOME/pinset.toml` 充当伪全局配置。

### Provider

Node.js、CPython 和 Flutter 使用内置 provider。Provider 负责版本元数据、目标映射、产物
布局、可信校验和安装后的必要文件验证。首版不开放任意脚本插件。

### Installer

Installer 负责下载限制、哈希校验、安全解压、临时目录、原子提交、收据和幂等复用。
只有完整且经过验证的安装才能进入可解析目录。

### Shim and Exec

shim 让现有的 `node`、`npm`、`python`、`flutter` 等命令根据当前目录选择运行时；
`pinset exec` 为 CI、脚本和不安装 shim 的用户提供显式入口。二者共享同一个核心解析器。

### Source Configuration

安装源属于机器本地配置。内置 `official` 不可覆盖或删除；用户可显式增加 HTTPS 镜像、
受信任局域网 HTTP 服务和有序回退。源只改变传输位置，不改变锁文件中的可信身份。

### Doctor

`doctor` 解释配置优先级、安装完整性、PATH、shim、旧管理器和 IDE 常见冲突，只给出
精确、可逆的建议，不自动修复或卸载。

## 7. 功能需求

### 配置与锁文件

- 项目配置采用不可执行 TOML，未知字段和未知 schema 必须明确失败。
- 版本选择最终写入精确版本，锁文件序列化结果确定。
- 锁文件包含所有认证目标需要的产物，而安装只处理当前平台。
- `install --locked` 在配置和锁不一致时失败，不得偷偷更新。
- 机器相关绝对路径和活动镜像不得写入项目锁文件。
- schema 变更必须有兼容或迁移说明，不能静默重写用户项目。

### 解析与执行

- 项目版本从当前目录向上查找最近的 `pinset.toml`。
- 正式解析顺序为：显式一次性选择、项目、兼容文件、全局、系统 PATH。
- 当前层明确声明但不可用时失败关闭，不得静默降级到低优先级版本。
- 同一次命令的主程序和附带命令必须使用同一运行时目录。
- shim 不能递归调用自己，不能在执行阶段访问网络。
- `exec` 和 shim 必须保留工作目录、参数和子进程退出码。

### 安装与完整性

- 下载前确定 canonical 产物身份和可信校验信息。
- 自定义镜像不能提供或替换项目锁中的可信哈希。
- SHA-256 不匹配、归档截断、路径穿越、非法链接和展开超限必须失败。
- 只有网络类失败可以尝试用户批准的下一个源；校验失败立即停止。
- 解压和验证在 Pinset 数据根中的临时事务目录完成。
- 最终提交原子化；中断后不能出现可被选择的半安装。
- 安装收据不得保存 URL 凭据、查询参数、fragment 或其他 Secret。

### Node.js

- 当前已支持精确稳定版本 `x.y.z`。
- 路由 `node`、`npm`、`npx` 和 `corepack`。
- 支持 Windows x64、Linux x64、macOS x64 和 macOS arm64 官方预编译产物。
- npm/corepack 通过 `/usr/bin/env node` 启动时，子进程 PATH 必须包含所选 Node。
- 后续支持 major/minor、LTS 和 current 选择器，但写锁结果始终是精确版本。

### CPython

- 计划使用版本化、可审计的 python-build-standalone 清单。
- 锁文件记录来源、许可证、哈希和目标。
- 路由 Python 解释器及归档实际包含的 pip 命令。
- 与 uv/pip 共存，Pinset 不创建虚拟环境、不管理项目依赖。

### Flutter

- 计划支持 Flutter stable 与同 SDK 内 Dart。
- 提供稳定 SDK 根目录查询供 IDE 和脚本使用。
- 镜像兼容不改变可信 release 元数据与校验边界。
- Pinset 不自动改写 VS Code、Android Studio 或 Xcode 配置。

### 查询、诊断与生命周期

- `current` 展示请求版本、精确版本、选择来源、配置路径和安装路径。
- `which` 展示最终将执行的真实文件；SDK 型工具可查询 SDK 根目录。
- `list` 明确区分 installed、selected、cached 和 system。
- `uninstall` 只删除 Pinset 登记的精确安装；被配置引用时默认拒绝。
- `doctor --json` 提供稳定机器可读结果，但普通诊断不收集或上传遥测。
- 缓存清理和卸载使用不同显式命令，避免删除语义混淆。

## 8. 核心流程

### 首次安装 Pinset

1. 用户从 GitHub Release 或官方安装脚本取得 Pinset。
2. 安装器识别平台并下载对应归档与 `SHA256SUMS`。
3. 校验通过后把 `pinset` 与 `pinset-shim` 安装到用户目录。
4. 安装器展示 PATH 建议，但不自动修改 shell profile。
5. 用户执行 `pinset --version` 和 `pinset doctor` 验证环境。

### 设置全局默认版本

1. 用户执行 `pinset global node@24`；兼容入口为 `pinset use node@24 --global`。
2. Pinset 读取可信元数据并生成用户级全局锁。
3. 当前平台缺失时执行校验和事务安装。
4. shim 在没有项目声明时使用该全局版本。
5. `current` 展示来源为 `global`。

### 初始化项目

1. 用户在项目根目录执行 `pinset init`。
2. Pinset 创建最小 `pinset.toml`，已有文件时拒绝覆盖。
3. 用户执行 `pinset use node@<exact-version>`。
4. Pinset 生成跨平台锁并安装当前平台产物。
5. 用户提交 `pinset.toml` 与 `pinset.lock`。
6. 项目及其子目录自动覆盖全局版本。

### 克隆项目与 CI

1. 用户或 CI 克隆包含配置和锁文件的项目。
2. 配置受信任镜像或使用 official 源。
3. 执行 `pinset install --locked`。
4. Pinset 在写入前校验配置、锁、目标和哈希。
5. 使用 shim 或 `pinset exec -- <command>` 执行构建和测试。

### 国内或企业网络

1. 用户显式添加与官方目录结构兼容的源别名。
2. 用户选择活动源并配置需要的网络回退顺序。
3. Pinset 仍从可信元数据或已有锁取得 canonical 身份与哈希。
4. 网络失败可以切换下一个已批准源；哈希失败立即停止。
5. 项目成员可以使用不同源，但共享同一项目配置和锁。

### 与旧管理器共存

1. `doctor` 枚举 PATH 和已知配置文件。
2. 展示每个候选命令的来源和实际优先级。
3. 多个配置冲突时停止并解释。
4. 用户明确选择迁移后再生成候选 Pinset 配置。
5. Pinset 不卸载旧工具，也不删除旧配置。

## 9. 数据与安全原则

- Local-first：已安装运行时的日常选择和执行不依赖账号或网络。
- Data, not code：进入项目、读取配置或解析版本不执行项目代码。
- Fail closed：哈希异常、损坏归档、配置冲突和已声明版本缺失默认阻断。
- Trust separation：官方元数据和锁是信任来源，镜像只是传输来源。
- Atomic visibility：只有完整验证并原子提交的安装对解析器可见。
- Least privilege：不使用 sudo 完成用户级日常操作，不写系统目录。
- Ownership boundary：卸载和清理不能越过 Pinset 数据根或删除外部管理器文件。
- Secret minimization：配置、锁、收据、日志和诊断不保存凭据或敏感 URL 部分。
- Explicit change：PATH、shell profile、IDE 和旧管理器只给建议，不静默修改。
- No hidden telemetry：默认不收集或上传命令、路径、版本和使用行为。

## 10. 平台与发布原则

Pinset 的产品目标平台是 Windows、macOS 和 Linux；WSL 按独立 Linux 环境处理。

当前 `v0.1.0-alpha.4` 的产物矩阵：

| 能力 | Windows x64 | Linux x64 | macOS x64 | macOS arm64 |
| --- | --- | --- | --- | --- |
| Node 项目锁定与安装 | 支持 | 支持 | 支持 | 支持 |
| Pinset 官方 Release 产物 | ZIP | TAR.GZ | 暂未发布 | TAR.GZ |
| 官方 curl 安装器 | 不适用 | 支持 | 不适用 | 支持 |

平台支持声明必须区分“代码可构建”“CI 构建通过”“真实安装通过”和“完整用户流程验收”。
未执行的层级不得用更低层级结果代替。

## 11. 非功能需求

- shim 热路径不访问网络，解析开销需持续基准测试并公开认证结果。
- 锁文件对同一输入产生确定输出，避免无意义 diff。
- 大归档、慢网络和中断不会造成无限内存、磁盘或临时文件增长。
- 并发安装和并发配置写入具备互斥、幂等或明确冲突结果。
- 错误信息包含工具、版本、来源、目标和可操作建议，不只输出底层异常。
- Windows 路径大小写、长路径、文件占用和链接权限必须有明确行为。
- Unix 归档符号链接必须保持在归档根内，并指向已验证的普通文件。
- 日志、诊断和 Issue 模板不得默认包含完整用户路径、凭据或 URL Secret。
- 每个认证平台都有构建、自动化测试和相称的真实环境验收边界。

## 12. 成功指标

质量指标：

- 认证矩阵中每个 provider 至少一个固定版本完成真实下载、校验、安装和执行。
- 项目、全局和系统 PATH 解析矩阵无歧义、无静默降级。
- 错误哈希、恶意归档、中断和并发安装不会产生可选择的半安装。
- 已提交锁文件在支持平台和批准源上能够复现。
- Release 资产、SHA-256、版本号和 Git tag 保持一致。

使用指标：

- 新用户可在五分钟内安装 Pinset、初始化项目并执行一次所选运行时。
- 用户能通过 `current`、`which` 或 `doctor` 回答当前版本及其选择原因。
- 同一用户可以在全局版本和两个不同项目版本之间切换，不手动重写 PATH。
- 国内或企业网络用户可以切换传输源而不修改项目锁。
- 旧管理器用户可以先诊断再迁移，且不需要卸载原工具完成试用。

Pinset 默认无遥测，因此产品验证主要来自公开 Issue、用户主动反馈、可复现测试和明确同意
的访谈，不以隐藏采集换取指标。

## 13. 当前版本边界

`v0.1.0-alpha.4` 已经完成 Node 精确和浮动版本选择、显式全局默认、项目/全局/PATH 解析、
安全卸载、带进度显示的归档下载、内容寻址下载缓存、来源测试、JSON 诊断、中英文界面和
旧管理器只读迁移预览。
该版本已通过本地自动化，三平台构建与公开资产由 Release 标签工作流完成；用户目标系统
功能验收留待发布后执行；
CPython 与 Flutter 仍未交付。完整范围和发布门禁见
[Plans](PLANS.md#4-v010-alpha4--node-workflow-hardening)。

## 14. 命令契约与交付状态

| 命令 | 交付状态 | 目标契约 |
| --- | --- | --- |
| `pinset init` | 已实现 | 创建最小项目配置，已有文件时拒绝覆盖 |
| `pinset use node@x.y.z` | 已实现 | 更新项目配置与锁，并安装当前平台 |
| `pinset use node@24`、`node@24.12`、`node@lts`、`node@current` | 已实现 | 联网解析为精确稳定版本后写锁 |
| `pinset use node@x.y.z --no-install` | 已实现 | 只更新项目配置和锁 |
| `pinset use node@x.y.z --global` | 已实现 | 更新用户级全局选择，不修改项目 |
| `pinset global [node@selector]` | 已实现 | 显式查看或设置全局默认版本，并提示项目覆盖 |
| `pinset install --locked` | 已实现 | 配置与锁不匹配时失败，安装当前目标 |
| `pinset install --global --locked` | 已实现 | 根据全局锁恢复当前目标 |
| `pinset current` | 已实现 | 显示当前目录最终生效的项目、全局或系统选择 |
| `pinset current [tool]` | 已实现 | 显示工具、精确版本、来源和路径 |
| `pinset which <command>` | 已实现 | 显示将执行的真实文件 |
| `pinset which <command> --sdk` | Flutter 阶段 | 返回 SDK 根路径供 IDE/脚本使用 |
| `pinset exec -- <command>` | 已实现 | 使用当前项目运行时执行并返回子进程退出码 |
| `pinset exec node@<selector> -- ...` | 已实现 | 一次性选择已安装版本，不修改项目或全局状态 |
| `pinset doctor` | 已实现基础版 | 扩展 PATH、全局、旧管理器与 IDE 诊断 |
| `pinset doctor --json` | 已实现 | schema 1 稳定机器可读诊断结构 |
| `pinset source list/add/use/fallback/remove` | 已实现 | 管理本机传输源，不修改项目锁 |
| `pinset source test node [alias]` | 已实现 | 只读检测 HTTP/TLS、版本索引和 SHASUMS，不下载归档 |
| `pinset list node [--available]` | 已实现 | 本地安装列表默认离线；`--available` 显式读取官方索引 |
| `pinset uninstall node@x.y.z` | 已实现 | 默认保护当前项目和全局引用，只删除 Pinset 收据匹配目录 |
| `pinset cache list/clean` | 已实现 | 查看和清理 SHA-256 内容寻址归档，保留未知文件 |
| `pinset import --dry-run` | 已实现 | 只读检测 nvm/node-version/Volta/asdf/mise 并报告冲突 |
| `pinset --lang <en\|zh-CN>` | 已实现 | 无子命令时保存界面语言，带子命令时仅覆盖本次输出 |

计划命令只有在对应版本发布后才成为兼容承诺。脚本不得依赖未冻结的参数、输出文本或退出码。

建议的退出码分类为：

- `0`：成功；
- `2`：使用方式或配置错误；
- `3`：版本无法解析；
- `4`：网络或上游错误；
- `5`：校验或供应链错误；
- `6`：本地文件系统或权限错误；
- `7`：锁文件不一致；
- `8`：冲突、递归或其他管理器遮蔽。

当前只冻结已经实现命令的实际退出行为；完整分类需要在稳定版前由集成测试固定。

## 15. 配置、锁与本机状态

### 15.1 当前项目配置

`pinset.toml` 是不可执行数据，应提交版本控制。当前只接受精确 Node 版本：

```toml
schema = 1

[tools]
node = "24.0.0"
```

配置禁止 Shell 命令、安装后脚本、任务、动态表达式和远程 include。未知字段或未知 schema
默认拒绝，不能把未来字段误当成安全默认值。

### 15.2 当前项目锁

`pinset.lock` 应提交版本控制，包含：

- schema 和生成器版本；
- 工具、用户请求和精确版本；
- Windows x64、Linux x64、macOS x64、macOS arm64 产物；
- canonical URL、artifact path、SHA-256 和验证方式。

锁文件不记录当前机器活动镜像、下载缓存路径或安装绝对路径。`install --locked` 不修改锁，
也不访问“最新版本”重新解析。

概念示例：

```toml
schema = 1
generated_by = "pinset 0.1.0-alpha.4"

[[tool]]
name = "node"
requested = "24.0.0"
version = "24.0.0"
provider = "nodejs-official"

[[tool.artifact]]
target = "linux-x86_64"
canonical_url = "https://nodejs.org/dist/v24.0.0/node-v24.0.0-linux-x64.tar.xz"
artifact_path = "v24.0.0/node-v24.0.0-linux-x64.tar.xz"
sha256 = "..."
verification = "nodejs-shasums-https"
```

### 15.3 本机源配置

源配置位于 `PINSET_HOME`，项目不能声明任意下载 URL。概念结构：

```toml
schema = 1

[providers.node]
active = "company-mirror"
fallback = ["official"]

[providers.node.sources.company-mirror]
base_url = "https://mirror.example/node/"
```

`official` 是只读内置源。自定义源不能使用凭据 URL、query 或 fragment；HTTPS 是默认要求，
受信任局域网 HTTP 必须逐源显式批准。

### 15.4 数据目录

默认位置：

| 系统 | `PINSET_HOME` 默认值 |
| --- | --- |
| Windows | `%LOCALAPPDATA%\Pinset` |
| Linux/macOS | `$XDG_DATA_HOME/pinset`，未设置时为 `$HOME/.local/share/pinset` |

可用 `PINSET_HOME` 覆盖。Windows 与 WSL 是两个独立系统，不共享安装目录、PATH 或状态。

目标布局：

```text
PINSET_HOME/
├─ settings.toml                      # 本机界面语言等用户偏好
├─ downloads/                         # 已验证下载缓存，后续交付
├─ installs/<tool>/<version>/<target>/
├─ shims/
├─ state/                             # 全局选择、安装索引
├─ locks/                             # 跨进程锁，后续完善
└─ tmp/                               # 唯一事务目录
```

## 16. 技术架构

### 16.1 Workspace 与职责

```text
crates/
├─ pinset/          # 用户 CLI、参数和人类输出
├─ pinset-core/     # 配置、锁、解析、provider、安装器和诊断
└─ pinset-shim/     # 高频、无网络的轻量命令路由程序
```

安装器能力通过 Cargo feature 与 shim 隔离。单独构建 `pinset-shim` 时，正常依赖图不得包含
HTTP、TLS、归档、哈希和临时文件依赖。

### 16.2 命令解析

```text
调用 node/npm/python/flutter
  → Pinset shim 从 argv[0] 识别命令
  → 规范化 cwd 与命令名
  → 读取同一配置快照
  → 按显式/项目/兼容/全局/系统优先级解析
  → 定位已验证安装
  → 构造最小子进程环境
  → 执行真实二进制并返回退出码
```

解析热路径不访问网络。shim 使用深度标记和路径排除防止自递归；缓存只能优化性能，不能
改变正确性或使旧配置继续生效。

### 16.3 安装事务

```text
解析精确版本与目标
  → 获取可信元数据或读取已有锁
  → 在 PINSET_HOME/tmp 创建唯一事务目录
  → 按 active/fallback 下载并流式计算 SHA-256
  → 验证归档大小、类型与哈希
  → 安全解压并验证必要文件
  → 写入脱敏安装收据
  → 原子提交到 installs/<tool>/<version>/<target>
```

任何步骤失败都不能暴露最终安装。网络错误可以尝试下一个用户批准源；Content-Length/
大小限制、哈希、签名、provenance、写入、权限或解压错误必须硬停止。

归档至少拒绝：

- 绝对路径、`..`、设备文件和目标根逃逸；
- Windows 大小写碰撞、保留名和危险路径；
- 重复条目、非法特殊文件和越界链接；
- 超过文件数量、单文件大小、总展开大小或下载上限的内容。

Unix Node TAR.XZ 只允许归档根内的安全相对符号链接，且提交前目标必须是已验证普通文件。

### 16.4 Provider 契约

每个内置 provider 负责：

- 将自己的版本语义解析为精确版本；
- 根据目标构造 canonical artifact path；
- 提供可信元数据和校验方式；
- 描述归档布局、命令映射和必要文件；
- 验证安装后的运行时结构。

Node semver、Python 构建标签和 Flutter channel/ref 不能强制共用同一个字符串比较器。
首版只有内置 provider，不加载第三方代码；未来插件只有在能力隔离、签名和信任模型通过
安全评审后才考虑。

### 16.5 IDE 边界

终端 shim 不能自动解决 IDE SDK 路径。Flutter 阶段计划提供 `which --sdk` 和稳定项目 SDK
别名；Windows junction、Unix symlink 和 IDE 缓存必须先通过真实验证。Pinset 不自动提交
机器绝对路径，也不在未授权时修改 VS Code、Android Studio 或 Xcode 设置。

## 17. 当前版本使用指南

### 17.1 安装 Pinset

Linux x64 和 macOS Apple Silicon 的固定预发布安装命令、Release 资产和校验方式见
[v0.1.0-alpha.4 发布说明](RELEASE_NOTES.md#v010-alpha4)。

默认安装器把 `pinset` 与 `pinset-shim` 放到 `$HOME/.local/bin`，不使用 `sudo`，不修改
PATH，也不安装 Node。

### 17.2 项目选择与安装

设置或查看用户全局默认版本：

```bash
pinset global node@24
pinset global
```

带选择器时会解析并保存精确版本，默认安装当前平台；不带参数时只读查看全局默认。项目中的
`pinset.toml` 优先级更高，此时 `pinset global` 会提示覆盖关系，`pinset current` 显示真正
生效的项目版本。兼容命令 `pinset use node@24 --global` 仍然可用。

只写入全局配置和锁、不安装：

```bash
pinset global node@lts --no-install
pinset install --global --locked
```

全局操作不要求 `pinset init`，也不会创建或修改当前目录中的项目文件。

#### 项目版本

```bash
cd /path/to/project
pinset init
pinset use node@24.0.0
```

这会创建/更新 `pinset.toml` 与 `pinset.lock`，并为当前平台安装 Node。建议提交两个文件。

只锁定、不安装：

```bash
pinset use node@24.0.0 --no-install
```

根据已有锁安装：

```bash
pinset install --locked
```

### 17.3 执行与查询

不安装 shim 也可以完整使用 alpha.3：

```bash
pinset current
pinset which node
pinset exec -- node --version
pinset exec -- npm --version
pinset exec -- node ./scripts/build.mjs
```

从其他目录检查指定项目：

```bash
pinset current --cwd /path/to/project
pinset doctor --cwd /path/to/project
```

alpha.3 的解析顺序为最近项目配置、正式全局选择、排除 Pinset shim 后的系统 PATH。项目或
全局已经声明 Node 时，即使安装缺失也不会静默回退。

### 17.4 界面语言

保存简体中文为默认界面语言：

```shell
pinset --lang zh-CN
```

之后的正常提示、帮助和诊断默认使用中文。临时对单个命令使用英文但不修改持久设置：

```shell
pinset --lang en doctor
```

也可以通过 `PINSET_LANG=zh-CN` 覆盖当前进程。优先级为命令行参数、环境变量、
`$PINSET_HOME/settings.toml`、英文默认值。语言设置属于本机用户，不写入项目。

### 17.5 安装 shim

shim 只在用户指定目录创建 `node`、`npm`、`npx` 和 `corepack` 入口，不覆盖已有同名文件。

Windows PowerShell：

```powershell
$shimDir = pinset shim path
pinset shim install
$env:PATH = "$shimDir;$env:PATH"
node --version
pinset doctor
```

Linux/macOS：

```bash
PINSET_SHIM_DIR="$(pinset shim path)"
pinset shim install
export PATH="$PINSET_SHIM_DIR:$HOME/.local/bin:$PATH"
node --version
pinset doctor
```

默认从 `pinset` 同目录寻找 `pinset-shim`，默认目标为 `$PINSET_HOME/shims`；高级场景仍可用
`--binary` 和 `--dir` 覆盖。目标中任何同名文件已存在时都会拒绝整组安装，不会覆盖其他
管理器。确认无冲突后，用户可自行把同一 PATH 设置写入 shell profile；Pinset 不自动修改
已运行父 Shell。

### 17.6 国内或企业镜像

镜像必须保持 Node 官方发布目录的相对路径结构：

```bash
pinset source add node company-mirror --base-url https://mirror.example/node/
pinset source use node company-mirror
pinset source fallback node official
pinset source list node
```

恢复 official：

```bash
pinset source use node official
pinset source fallback node
```

删除未被 active/fallback 引用的源：

```bash
pinset source remove node company-mirror
```

Pinset 不内置或自动启用公共第三方镜像。首次 `use` 仍需要从 Node 官方 HTTPS 清单取得
哈希；如果项目已经提交锁文件，受限网络可以只通过批准镜像执行 `install --locked`。

### 17.7 CI

```bash
pinset install --locked
pinset exec -- node --version
pinset exec -- npm ci
pinset exec -- npm test
```

重复安装同一版本和目标时会验证收据及必要路径并直接复用。

真实 Node 验收使用 CI 中仅手动触发的 `Ubuntu real runtime acceptance`，运行位置是 GitHub 一次性
Ubuntu runner，所有状态写入临时 `PINSET_HOME`。普通 PR 和本地测试仍只使用假运行时，
避免无意下载或修改开发者环境。

### 17.8 Ubuntu/WSL 构建与测试

Windows 编译的 PE 文件不能作为 Linux 原生 Pinset 使用。Ubuntu/WSL 应下载 Linux Release
或在 Linux 文件系统中构建 ELF；源码建议放在 `$HOME` 下而不是 `/mnt/c`。

构建依赖：

```bash
sudo apt update
sudo apt install -y build-essential curl ca-certificates git pkg-config liblzma-dev
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
source "$HOME/.cargo/env"
rustup update stable
```

最低 Rust 版本为 1.85。构建和验证：

```bash
git clone git@github.com:Future-Element/pinset.git "$HOME/src/Pinset"
cd "$HOME/src/Pinset"
cargo build --release --locked -p pinset-cli -p pinset-shim
file target/release/pinset target/release/pinset-shim
ldd target/release/pinset
target/release/pinset --version
```

开发测试：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

自动化测试只使用临时目录、本地假 HTTP 服务、构造归档和假运行时，不安装真实语言运行时。

### 17.9 alpha.3 新增功能

以下命令已随 `0.1.0-alpha.3` 发布：

```bash
pinset list node
pinset list node --available
pinset use node@24 --no-install
pinset exec node@24.0.0 -- node --version
pinset uninstall node@24.0.0
pinset cache list
pinset cache clean
pinset source test node company-mirror
pinset doctor --json
pinset import --dry-run
```

`uninstall` 默认拒绝删除当前项目或全局仍引用的版本；`--force` 只跳过引用检查，不会删除
Pinset 数据目录之外、缺少收据或收据身份不匹配的目录。`cache clean` 只删除
`downloads/sha256/<SHA-256>.archive` 普通文件。`import --dry-run` 只读取当前目录中的旧管理器
配置并报告冲突，不会写入 `pinset.toml`。

### 17.10 alpha.4 新增功能

```bash
pinset global node@24
pinset global
pinset shim install
pinset shim path
```

`global` 不带参数时只读查看用户默认，带参数时解析精确版本、写入全局配置和锁，并默认安装
当前平台。下载 Node 归档时，交互终端显示进度条、百分比和字节数；完成提示只在 SHA-256
校验通过后出现。非交互环境输出简洁状态，内容寻址缓存命中不产生伪下载进度。

## 18. 诊断与常见问题

```bash
pinset doctor
```

alpha.3 会只读检查数据目录、最近项目配置、正式全局状态、锁匹配、当前目标安装、shim PATH
和其他 Node 候选。常见问题：

- `pinset.toml was not found`：进入项目目录或先执行 `pinset init`。
- `lockfile does not match`：执行 `pinset use node@完整版本 --no-install` 并审查锁文件。
- `runtime ... missing`：执行 `pinset install --locked`。
- shim 目标已存在：先确认来源；Pinset 不覆盖其他管理器或用户文件。
- 镜像返回 404：检查 base URL 是否指向 Node 发布目录根，或该版本是否已同步。
- 哈希不匹配：停止使用该源并调查；Pinset 没有关闭校验的选项。
- npm/corepack 报 `/usr/bin/env: node: No such file`：确认使用包含所选 runtime PATH 修复的
  `v0.1.0-alpha.1` 或更高版本，并通过 `pinset exec -- npm --version` 复验。
- Ubuntu 源码构建报 `linker cc not found`：安装 `build-essential`。
- WSL 运行 Windows 产物失败：改用 Linux x64 Release 或在 WSL 中构建 ELF。

诊断输出和提交 Issue 前应移除凭据、代理 Token、完整敏感 URL 和不必要的用户路径。

## 19. 安全与供应链威胁模型

必须持续覆盖：

- 哈希不匹配、签名身份混淆、镜像替换和元数据回滚；
- ZIP/TAR 路径穿越、绝对路径、链接逃逸、压缩炸弹和磁盘耗尽；
- 下载截断、并发安装、进程崩溃、锁损坏和断电持久性；
- 企业 TLS 代理、自定义 CA、离线缓存污染和敏感 URL 泄露；
- Windows 长路径、保留名、ADS、Unicode/大小写碰撞和文件占用；
- 卸载或缓存清理越过 Pinset 数据所有权边界；
- shim 自递归、PATH 遮蔽和项目配置诱导执行代码。

发布物目标包括 SHA-256、SBOM 和 artifact attestation。Attestation 只能关联产物、源码和
构建工作流，不代表代码没有漏洞。Node 稳定版前仍需完成授权发布密钥、签名 SHASUMS 和
离线密钥轮换策略。
