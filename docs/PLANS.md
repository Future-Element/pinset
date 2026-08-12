# Pinset Plans

当前发布版本：`v0.1.0-alpha.4`
下一开发目标：`v0.1.0-alpha.5`
目标稳定版本：`v0.1.0`
状态：`alpha.4 published — alpha.5 planning`
更新时间：2026-08-12

## 文档边界

- [PRD](PRD.md) 是产品定位、目标用户、产品目标和非目标的基线。
- 本文件是版本范围、实施顺序和发布门禁的权威来源。
- [发布说明](RELEASE_NOTES.md) 只记录已经发布的用户可见能力，不把计划写成已交付。
- 本文件同时保留研究结论、关键决策、Spike 证据、开发验证和发布流程；`Accepted` 决策
  如需改变，必须补充证据和迁移影响。

## 版本路线

| 版本 | 主题 | 核心结果 |
| --- | --- | --- |
| `v0.1.0-alpha.1` | Node project MVP | 精确 Node 版本、项目配置与锁文件、安全安装、镜像、shim 和三平台 Release |
| `v0.1.0-alpha.2` | Global and project resolution | 正式全局选择、项目覆盖、系统 PATH 透传和可解释解析 |
| `v0.1.0-alpha.3` | Node lifecycle and migration | 浮动选择器、本地版本管理、卸载、来源测试和旧管理器诊断 |
| `v0.1.0-alpha.4` | Node workflow hardening | 显式全局默认入口、项目覆盖解释和 Node 使用文档收口 |
| `v0.1.0-alpha.5` | CPython provider | 可审计 CPython 产物、Python 命令路由和 uv/pip 共存 |
| `v0.1.0-alpha.6` | Flutter provider | Flutter stable、Dart 路由、稳定 SDK 路径和 IDE 流程 |
| `v0.1.0-beta.1` | Integrated public preview | 三种运行时的跨平台闭环、离线复现和迁移验收 |
| `v0.1.0` | Stable preview | 冻结 schema、供应链与兼容策略，完成首个稳定版本 |

版本号不代表承诺日期。前一阶段的安全、兼容和真实环境门禁未通过时，不以增加功能代替
风险收敛。

## 1. v0.1.0-alpha.1 — Node Project MVP

状态：**已发布**

发布结果：

- [x] 支持精确稳定 Node.js 版本 `x.y.z`。
- [x] 生成可提交的 `pinset.toml` 与 `pinset.lock`。
- [x] 锁定 Windows x64、Linux x64、macOS x64 和 macOS arm64 官方产物。
- [x] 从 Node 官方 HTTPS `SHASUMS256.txt` 获取可信 SHA-256。
- [x] 支持官方源、自定义 HTTPS 源、受信任局域网 HTTP 源和有序网络回退。
- [x] ZIP/TAR.XZ 安全解压、归档链接校验、事务安装和重复安装复用。
- [x] 提供 `use`、`install`、`current`、`which`、`exec`、`doctor` 和多调用 shim。
- [x] 修复 Node 官方校验清单嵌套路径、TAR.XZ 链接以及 npm/corepack 子进程 PATH。
- [x] GitHub Actions 完成 Quality、Linux x64、Windows x64、macOS arm64 构建和自动 Release。
- [x] 提供带 SHA-256 验证的 `curl | sh` 安装器。
- [x] 仓库使用 MIT License 公开发布。

已知边界：

- 只支持项目选择，没有正式全局选择。
- 只接受精确稳定 Node 版本，不支持 `node@24`、`lts` 或 `current`。
- shim 在找不到项目配置时不会使用正式全局状态或安全透传系统 Node。
- Python、Flutter、缓存生命周期、卸载和旧管理器导入尚未交付。

## 2. v0.1.0-alpha.2 — Global and Project Resolution

主题：`Predictable Node selection everywhere`

状态：**已发布**

交付结果：

- [x] 独立、原子写入的全局选择与全局锁。
- [x] 项目、全局、排除 Pinset shim 后系统 PATH 的统一解析顺序。
- [x] 项目或全局已声明但不可用时失败关闭。
- [x] `current`、`which`、`exec`、`doctor` 与 shim 共用来源感知解析。
- [x] npm、npx、corepack 使用所选 Node 的子进程 PATH。
- [x] 英文与简体中文提示、帮助、参数错误和常见运行错误。
- [x] 临时目录中的 78 项测试、三平台构建和隔离 Ubuntu 真实 Node 验收。
- [x] 发布 `v0.1.0-alpha.2` 标签并复核公开 Release 资产、SHA-256、归档内容与版本输出。

目标是在不改变现有项目文件语义的前提下，为同一用户提供正式默认 Node 版本，让项目配置
稳定覆盖全局选择，并让 `node`、`npm`、`npx`、`corepack`、`exec` 与诊断命令得到同一个
可解释结果。

成功条件：

- 用户可用一条命令设置或更新全局 Node 版本。
- 项目配置只覆盖自己明确声明的工具，不修改全局状态。
- 项目、全局和系统 PATH 的优先级唯一、稳定且可由 `current`/`doctor` 解释。
- 已声明但损坏或未安装的版本必须失败关闭，不得静默换成另一版本。
- shim 热路径不访问网络，不执行项目代码，不修改父 Shell。
- CLI 可持久切换英文或简体中文，语言偏好不进入项目配置。

### 2.1 P0-01 Global Selection State

功能：

- `pinset use node@24.0.0 --global` 更新用户级选择并安装当前平台缺失产物。
- `--global --no-install` 只更新全局选择和锁，不下载运行时。
- 推荐增加 `pinset install --global --locked`，根据全局锁恢复当前平台安装。
- 全局状态位于 `$PINSET_HOME/state`，不写入 `$HOME/pinset.toml` 或任意项目目录。
- 全局选择保存精确版本；全局锁保存 canonical 产物、目标、哈希和验证方式。
- 配置和锁文件使用同目录临时文件、刷新和原子替换。

建议布局：

```text
$PINSET_HOME/
└─ state/
   ├─ global.toml
   └─ global.lock
```

验收：

- 更新全局选择不会修改当前目录或祖先目录中的 `pinset.toml`/`pinset.lock`。
- 写入中断不会留下可被解析的半套全局状态。
- 配置与锁不一致时，锁定安装在联网和写入安装目录前失败。
- 同版本重复选择和安装幂等成功。

### 2.2 P0-02 Resolution Precedence

Node 命令解析顺序：

1. 最近祖先目录中 `pinset.toml` 明确声明的 Node。
2. Pinset 全局选择。
3. 系统 PATH 中排除 Pinset shim 目录后的真实命令。

后续 `pinset exec node@<selector> -- ...` 进入 alpha.3；alpha.2 不因此扩张一次性选择器范围。

失败规则：

- 项目声明 Node 后，即使对应安装缺失或损坏，也不能回退到全局或系统 Node。
- 全局声明 Node 后，即使对应安装缺失或损坏，也不能回退到系统 Node。
- 只有当前层完全没有声明该工具时，才继续下一层。
- 项目存在 `pinset.toml` 但未声明 Node 时，可以继续解析全局 Node。
- 单次解析期间使用同一配置快照；命令执行阶段不重新联网解析。

验收：

- 项目根目录和任意嵌套目录选择同一个最近配置。
- 相邻项目不会互相污染版本。
- 项目版本覆盖全局版本，离开项目后恢复全局版本。
- 未声明工具的系统透传不会递归回到 Pinset shim。
- 错误包含被选择层级、配置路径、请求版本和实际搜索路径。

### 2.3 P0-03 CLI and Diagnostics

命令契约：

```shell
pinset use node@24.0.0 --global
pinset use node@24.0.0 --global --no-install
pinset install --global --locked
pinset current
pinset current node
pinset which node
pinset doctor
```

`current`、`which` 和 `doctor` 至少展示：

- 工具和精确版本；
- 选择来源：`project`、`global` 或 `system`；
- 配置或状态文件路径；
- 安装状态和真实可执行文件；
- 被更高优先级配置覆盖的候选；
- PATH 中可见的旧管理器或其他同名命令。

诊断命令默认只读，不自动改 PATH、不删除旧管理器、不修复项目文件。

### 2.4 P0-04 Shim and System PATH Safety

- `node`、`npm`、`npx` 和 `corepack` 共用同一个解析快照。
- 运行 npm/corepack 时把所选 Node 的命令目录放到子进程 PATH 最前面。
- 搜索系统命令时排除当前 shim 目录和当前 shim 文件，阻止直接或间接递归。
- 系统透传保留参数、工作目录、环境和退出码，不创建新的 Pinset 状态。
- 项目或全局已声明版本时，PATH 中的其他 Node 只能作为诊断候选，不能参与执行。

### 2.5 P1-01 Transitional Home Configuration Detection

alpha.1 文档曾允许把 `$HOME/pinset.toml` 作为临时默认版本。alpha.2 不自动删除、迁移或
改写该文件，因为它也可能是用户有意创建的普通项目配置。

`doctor` 应识别以下情况并给出可逆建议：

- `$HOME/pinset.toml` 被大量子目录继承；
- 同时存在正式全局选择与 `$HOME/pinset.toml`；
- HOME 配置覆盖全局选择。

建议必须先展示当前行为，再由用户手动决定是否迁移和删除文件。

### 2.6 P1-02 CLI Language

- `pinset --lang zh-CN` 把简体中文保存为当前系统用户的默认界面语言。
- `pinset --lang en` 恢复英文；带子命令时 `--lang` 只覆盖本次执行，不改持久设置。
- `PINSET_LANG` 可作为当前进程覆盖，优先级低于命令行参数、高于持久设置。
- 语言保存在 `$PINSET_HOME/settings.toml`，不修改项目配置、锁文件、源配置或 shell profile。
- 正常提示、诊断、帮助、参数错误和常见运行错误使用同一语言目录；路径、命令、版本、源
  别名和底层系统错误保持原始技术值。
- 首批只冻结 `en` 与 `zh-CN` 标识，后续语言必须通过同一目录扩展，不能在各命令散落判断。

### 2.7 Data and Compatibility

- 保持项目 `schema = 1` 和当前 `pinset.lock` 兼容，不因全局功能改写已提交文件。
- 全局配置使用独立 schema，未来扩展 Python/Flutter 时按工具增加字段。
- 全局状态属于当前用户与当前操作系统，不进入项目、备份或远程同步。
- Windows 与 WSL 使用独立 `PINSET_HOME`，不共享安装目录或锁定状态。
- 源配置仍属于本机；切换镜像不得改变项目锁或全局锁中的 canonical 身份和哈希。

### 2.8 Implementation Order

1. 冻结全局状态 schema、文件位置、原子写入和错误语义。
2. 抽象带来源信息的统一解析结果，不先修改 shim 行为。
3. 实现 `use --global`、全局锁和锁定安装。
4. 让 `current`、`which`、`exec` 使用统一解析器。
5. 实现安全的系统 PATH 搜索和 shim 透传。
6. 扩展 `doctor`，覆盖项目、HOME 过渡配置、全局和系统候选。
7. 补齐文档、升级说明、三平台 CI 与 Ubuntu VM 观察性验收。
8. 冻结 Release Notes，创建 `v0.1.0-alpha.2` 标签并验证公开安装。

建议拆分为三个可独立审查的 PR：

| PR | 内容 | 合并门禁 |
| --- | --- | --- |
| 1 | 全局状态、schema、锁和核心解析模型 | 临时目录单测、原子写入、项目兼容测试 |
| 2 | CLI、shim、系统 PATH 透传和诊断 | 解析矩阵、递归保护、npm/corepack 与退出码测试 |
| 3 | 文档、安装/升级流程和发布准备 | 全量 Quality、三平台构建、Ubuntu VM 验收 |

### 2.9 Acceptance Matrix

自动化至少覆盖：

- 只有全局版本。
- 项目版本覆盖全局版本。
- 项目存在但未声明 Node，回退全局版本。
- 项目声明的安装缺失，禁止回退。
- 全局声明的安装缺失，禁止回退。
- 没有项目和全局选择，安全透传系统 Node。
- PATH 中存在 Pinset shim、系统 Node 和其他管理器 Node 的组合。
- `node`、`npm`、`npx`、`corepack` 使用同一 Node 命令目录。
- `--global` 不修改项目文件，普通 `use` 不修改全局文件。
- `--no-install` 不发起运行时归档下载。
- 配置损坏、锁不匹配、并发写入和中断恢复。
- `--lang zh-CN` 持久化、单次英文覆盖、中文帮助、中文参数错误和中文诊断。

真实环境验收：

| 平台 | 自动化 | 观察性验收 |
| --- | --- | --- |
| Windows 11 x64 | 构建、单测、假运行时解析与 shim | PowerShell PATH、系统 Node 共存、项目切换 |
| Ubuntu x64 | 构建、单测、假运行时、curl 安装器 | 全局/项目切换、npm/corepack、镜像与真实 Node |
| macOS Apple Silicon | 构建、单测、假运行时与安装器 | shell PATH、项目切换和真实 Node |

本地日常测试继续使用临时 `PINSET_HOME`、假归档和假运行时，不在开发者机器自动安装
Node、Python 或 Flutter。真实运行时只在明确隔离的 VM/测试用户中执行。

CI 提供仅 `workflow_dispatch` 触发的 `Ubuntu real runtime acceptance`。它在一次性 Ubuntu runner
和临时 `PINSET_HOME` 中安装两个真实 Node 版本，验证全局选择、项目覆盖、离开项目恢复
全局、npm/corepack 和中文设置；普通 PR 推送不会触发该下载型验收。

### 2.10 Release Gates

`v0.1.0-alpha.2` 必须满足：

- 现有 alpha.1 项目配置和锁文件无迁移即可使用。
- 全局、项目、系统三层解析矩阵全部通过。
- 已声明但不可用的版本不会静默降级。
- shim 系统透传无递归、无参数丢失并保留退出码。
- npm/corepack 子进程可以找到所选 Node。
- 全局配置与锁写入具备失败恢复和并发测试。
- `doctor` 能解释实际选择与 PATH 冲突，但不修改用户环境。
- Quality、Linux x64、Windows x64、macOS arm64 Release 构建通过。
- Ubuntu VM 完成一次真实全局版本、项目覆盖、离开项目恢复全局版本的验收。
- `curl | sh` 仍强制验证 SHA-256，且安装器不修改 shell profile。
- 用户可见变化已经写入 Release Notes。
- 英文与简体中文 CLI 测试通过，语言切换不创建或修改项目文件。

### 2.11 Explicitly Out of Scope

alpha.2 不包含：

- CPython、Flutter 或新的运行时 provider。
- `node@24`、`node@lts`、`node@current` 等浮动选择器。
- `pinset exec node@<selector>` 一次性覆盖。
- 自动创建 Python 虚拟环境或管理 npm/pip/pub 包依赖。
- 自动修改 shell profile、IDE 配置或系统级 PATH。
- 自动删除 nvm、fnm、mise、asdf、系统 Node 或 `$HOME/pinset.toml`。
- 第三方 Homebrew Tap、Scoop Bucket 或社区镜像 preset。
- 云账号、同步、遥测、插件、钩子和项目任意代码执行。

### 2.12 Risks

| 风险 | 应对 |
| --- | --- |
| 项目缺失安装时静默使用全局版本 | 声明即绑定；不可用时失败关闭 |
| 系统 PATH 透传递归调用 shim | 搜索时排除 shim 目录和当前可执行文件，并设置深度保护 |
| 全局状态与项目锁共用导致语义混乱 | 使用独立 `state/global.toml` 与 `global.lock` |
| `$HOME/pinset.toml` 过渡方案继续覆盖全局 | `doctor` 解释优先级，只提供手动迁移建议 |
| 全局与项目命令实现两套解析逻辑 | 先建立统一核心解析结果，所有入口只消费它 |
| 扩展系统透传造成 alpha.2 范围失控 | 只处理 Node 附带命令，不扩展任意插件或 Shell activation |
| 真实平台验证成本上升 | 自动化使用假运行时，真实下载限定到隔离 VM 和发布验收 |

## 3. v0.1.0-alpha.3 — Node Lifecycle and Migration

目标：完成 Node provider 的日常版本发现、安装生命周期和旧管理器共存闭环。

状态：**已发布**

当前实现：

- [x] `node@<major>`、`node@<major.minor>`、`node@lts`、`node@current` 解析为精确稳定版本后写锁。
- [x] 精确 `node@x.y.z` 保持原有离线选择语义，不为日常使用额外读取版本索引。
- [x] 只选择同时提供 Windows x64、Linux x64、macOS x64/arm64 所需产物的官方版本。
- [x] `pinset list node` 只列出带完整 Pinset 安装收据的本地版本，不访问网络。
- [x] `pinset list node --available` 显式读取 Node 官方索引并显示日期、LTS 与安全更新标记。
- [x] 英文与简体中文输出、假官方索引和假安装目录测试。
- [x] `pinset uninstall node@<exact-version>` 默认保护当前项目和全局引用，`--force` 仍只删除带匹配收据的 Pinset 安装。
- [x] 下载归档按 SHA-256 内容寻址缓存；支持离线复用、`cache list` 和所有权安全的 `cache clean`。
- [x] `source test node [alias]` 只读验证版本索引和最新稳定版 SHASUMS，不下载运行时归档。
- [x] `doctor --json` 输出 schema 1 机器可读报告。
- [x] `.nvmrc`、`.node-version`、Volta、asdf 和 mise 的 `import --dry-run` 只读检测与冲突报告。
- [x] `pinset exec node@<selector> -- <command>` 一次性选择已安装版本，不修改项目或全局配置。
- [x] workspace 版本冻结为 `0.1.0-alpha.3`，本地格式、Clippy、测试和 Release 构建通过。
- [x] 发布 `v0.1.0-alpha.3` 标签并复核公开 Release 资产、SHA-256、归档内容与版本输出。

这一切片的索引信任规则：版本发现固定读取 Node 官方 HTTPS `index.json`；镜像仍只改变归档
传输位置，不能提供或替换版本身份。索引中的预发布版本和缺少任一支持目标产物的版本不会
参与浮动选择器匹配。`current` 表示最新可锁定稳定版，`lts` 表示最新可锁定 LTS 版本。

范围：

- `node@24`、`node@24.12`、`lts`、`current` 等选择器解析为精确版本后写锁。
- 明确稳定版、预发布版和 LTS 别名的新鲜度与信任规则。
- `pinset list node`、`pinset list node --available`。
- `pinset uninstall node@<exact-version>`，被项目或全局引用时默认拒绝。
- 显式缓存查看和清理，删除范围不得超出 Pinset 数据目录。
- `pinset source test node [alias]`，只诊断 DNS/TLS/HTTP/路径/校验能力。
- `doctor --json` 稳定机器可读结构。
- 对 `.nvmrc`、`.node-version`、Volta 和 mise/asdf 文件进行只读检测与 `import --dry-run`。
- `pinset exec node@<selector> -- <command>` 一次性选择，不修改项目或全局状态。

发布门禁：

- 浮动选择器只在写锁或显式联网查询时访问网络，日常执行不访问网络。
- 卸载不会删除外部管理器或 Pinset 数据根之外的文件。
- 缓存只清理 `downloads/sha256/<64 hex>.archive` 普通文件，未知文件和非普通文件不会被跟随或删除。
- `source test` 读取远程元数据；显式允许 HTTP 的源会清楚报告 TLS 不适用。
- 多来源冲突时停止并解释，不自动覆盖项目配置。
- 系统 PATH 和旧管理器组合至少覆盖五种可复现环境。

## 4. v0.1.0-alpha.4 — Node Workflow Hardening

状态：**已发布**

目标：先把 Node 的全局默认、项目覆盖和命令发现体验收口，再扩展第二种运行时。

范围：

- 新增 `pinset global [node@selector]` 一等入口；无参数只读查看，带参数设置并默认安装。
- 保留 `pinset use node@... --global` 和现有 schema 的完全兼容。
- 在项目覆盖全局默认时明确显示全局值、项目值和生效配置路径。
- `pinset current` 继续代表当前目录的最终生效结果，`pinset global` 代表用户默认值。
- `pinset shim install` 自动推导发布包内的 shim 二进制和用户级目标目录，并提供 `shim path`。
- Node 归档下载在交互终端显示进度条、百分比和字节数，非交互环境输出简洁状态。
- 完善中英文顶层帮助、README 全局/项目流程和临时目录回归测试。

不包含：

- 自动修改 shell profile、系统 PATH 或 IDE 设置。
- 安装 Python、Flutter 或其他新的运行时 Provider。
- 为便利性放宽哈希、安全解压、锁匹配或安装所有权边界。

验收：

- 用户无需了解全局状态文件位置即可设置、查看和更新全局 Node。
- 在有/无项目配置、有/无安装的组合下，输出明确且解析优先级不变。
- 新入口不访问项目文件；只读查看不创建 `PINSET_HOME`。
- 所有自动测试仅使用临时目录和假运行时，不下载真实 Node。

当前证据：

- `cargo fmt --all -- --check` 通过。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` 通过。
- 安装器测试验证下载开始、字节推进、SHA-256 校验完成事件；缓存命中不产生下载事件。
- `cargo test --workspace --all-features` 共 102 项测试通过。
- `git diff --check` 通过。
- 自动化仅写临时目录、假 shim 与假运行时；未在开发机下载或安装真实 Node。
- Release 工作流 [#31567989648](https://github.com/Future-Element/pinset/actions/runs/31567989648)
  完成质量门禁和 Linux x64、Windows x64、macOS arm64 构建，发布 5 个预期资产。
- 发布后重新下载全部资产，`SHA256SUMS` 四项复算一致；三个归档只含预期 CLI 与 shim，
  Windows 产物输出 `pinset 0.1.0-alpha.4` 并包含 `global` 命令。

## 5. v0.1.0-alpha.5 — CPython Provider

目标：在不接管依赖和虚拟环境的前提下，提供可审计的 CPython 解释器安装与选择。

范围：

- 固定并版本化 python-build-standalone 产物清单。
- 锁文件记录来源、许可证、目标、哈希和必要 provenance。
- 路由 `python`、`python3`、`pip`、`pip3` 及归档实际提供的版本化命令。
- 与 uv/pip 的职责边界和真实项目流程。
- Python 镜像、自定义 CA 和已有锁文件离线安装。
- `.python-version` 只读导入与冲突诊断。

不包含：

- 自动创建或激活虚拟环境。
- 管理 requirements、pyproject 依赖或发布 Python 包。
- 依赖未固定的“最新构建”远程状态复现锁文件。

## 6. v0.1.0-alpha.6 — Flutter Provider

目标：提供 Flutter stable 与 Dart 的可预测版本选择，并形成可操作的 IDE SDK 流程。

范围：

- Flutter stable provider、发布清单与归档验证。
- `flutter` 与同 SDK 内 `dart` 路由。
- `.fvmrc` 只读导入。
- 项目稳定 SDK 路径、`which --sdk` 和 IDE 配置文档。
- Windows junction、Unix symlink、权限和 IDE 缓存观察性验证。
- Flutter 官方中国指南中的镜像兼容方式，但不默认选择第三方镜像。

不包含：

- 管理 pub 包依赖。
- 自动改写 VS Code、Android Studio 或 Xcode 项目配置。
- 未经用户确认替换 FVM 或系统 Flutter。

## 7. v0.1.0-beta.1 — Integrated Public Preview

目标：验证三种内置 provider 能否以一套配置、锁、解析和诊断模型共同工作。

发布条件：

- Node、CPython、Flutter 各至少一个固定版本通过认证平台真实安装与执行。
- 同一项目声明多个工具时，每个工具独立解析且共享确定性锁文件。
- 离线模式可以根据已有锁和缓存复现，不访问未批准来源。
- 旧管理器共存、迁移、IDE 和 CI 核心流程完成真实项目验收。
- schema、锁文件兼容和 provider 更新策略形成书面承诺。
- SBOM、artifact attestation、SHA-256 与发布来源对外可验证。

## 8. v0.1.0 — Stable Preview

`v0.1.0` 冻结以下承诺：

- Node、CPython 和 Flutter 的已认证平台与版本范围。
- `pinset.toml`、`pinset.lock` 和全局状态 schema 的兼容策略。
- 项目、全局、兼容文件和系统 PATH 的解析语义。
- provider 元数据、校验、镜像、缓存和离线安装的信任边界。
- 安装、升级、卸载、恢复和诊断不会越过 Pinset 数据所有权。
- GitHub Release 三平台产物、校验和、SBOM 和 provenance 验证流程。

稳定版不等于覆盖所有语言、平台和版本，也不建立第三方插件执行生态。

## 9. Release Channels

- GitHub Releases 是当前官方二进制分发渠道。
- Linux x64 与 macOS Apple Silicon 使用官方 `install.sh`；Windows 使用 Release ZIP。
- 项目不维护第三方 Homebrew Tap 或 Scoop Bucket。
- Homebrew、Scoop、WinGet 等中央渠道只有在满足其官方接收和持续维护政策后单独评估。
- alpha/beta 预发布必须使用固定版本 URL；`releases/latest` 只面向稳定 Release。

## 10. Common Release Gates

每个版本冻结前必须：

- 版本号、Cargo workspace、Git tag 和 Release 名称一致。
- 格式、Clippy、workspace 测试、安装器离线测试和 shim 依赖约束通过。
- Linux x64、Windows x64、macOS arm64 构建通过。
- Release 归档只包含预期文件，SHA-256 与下载后复算一致。
- 已发布平台完成相称的安装、启动和核心命令验收；只构建成功不等于运行验收。
- 文档明确支持矩阵、已知限制、升级影响和未执行的人工验收。
- 用户可见变化进入 [发布说明](RELEASE_NOTES.md)。
- 安全问题按 [安全策略](../SECURITY.md) 处理，不在公开 Issue 泄露未修复细节。

## 11. Research Baseline

调研基线日期：2026-07-28。产品行为和技术机制优先采用官方文档、官方仓库和官方机器
可读索引；社区讨论只用于发现问题，不作为已确认结论。

### 11.1 市场结论

asdf、mise、proto 和 vfox 已经证明“一个工具管理多种运行时”不是市场空白。Pinset 不能
只靠统一命令形成价值，继续开发必须聚焦：

- Node.js、CPython、Flutter 三个生态的高质量内置适配；
- 从首版开始稳定且记录产物来源与校验信息的锁文件；
- 完全不可执行的项目配置；
- Windows、PATH、旧管理器和 IDE 冲突的一等诊断；
- 国内镜像、企业代理和离线缓存下仍保持可信校验；
- CI、终端和 IDE 使用同一个可解释解析结果。

如果真实访谈和可用性测试显示目标用户认为现有 mise/proto 已充分解决问题，Pinset 应缩小
到迁移/诊断工具或停止扩张，不以增加 provider 掩盖需求不足。

### 11.2 竞品启示

| 工具 | 已有优势 | Pinset 取舍 |
| --- | --- | --- |
| asdf | `.tool-versions`、成熟插件生态 | v0.1 不执行任意 Shell 插件 |
| mise | 多后端、环境、任务、activation/shim | 不在广度竞争，聚焦锁和信任边界 |
| proto | Rust、WASM 插件、shim、锁 | WASM 只作为未来能力隔离候选 |
| vfox | 原生跨平台、Lua 插件、项目配置 | Windows 体验重要，但首版不开放脚本插件 |
| fnm/nvm | Node 专用和旧项目兼容 | 只读检测旧文件，不自动改写 |
| uv | 高质量 Python 安装、依赖与虚拟环境 | Pinset 只提供解释器，uv 是互补工具 |
| FVM | Flutter 缓存和项目选择 | 必须同时解决 IDE 稳定 SDK 路径 |

竞品的脚本或插件能力并非天然缺陷；Pinset 的不可执行配置是自身供应链和信任模型选择。

### 11.3 官方运行时来源

Node.js：

- 官方 `dist/index.json` 提供版本索引，发布目录提供归档和 `SHASUMS256.txt`。
- alpha.1 使用官方 HTTPS 清单中的 SHA-256；稳定版目标是验证授权发布密钥签名清单。
- 只安装官方预编译包，不在 v0.1 本地编译 Node。

CPython：

- 计划采用 Astral `python-build-standalone` 的 install-only 归档和 `PYTHON.json`。
- 首批只考虑现代 CPython 常规构建，不承诺 PyPy、debug、free-threaded 和历史 GPL 依赖构建。
- 必须冻结清单发布/签名、许可证、证书、pip 引导和动态库策略。

Flutter：

- 官方三平台 release JSON 提供 channel、版本、Git 引用、归档路径和 SHA-256。
- 首批只认证 stable；Flutter bundle 自带 Dart，锁文件必须表达该关系。
- 上游平台矩阵是动态事实，不能把 2026-07-28 快照永久硬编码。

### 11.4 镜像与分发结论

- `official` 始终存在，镜像只替换 artifact base URL。
- Node 没有官方指定的唯一中国镜像，公共加速服务只能标记为用户批准的第三方传输来源。
- 不基于 IP、地区或后台测速静默换源。
- 项目不维护第三方 Homebrew Tap 或 Scoop Bucket。
- npm 已存在无作用域同名包，因此不以 `npm install -g pinset` 作为官方分发方式。
- GitHub Releases 是当前官方渠道；未来中央渠道必须满足其官方接收和持续维护要求。

## 12. Decision Registry

`Accepted` 是已确定产品方向；`Provisional` 仍需技术验证或用户证据；`Deferred` 不进入当前
版本。相同主题的细节以 [PRD](PRD.md) 和本文件对应版本契约为准。

| ID | 状态 | 决策 |
| --- | --- | --- |
| D-001 | Accepted | 产品名为 Pinset，命令为 `pinset` |
| D-002 | Accepted | 首版内置管理 Node.js、CPython、Flutter |
| D-003 | Accepted | 产品语义覆盖 Windows、macOS、Linux，WSL 独立处理 |
| D-004 | Accepted | 只管理运行时，不管理包依赖或 Python 虚拟环境 |
| D-005 | Accepted | 项目配置是不可执行 TOML |
| D-006 | Accepted | 从 v0.1 提供可提交的稳定锁文件 |
| D-007 | Accepted | Detect many, activate one；共存可观察，解析结果唯一 |
| D-008 | Accepted | v0.1 只有内置 provider，不开放任意脚本插件 |
| D-009 | Accepted | Rust 单一核心与独立轻量 shim |
| D-010 | Provisional | 日常项目选择以 shim 为主；性能仍需三平台认证 |
| D-011 | Accepted | 安装先校验、事务提交，半安装不可见 |
| D-012 | Accepted | 镜像不能替换可信校验值 |
| D-013 | Accepted | 默认无账号、无遥测 |
| D-014 | Provisional | Windows x64、Linux x64、macOS x64/arm64 为首批认证候选矩阵 |
| D-015 | Provisional | Flutter 使用项目稳定 SDK 别名解决 IDE 路径 |
| D-016 | Provisional | CPython 使用固定 python-build-standalone 清单 |
| D-017 | Provisional | 未声明工具最终安全透传系统 PATH |
| D-018 | Provisional | shim p95 额外开销候选目标不超过 10 ms |
| D-019 | Accepted | 安装源是传输位置，不是信任根 |
| D-020 | Accepted | 活动源属于本机配置，不写入项目锁 |
| D-021 | Accepted | 仅网络失败允许源回退，校验失败硬停止 |
| D-022 | Deferred | 社区镜像 preset 未冻结，不默认提供或启用 |
| D-023 | Accepted | 自定义源默认 HTTPS，局域网 HTTP 必须逐源显式批准 |
| D-024 | Accepted | `official` 是不可覆盖、不可删除的内置别名 |
| D-025 | Provisional | 浮动 Node 选择器等待可信索引与锁流程 |
| D-026 | Accepted | canonical URL 与有序下载候选分离 |
| D-027 | Accepted | 分发首选 GitHub Releases，不维护第三方 Tap/Bucket |
| D-028 | Accepted | 先发布 Node-only alpha，再扩展 Python/Flutter |
| D-029 | Accepted | Node MVP 锁定四个平台的精确稳定产物 |
| D-030 | Provisional | Node alpha 使用官方 HTTPS SHASUMS；稳定版补 PGP 验签 |
| D-031 | Accepted | Pinset 使用 MIT License 开源 |

变更规则：

- Accepted 决策被推翻时，在本节新增替代决策、证据、兼容和迁移影响，不重写历史理由。
- Provisional 转为 Accepted 时必须引用自动化、真实平台或用户研究结果。
- 扩大远程执行、权限、数据收集、项目写入或删除范围必须单独安全评审。

## 13. Spike and Validation Evidence

### 13.1 Shim Spike

2026-07-28 Windows x64 早期结果：

```text
core resolver: median 99 us, p95 195 us, p99 289 us
direct process: median 6164 us, p95 7588 us
shimmed process: median 14188 us, p95 25606 us
estimated p95 overhead: 18017 us
```

结论：

- 最近祖先配置、假 Node 选择、参数/退出码透传、用户目录 shim 和递归保护功能通过。
- 核心解析不是瓶颈。
- Windows 未签名 spike 的完整 shim p95 额外开销约 18 ms，未达到 10 ms 候选目标。
- 数据只代表当时机器，不能外推到 Linux、macOS 或签名 Release。
- alpha.1 已补充 Linux/WSL 功能验证，但三平台性能认证仍未完成，D-010/D-018 保持 Provisional。

后续必须比较签名/LTO 构建、真实 Node 相对开销和必要时的 Shell activation/显式 exec 降级，
不能用核心函数微基准替代完整用户路径。

### 13.2 Installer Spike

早期本地 fixture 已验证：

- 流式 SHA-256、下载和展开上限；
- 错误哈希、截断响应、路径穿越和失败后无最终安装；
- 同一文件系统临时目录与原子 rename；
- 网络错误回退、哈希错误禁止回退；
- 收据 URL 脱敏；
- source 配置原子替换和 official 保护；
- shim 依赖树不包含 reqwest、归档、哈希和 source 写入依赖。

alpha.1 已进一步完成：

- Node 官方 SHASUMS 精确版本解析和四目标锁；
- Windows ZIP、Linux/macOS TAR.XZ；
- Unix 安全归档符号链接；
- 完全一致安装的幂等复用；
- WSL 真实 Node、npm、corepack 执行；
- GitHub 三平台 Release 构建和离线安装器测试。

仍未关闭：

- 显式跨进程安装文件锁和突然断电持久性；
- 下载缓存、断点续传、离线包导入和清理；
- Node PGP、SBOM、attestation 和密钥轮换；
- Python/Flutter 真实产物、许可证和 IDE 验收；
- Windows 长路径、ADS、更多 Unicode/大小写冲突真实样本。

### 13.3 Remaining Spikes

- Flutter IDE：`which --sdk`、稳定路径、VS Code/Android Studio、Windows junction。
- Python 来源：固定清单、`PYTHON.json`、许可证、镜像、自定义 CA 和离线缓存。
- 共存诊断：fnm、nvm、uv、FVM、mise、asdf、vfox 与系统安装的真实组合。
- 系统 PATH 透传：排除 shim、自递归、命令身份和已声明版本失败关闭。

## 14. Development and Release Workflow

### 14.1 本地开发

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --locked -p pinset-cli -p pinset-shim
```

测试默认使用临时 `PINSET_HOME`、本地假 HTTP、构造归档和假运行时，不安装真实 Node、
Python 或 Flutter。真实上游测试必须放在明确隔离的 VM、测试用户或 CI job，并记录平台。

### 14.2 CI

- 非草稿 PR 在 opened、reopened、synchronize、ready_for_review 时运行 `Quality`。
- `main` 推送再次运行 `Quality`。
- `Quality` 包含格式、Clippy、workspace 测试、curl 安装器离线测试和 shim 依赖约束。
- 手动 workflow 可以构建 Linux x64、Windows x64、macOS arm64 归档。
- 版本标签触发完整 Release workflow，不手工上传资产。

### 14.3 发布前

1. 更新 Cargo workspace 版本和 `Cargo.lock`。
2. 更新 PRD 当前边界、Plans 状态、README 和 Release Notes。
3. 在 PR 中通过 Quality 并完成相称的真实环境验收。
4. 合并到 `main`，确认主分支 Quality 成功。
5. 检查平台支持、已知限制、schema/锁兼容和升级说明。
6. 确认目标 tag 尚未存在，工作树与 `origin/main` 对齐。

### 14.4 触发发布

标签必须与 workspace 版本完全一致：

```bash
git tag -a v0.1.0-alpha.4 -m "Pinset 0.1.0-alpha.4"
git push origin v0.1.0-alpha.4
```

Release workflow：

1. 校验 tag 与 workspace 版本。
2. 运行 Quality 与离线安装器测试。
3. 构建 Linux x64、Windows x64、macOS arm64。
4. 生成平台归档和 `SHA256SUMS`。
5. 发布 `install.sh`、归档和校验文件。
6. 含连字符版本自动标记为 GitHub Prerelease。

失败时修复并发布新版本；不得覆盖已经公开使用的版本标签或静默替换 Release 资产。

### 14.5 发布后

- 确认 Release 非草稿，预发布标记与版本语义一致。
- 核对预期资产名称和数量。
- 重新下载全部资产并复算 SHA-256。
- 在 Linux/WSL 使用固定版本 URL 安装到明确临时目录。
- 执行 `pinset --version`，alpha.2 起验收全局/项目切换。
- 记录未执行的平台、真实运行时、IDE、签名或安装渠道验收。

## 15. User Research and Go/No-Go

访谈目标为 8–12 位跨语言或跨平台开发者，可用性测试目标为 5–8 位。验证：

- 当前实际使用哪些管理器，冲突和恢复成本是什么；
- 锁文件、镜像校验和 `doctor` 是否解决真实问题；
- 是否愿意在已有工具旁试用，而不是只认可“统一命令”的概念；
- 五分钟内能否完成安装、初始化、切换、锁定安装和问题定位；
- 哪些自动 PATH/IDE/卸载行为会破坏信任。

不得使用 GitHub star 代替迁移意愿、留存或需求证据。默认无遥测，研究只能使用用户主动
反馈、公开 Issue、可复现样本和明确同意的访谈。

Go/No-Go：

- Node 全局/项目闭环无法做到唯一且可解释时，不进入 Python。
- Python 可信清单与许可证无法冻结时，不发布 Python provider。
- Flutter IDE 稳定路径无法接受时，不用终端 shim 成功掩盖 IDE 缺口。
- 多语言价值不能被用户研究证明时，缩小到 Node/迁移诊断或终止扩张。

## 16. Open Decisions

- Node 发布密钥轮换、离线密钥环和 PGP 失败恢复。
- Python 固定清单的发布、签名和许可证展示。
- Flutter release metadata 与 provenance 的验证层级。
- 系统 PATH 透传从 Provisional 转为 Accepted 的真实组合矩阵。
- shim 三平台性能目标和 Windows 降级策略。
- 缓存保留、断点续传、离线包导入和安全清理语义。
- 自定义 CA、企业代理凭据和诊断脱敏。
- 首批申请哪些中央包管理器官方渠道及其持续维护责任。
- 包名、域名和商标的正式清查。
