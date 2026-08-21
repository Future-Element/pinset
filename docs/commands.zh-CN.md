# Pinset 命令文档

[English](commands.md) | [简体中文](commands.zh-CN.md) · [README](../README.zh-CN.md)

本文档描述 Pinset v2.1 命令行协议。可以运行 `pinset <command> --help` 查看当前二进制附带的精确参数帮助。

## 通用约定

### 选择器与作用域

选择表达式格式为 `<tool>@<selector>`，例如 `node@22`、`pnpm@latest`、`java@lts` 或 `rust@stable`。schema 4 项目配置会保留这个请求选择器，schema 3 锁文件记录精确解析版本。

支持的工具为 Node.js、pnpm、Bun、Go、Python、Java、Rust、.NET 和 Flutter。Dart 由所选 Flutter SDK 提供。项目发现默认在最近的 Git 根目录停止；没有 Git 标记时只检查起始目录。项目默认严格：未声明工具不会继承全局状态，也不会回退系统命令；只有 `[policy]` 显式启用 `inherit-global` 或 `system-fallback` 时才允许。项目之外仍按全局状态、系统 `PATH` 的顺序解析。

全局选项 `--lang <en|zh-CN>` 用于选择单次调用的输出语言。不带子命令运行 `pinset --lang <language>` 会保存默认语言。

### 状态

- 项目选择：`pinset.toml` 与 `pinset.lock`。
- 全局选择：`PINSET_HOME/state/global.toml` 与 `global.lock`。
- 本机设置、源、下载缓存、安装和收据：位于 `PINSET_HOME`。
- `--cwd <path>` 从指定路径开始查找项目。
- `--dry-run` 只报告计划中的破坏性操作，不执行修改。

### JSON schema 1

只有下文标记为“**支持**”的命令接受 `--json`。它们向标准输出写入一个 JSON 文档：

```json
{"schema":1,"command":"current","ok":true,"data":{}}
```

```json
{"schema":1,"command":"current","ok":false,"error":{"code":"runtime_missing","message":"...","details":{}}}
```

`command` 是稳定标识，二级命令使用 `cache.verify` 等名称。错误 `code` 是稳定的 snake_case 标识；本地化 `message` 面向用户，`details` 是经过脱敏的自动化上下文。参数、配置、元数据、安装和完整性错误在 JSON 模式下也遵循同一结构。

### 退出码

- `0`：Pinset 成功完成。
- `1`：`pinset lock audit` 已完成，但发现一个或多个需要处理的错误或警告。只有信息级发现时仍返回 `0`。
- `2`：Pinset 使用、配置、元数据、完整性或安装失败。
- `pinset exec` 与 `pinset x`：成功启动子进程后，透传子进程的精确退出码；启动前的 Pinset 错误返回 `2`。

下表会在必要位置重复特殊行为；其余命令均遵循这些退出码。

## 项目与选择命令

### `init`

| 字段 | 说明 |
| --- | --- |
| 用途 | 在当前目录创建最小项目配置。 |
| 语法与参数 | `pinset init`；没有命令专属选项。 |
| 修改状态 | **是。** 创建包含唯一 `project-id` 与严格项目策略的 schema 4 `pinset.toml`，但不选择或安装运行时。 |
| 示例 | `mkdir app && cd app && pinset init` |
| JSON | 不支持。 |
| 退出码 | 成功为 `0`；无法安全创建文件为 `2`。 |
| 关键错误 | 配置已存在、不安全路径或文件系统权限失败。 |

### `detect`

| 字段 | 说明 |
| --- | --- |
| 用途 | 读取传统项目版本文件，报告版本选择、范围约束、忽略的工具、不支持值和冲突。 |
| 语法与参数 | `pinset detect [--cwd <目录>] [--json]`。扫描在最近的 `.git` 文件/目录处停止；不存在 Git 标记时只扫描起始目录。 |
| 修改状态 | **否。** 不联网、不创建 Pinset 状态、不执行第三方工具，也不修改来源文件。 |
| 示例 | `pinset detect --cwd ./app --json` |
| JSON | **支持。** `data` 包含 `start`、`boundary`、`target_config`、`can_import` 以及按 Provider 稳定排序的 `findings`。 |
| 退出码 | 本地扫描完成为 `0`，即使报告含冲突或没有可导入选择；只有无法开始扫描时返回 `2`。 |
| 关键错误 | 起始目录不存在或不可访问。不安全、畸形、不可表示的来源文件会作为报告项返回，而不是命令错误。 |

可导入来源包括 `.nvmrc`、`.node-version`、`.bun-version`、`.go-version`、`.python-version`、`.java-version`、`.sdkmanrc`、`rust-toolchain(.toml)`、`global.json`、`.fvmrc`、旧版 FVM 项目 JSON、`.tool-versions`、`mise.toml`，以及 `package.json`、`go.mod`、`go.work` 中无歧义的字段。包清单中的版本范围只报告、不导入。符号链接、非普通文件、非 UTF-8 来源和超过 1 MiB 的文件会在报告中被拒绝。

### `import`

| 字段 | 说明 |
| --- | --- |
| 用途 | 重新扫描，并把所有可安全映射的传统选择导入 schema 4 `pinset.toml` 与 schema 3 `pinset.lock`。 |
| 语法与参数 | `pinset import [--cwd <目录>] [--force] [--no-install]`。`--force` 只替换本次发现且现有请求选择器不同的工具。 |
| 修改状态 | **是。** 解析元数据，先锁文件、后配置分别进行原子文件替换，并默认安装项目全部选择。`--no-install` 跳过运行时归档和 Python `.venv`，但仍解析并锁定元数据。 |
| 示例 | `pinset import --no-install` |
| JSON | 不支持。 |
| 退出码 | 完整导入/安装后为 `0`；没有选择、存在阻断项、现有 Pinset 状态无效、解析/写入失败或安装失败为 `2`。 |
| 关键错误 | 来源冲突、不支持值、现有锁缺失/不匹配、未带 `--force` 的版本替换，或 Provider 元数据不可用。 |

导入不会读取其他运行时管理器的安装状态，不执行管理器任务或 hook，也不删除传统文件。若状态提交后的安装失败，有效配置与锁会保留，可运行 `pinset install --locked` 继续安装。

### `global`

| 字段 | 说明 |
| --- | --- |
| 用途 | 查看全局选择，或批量设置任意组合的全局默认运行时。 |
| 语法与参数 | `pinset global [<tool>@<selector>...] [--no-install]`。不带选择时仍是只读查看；`--no-install` 至少需要一个选择。每个 Provider 在同一批次只能出现一次。 |
| 修改状态 | 不带选择：**否**。带选择：**是。** Pinset 先解析完整批次，再执行一次配置/锁更新，保留未提及的选择；除非使用 `--no-install`，之后只执行一轮锁定安装。 |
| 示例 | `pinset global node@lts python@latest rust@stable` |
| JSON | 不支持。 |
| 退出码 | 成功为 `0`；Pinset 失败为 `2`。 |
| 关键错误 | Provider 重复/不支持、选择器无效、元数据不可用、清单不受信任、依赖/策略失败、下载/完整性失败或目标平台不支持。状态提交前失败不修改选择；安装失败保留完整新锁，并报告全局锁定安装重试命令。 |

### `use`

| 字段 | 说明 |
| --- | --- |
| 用途 | 为最近项目解析并锁定一个或多个运行时，或写入全局作用域。 |
| 语法与参数 | `pinset use <tool>@<selector>... [--no-install] [--global]`。至少需要一个选择，每个 Provider 只能出现一次。 |
| 修改状态 | **是。** 先解析所有选择，再执行一次作用域配置/锁更新，保留未提及的选择；除非使用 `--no-install`，之后只按依赖顺序执行一轮锁定安装。 |
| 示例 | `pinset use java@lts dotnet@lts flutter@latest` |
| JSON | 不支持。 |
| 退出码 | 成功为 `0`；Pinset 失败为 `2`。 |
| 关键错误 | 缺少项目配置、Provider 重复、选择器无效、元数据/签名、依赖/项目策略失败、平台不支持或安装失败。状态提交前失败不修改选择；安装失败保留完整新锁，并报告锁定安装重试命令。 |

### `unset`

| 字段 | 说明 |
| --- | --- |
| 用途 | 删除一个项目或全局选择，但不卸载对应运行时。 |
| 语法与参数 | `pinset unset <tool> [--global | --cwd <path>]`。 |
| 修改状态 | **是。** 只更新所选配置和锁。 |
| 示例 | `pinset unset python --cwd ./app` |
| JSON | 不支持。 |
| 退出码 | 成功为 `0`；作用域无效或写入失败为 `2`。 |
| 关键错误 | 工具不支持、找不到项目、选择不存在或配置/锁写入失败。 |

### `install`

| 字段 | 说明 |
| --- | --- |
| 用途 | 安装一个显式精确运行时，或安装项目/全局锁中的全部目标。 |
| 语法与参数 | `pinset install [<tool>@<exact-version>] [--locked] [--global | --cwd <path>]`。显式选择与锁作用域选项冲突；项目安装默认要求锁定状态。 |
| 修改状态 | **是。** 写入缓存、运行时文件、收据和命令路由；锁定的 Python 项目可能创建或验证 `.venv`。不会修改选择。 |
| 示例 | `pinset install --locked --cwd ./app` |
| JSON | 不支持。 |
| 退出码 | 成功为 `0`；Pinset 失败为 `2`。 |
| 关键错误 | 显式版本不精确、配置与锁不匹配、旧 Node 锁需要重新锁定、签名缺失、完整性失败、归档不安全或安装事务失败。 |

## 查询与生命周期命令

### `which`

| 字段 | 说明 |
| --- | --- |
| 用途 | 输出 Pinset 将为某命令使用的精确可执行文件。 |
| 语法与参数 | `pinset which <command> [--cwd <path>] [--explain] [--json]`。 |
| 修改状态 | 否。 |
| 示例 | `pinset which node --json` |
| JSON | **支持**；命令名为 `which`。使用 `--explain` 时，`data.explanation` 包含边界、候选链、策略结果和只用于迁移的传统来源。 |
| 退出码 | 成功解析为 `0`；无法解析可用命令为 `2`。 |
| 关键错误 | 未知受管命令、所选运行时缺失、锁无效或没有符合条件的系统回退。 |

### `current`

| 字段 | 说明 |
| --- | --- |
| 用途 | 显示当前生效的项目、全局或系统运行时选择和可执行文件。 |
| 语法与参数 | `pinset current [tool] [--cwd <path>] [--explain] [--json]`；默认工具为 Node.js。 |
| 修改状态 | 否。 |
| 示例 | `pinset current python --cwd ./app` |
| JSON | **支持**；命令名为 `current`，同时包含 `requested` 与精确 `version`；`--explain` 增加解析轨迹。 |
| 退出码 | 成功解析为 `0`；选择或安装不可用为 `2`。 |
| 关键错误 | 工具不支持、配置/锁无效、运行时缺失或系统回退被阻止。 |

### `list`

| 字段 | 说明 |
| --- | --- |
| 用途 | 列出已安装版本，或查询一个 Provider 的官方可用版本。 |
| 语法与参数 | `pinset list [tool] [--available] [--json]`。`--available` 必须同时指定 `tool`。 |
| 修改状态 | 否。 |
| 示例 | `pinset list java --available --json` |
| JSON | **支持**；命令名为 `list`，版本位于 `data.versions`。 |
| 退出码 | 成功为 `0`；参数或元数据失败为 `2`。 |
| 关键错误 | Provider 不支持、网络/元数据失败、签名元数据无效或不受信任、响应超限。 |

### `outdated`

| 字段 | 说明 |
| --- | --- |
| 用途 | 将每个精确锁定版本同时与“请求选择器允许的最新版本”和“最新稳定版本”比较。 |
| 语法与参数 | `pinset outdated [tool] [--global | --cwd <path>] [--json]`。 |
| 修改状态 | 否。 |
| 示例 | `pinset outdated --cwd ./app --json` |
| JSON | **支持**；命令名为 `outdated`，`data.runtimes` 包含 `requested`、`current`、`latest_compatible`、`latest`、`update_available` 与 `upgrade_available`。 |
| 退出码 | 完整比较后为 `0`；作用域、锁或元数据验证失败为 `2`。 |
| 关键错误 | 项目缺失、工具不支持、锁无效或 Provider 元数据失败。 |

### `update`

| 字段 | 说明 |
| --- | --- |
| 用途 | 重新解析请求选择器并刷新精确锁记录，不改变选择器，也不安装运行时。 |
| 语法与参数 | `pinset update [tool] [--global | --cwd <path>] [--dry-run] [--json]`。 |
| 修改状态 | **是**，但 `--dry-run` 时不修改；只更新所选锁文件。 |
| 示例 | `pinset update node --cwd ./app --dry-run` |
| JSON | **支持**；命令名为 `update`，包含旧/新精确版本和请求选择器。 |
| 退出码 | 完成比较/写入为 `0`；作用域、锁或 Provider 元数据失败为 `2`。 |
| 关键错误 | 项目/选择缺失、锁无效、工具不支持或元数据不可用。 |

### `migrate`

| 字段 | 说明 |
| --- | --- |
| 用途 | 验证并把 schema 1–3 项目配置重写为 schema 4，同时保持运行时锁为 schema 3，不重新解析版本。 |
| 语法与参数 | `pinset migrate [--global | --cwd <path>] [--dry-run] [--json]`。 |
| 修改状态 | **是**，但 `--dry-run` 时不修改；仅以逐文件原子替换方式规范化配置与锁。 |
| 示例 | `pinset migrate --cwd ./app --dry-run` |
| JSON | **支持**；命令名为 `migrate`，包含来源与目标 schema。 |
| 退出码 | 验证/迁移完成为 `0`；无法证明配置与锁一致时为 `2`。 |
| 关键错误 | 配置/锁缺失、schema 不支持、配置与锁不匹配或写入失败。 |

### `lock audit`

| 字段 | 说明 |
| --- | --- |
| 用途 | 审计一组项目或全局配置/锁、当前平台制品、相关内容寻址缓存、安装收据与由收据证明的所有权。项目选择 Python 时还会审计 `.venv` 所有权标记。 |
| 语法与参数 | `pinset lock audit [--global | --cwd <path>] [--json]`。默认使用项目作用域，并遵循正常的仓库边界发现规则。 |
| 修改状态 | **否。** 命令始终只读，不会执行修复计划，也不会访问 Provider 元数据或归档服务。缓存检查只散列当前选择、当前平台制品所引用的缓存项。 |
| 示例 | `pinset lock audit --cwd ./app --json` |
| JSON | **支持**；命令名为 `lock.audit`。审计正常完成时，即使 `data.passed` 为 false，外层仍为 `ok: true`。`data.findings` 中返回稳定的 `reason_code`、`severity`、`category`、`subject`，以及可选的 `path` 和 `repair`。 |
| 退出码 | 没有错误或警告时为 `0`；审计完成但存在需处理错误/警告时为 `1`；只有命令解析或审计启动本身失败时才为 `2`。可选缓存缺失属于信息，不会导致退出码 `1`。 |
| 关键发现 | 配置/锁缺失、无效或过旧，选择器漂移，Provider 不受支持，当前平台制品缺失，缓存缺失/损坏/不安全，安装缺失/不安全，收据无效或不匹配，以及 Python 环境所有权无效。 |

稳定 reason code 按类别如下：

- 配置与锁：`config_missing`、`config_invalid`、`config_schema_legacy`、`lock_missing`、`lock_invalid`、`lock_schema_legacy`、`lock_tool_missing`、`lock_tool_unconfigured`、`lock_selector_mismatch`。
- Provider 与平台：`provider_unsupported`、`provider_audit_unsupported`、`platform_artifact_missing`、`platform_artifact_invalid`。
- 缓存：`cache_entry_missing`、`cache_entry_corrupt`、`cache_entry_unsafe`、`cache_entry_unreadable`。
- 收据与所有权：`install_missing`、`install_path_unsafe`、`receipt_missing`、`receipt_unreadable`、`receipt_invalid`、`receipt_schema_legacy`、`receipt_schema_unsupported`、`receipt_incomplete`、`receipt_identity_mismatch`、`receipt_integrity_missing`、`receipt_integrity_mismatch`、`receipt_overlay_mismatch`、`python_environment_missing`、`python_environment_ownership_invalid`。

### `uninstall`

| 字段 | 说明 |
| --- | --- |
| 用途 | 删除一个由 Pinset 所有的精确版本运行时安装。 |
| 语法与参数 | `pinset uninstall <tool>@<exact-version> [--force] [--cwd <path>] [--dry-run] [--json]`。 |
| 修改状态 | **是**，但 `--dry-run` 时不修改。只删除具有有效 Pinset 所有权证据的安装。 |
| 示例 | `pinset uninstall node@22.0.0 --dry-run --json` |
| JSON | **支持**；命令名为 `uninstall`。 |
| 退出码 | 完成计划/删除为 `0`；保护机制阻止或验证失败为 `2`。 |
| 关键错误 | 版本不精确、运行时仍被选择引用、收据缺失/无效、路径不安全或安装不归 Pinset 所有。`--force` 只绕过选择引用，不绕过所有权检查。 |

### `prune`

| 字段 | 说明 |
| --- | --- |
| 用途 | 删除未被全局或所提供项目选择保护的已安装版本。 |
| 语法与参数 | `pinset prune [--cwd <path>] [--project <path>]... [--dry-run] [--json]`。 |
| 修改状态 | **是**，但 `--dry-run` 时不修改。 |
| 示例 | `pinset prune --project ./app --project ../service --dry-run` |
| JSON | **支持**；命令名为 `prune`。 |
| 退出码 | 完成计划/删除为 `0`；无法验证引用或所有权为 `2`。 |
| 关键错误 | 项目锁无效、安装路径不安全、收据缺失或文件系统失败。 |

### `exec`

| 字段 | 说明 |
| --- | --- |
| 用途 | 使用 Pinset 所选运行时与环境启动子命令，不依赖 Shell 直接路由。 |
| 语法与参数 | `pinset exec [--cwd <path>] -- <command> [args...]`。子命令前可带精确工具选择，例如 `pinset exec node@22.0.0 -- node -v`。 |
| 修改状态 | Pinset 状态：**否**。被启动程序可能修改自身文件或外部状态。 |
| 示例 | `pinset exec -- node ./scripts/build.js` |
| JSON | 不支持；子进程 stdout/stderr 不会被包装。 |
| 退出码 | 启动后透传子进程精确退出码；Pinset 无法解析或启动时为 `2`。 |
| 关键错误 | 缺少命令、运行时无法解析、精确覆盖版本未安装、shim 防递归保护或进程启动失败。 |

### `x`

| 字段 | 说明 |
| --- | --- |
| 用途 | 解析、验证、安装并运行一次 Provider 命令，同时不改变项目/全局选择状态。 |
| 语法与参数 | `pinset x <tool>@<selector> [--cwd <path>] -- <command> [args...]`。命令必须属于所选择的 Provider。 |
| 修改状态 | 选择状态：**否**。可能在 `PINSET_HOME` 下创建已验证下载、缓存项、安装收据与运行时；子进程可能修改自己的文件或外部状态。 |
| 示例 | `pinset x node@24 -- node ./scripts/build.js` |
| JSON | 不支持；子进程 stdout/stderr 不封装。 |
| 退出码 | 成功启动后透传子进程精确退出码；Pinset 无法解析、验证、安装或启动时返回 `2`。 |
| 关键错误 | 选择器无效、命令与 Provider 不匹配、元数据/制品验证失败、声明的 Provider 依赖缺失、平台不支持或进程启动失败。pnpm 需要有效的项目/全局 Node.js 选择。 |

### `doctor`

| 字段 | 说明 |
| --- | --- |
| 用途 | 诊断项目边界与严格策略、锁、安装、命令路由、环境变量、PATH 状态和只用于迁移的传统来源。 |
| 语法与参数 | `pinset doctor [--cwd <path>] [--json]`。 |
| 修改状态 | 否。 |
| 示例 | `pinset doctor --json` |
| JSON | **支持**；命令名为 `doctor`。 |
| 退出码 | 诊断完成为 `0`；输入无法读取或验证为 `2`。诊断发现会写入数据，并不一定导致命令失败。 |
| 关键错误 | 配置/锁无法读取、状态格式错误、路径不安全或文件系统失败。 |

## 下载缓存命令

缓存按完整性标识保存已验证归档。缓存检查不会把文件名本身当作完整性证据。

### `cache`

| 字段 | 说明 |
| --- | --- |
| 用途 | 组合下载缓存检查、验证、修复、清理和离线导入操作。 |
| 语法与参数 | `pinset cache <list|info|verify|repair|clean|import> ...`；必须指定二级命令。 |
| 修改状态 | 取决于二级命令：`repair`、`clean` 与 `import` 会修改缓存状态。 |
| 示例 | `pinset cache info` |
| JSON | 没有一级命令输出；`list`、`info`、`verify`、`repair` 与 `clean` 支持 `--json`。 |
| 退出码 | 二级命令成功为 `0`；二级命令缺失/无效或缓存失败为 `2`。 |
| 关键错误 | 二级命令缺失、缓存路径不安全、内容损坏、完整性值无效或文件系统失败。 |

### `cache list`

| 字段 | 说明 |
| --- | --- |
| 用途 | 列出完整的内容寻址运行时归档。 |
| 语法与参数 | `pinset cache list [--json]`。 |
| 修改状态 | 否。 |
| 示例 | `pinset cache list --json` |
| JSON | **支持**；命令名为 `cache.list`，条目位于 `data.entries`。 |
| 退出码 | 成功为 `0`；无法检查缓存元数据为 `2`。 |
| 关键错误 | 缓存条目不安全或文件系统读取失败。 |

### `cache info`

| 字段 | 说明 |
| --- | --- |
| 用途 | 汇总完整与未完成下载的缓存用量。 |
| 语法与参数 | `pinset cache info [--json]`。 |
| 修改状态 | 否。 |
| 示例 | `pinset cache info` |
| JSON | **支持**；命令名为 `cache.info`。 |
| 退出码 | 成功为 `0`；缓存检查失败为 `2`。 |
| 关键错误 | 缓存目录无法读取或条目元数据无效。 |

### `cache verify`

| 字段 | 说明 |
| --- | --- |
| 用途 | 计算每个完整归档的摘要，并与内容标识比较。 |
| 语法与参数 | `pinset cache verify [--json]`。 |
| 修改状态 | 否。 |
| 示例 | `pinset cache verify --json` |
| JSON | **支持**；命令名为 `cache.verify`。损坏会返回 `ok: false` 文档。 |
| 退出码 | 所有条目通过时为 `0`；条目损坏或检查失败为 `2`。 |
| 关键错误 | 摘要不匹配、文件截断、条目不安全或读取失败。 |

### `cache repair`

| 字段 | 说明 |
| --- | --- |
| 用途 | 删除损坏的完整归档，使后续安装可以重新下载。 |
| 语法与参数 | `pinset cache repair [--dry-run] [--json]`。 |
| 修改状态 | **是**，但 `--dry-run` 时不修改；只处理已验证为损坏的完整归档。 |
| 示例 | `pinset cache repair --dry-run --json` |
| JSON | **支持**；命令名为 `cache.repair`。 |
| 退出码 | 完成计划/删除为 `0`；无法安全分类或删除条目为 `2`。 |
| 关键错误 | 路径不安全、权限失败或缓存内容在验证时发生变化。 |

### `cache clean`

| 字段 | 说明 |
| --- | --- |
| 用途 | 删除下载缓存中的完整内容寻址归档。 |
| 语法与参数 | `pinset cache clean [--dry-run] [--json]`。 |
| 修改状态 | **是**，但 `--dry-run` 时不修改；不会删除已安装运行时。 |
| 示例 | `pinset cache clean --dry-run` |
| JSON | **支持**；命令名为 `cache.clean`。 |
| 退出码 | 完成计划/删除为 `0`；路径不安全或文件系统失败为 `2`。 |
| 关键错误 | 条目不归 Pinset 所有/不安全，或删除失败。 |

### `cache import`

| 字段 | 说明 |
| --- | --- |
| 用途 | 把经过审查的归档导入已验证离线缓存。 |
| 语法与参数 | `pinset cache import <archive> (--sha256 <hex> | --integrity <SRI>)`；两种完整性选项互斥。 |
| 修改状态 | **是。** 校验匹配后按内容标识复制归档，但不安装。 |
| 示例 | `pinset cache import ./node.tar.xz --sha256 <reviewed-digest>` |
| JSON | 不支持。 |
| 退出码 | 校验后导入成功为 `0`；参数、摘要或写入失败为 `2`。 |
| 关键错误 | 未提供预期完整性、摘要不匹配、SRI/SHA-256 无效、来源不安全或缓存写入失败。 |

## Python 环境命令

只有当项目 `.venv` 的所有权标记与当前项目和所选 CPython 发行版一致时，Pinset 才认为它由自己所有。无法证明所有权时，破坏性操作会失败关闭。

### `venv`

| 字段 | 说明 |
| --- | --- |
| 用途 | 组合项目所有的 Python 环境操作。 |
| 语法与参数 | `pinset venv <create|status|recreate> ...`；必须指定二级命令。 |
| 修改状态 | 取决于二级命令；`create` 与 `recreate` 会修改状态。 |
| 示例 | `pinset venv status` |
| JSON | 不支持。 |
| 退出码 | 二级命令成功为 `0`；二级命令缺失/无效或环境失败为 `2`。 |
| 关键错误 | 二级命令缺失、项目 Python 选择缺失、所有权标记无效或环境创建失败。 |

### `venv create`

| 字段 | 说明 |
| --- | --- |
| 用途 | 必要时安装所选 CPython，然后创建或验证项目 `.venv`。 |
| 语法与参数 | `pinset venv create [--cwd <path>]`。 |
| 修改状态 | **是。** 可能安装 Python，并创建 `.venv` 及其所有权标记。 |
| 示例 | `pinset venv create --cwd ./app` |
| JSON | 不支持。 |
| 退出码 | 环境就绪为 `0`；Pinset 失败为 `2`。 |
| 关键错误 | 项目未选择 Python、锁不匹配、目标不支持、安装失败、已存在外部 `.venv` 或标记不匹配。 |

### `venv status`

| 字段 | 说明 |
| --- | --- |
| 用途 | 显示所选 CPython 发行版与受管项目环境路径。 |
| 语法与参数 | `pinset venv status [--cwd <path>]`。 |
| 修改状态 | 否。 |
| 示例 | `pinset venv status` |
| JSON | 不支持。 |
| 退出码 | 能确定状态为 `0`；项目或所有权状态无效为 `2`。 |
| 关键错误 | Python 选择缺失、锁无效、所有权标记缺失/不匹配或环境无法读取。 |

### `venv recreate`

| 字段 | 说明 |
| --- | --- |
| 用途 | 在证明 Pinset 所有权后删除并重建项目 `.venv`。 |
| 语法与参数 | `pinset venv recreate [--cwd <path>]`。 |
| 修改状态 | **是。** 只替换具有正确标记、归 Pinset 所有的 `.venv`。 |
| 示例 | `pinset venv recreate --cwd ./app` |
| JSON | 不支持。 |
| 退出码 | 重建成功为 `0`；验证或重建失败为 `2`。 |
| 关键错误 | 所有权标记缺失/无效、路径逃逸、所选 Python 不匹配、删除失败或 venv 创建失败。 |

## 命令路由命令

### `shim`

| 字段 | 说明 |
| --- | --- |
| 用途 | 组合 Provider 命令路由的检查与修复操作。 |
| 语法与参数 | `pinset shim <path|install|migrate> ...`；必须指定二级命令。 |
| 修改状态 | 取决于二级命令；`install` 与 `migrate` 会修改路由条目。 |
| 示例 | `pinset shim path` |
| JSON | 不支持。 |
| 退出码 | 二级命令成功为 `0`；二级命令缺失/无效或路由失败为 `2`。 |
| 关键错误 | 二级命令缺失、路由路径不安全、所有权冲突或 shim 二进制缺失。 |

### `shim path`

| 字段 | 说明 |
| --- | --- |
| 用途 | 输出保存 Pinset 命令 shim 的用户所有目录。 |
| 语法与参数 | `pinset shim path`。 |
| 修改状态 | 否。 |
| 示例 | `pinset shim path` |
| JSON | 不支持。 |
| 退出码 | 成功为 `0`；Pinset 主目录/路由路径无效为 `2`。 |
| 关键错误 | 缺少主目录上下文或配置路径不安全。 |

### `shim install`

| 字段 | 说明 |
| --- | --- |
| 用途 | 修复命令 shim，但不覆盖不归 Pinset 所有的文件。 |
| 语法与参数 | `pinset shim install [--binary <pinset-shim>] [--dir <path>] [--provider <tool> | <COMMAND>...]`。 |
| 修改状态 | **是。** 在目标目录创建或修复受管 shim 条目。 |
| 示例 | `pinset shim install --provider node` |
| JSON | 不支持。 |
| 退出码 | 请求的路由就绪为 `0`；验证/写入失败为 `2`。 |
| 关键错误 | Provider 不支持、命令名无效、shim 二进制缺失、已有文件不归 Pinset 所有或权限失败。 |

### `shim migrate`

| 字段 | 说明 |
| --- | --- |
| 用途 | 在当前路由目录注册已配置 Provider 命令，同时保留已有条目。 |
| 语法与参数 | `pinset shim migrate [--provider <tool>] [--dir <path>]`。 |
| 修改状态 | **是。** 只修复路由条目；这不是配置/锁迁移命令。 |
| 示例 | `pinset shim migrate --provider python` |
| JSON | 不支持。 |
| 退出码 | 成功为 `0`；所有权或路由验证失败为 `2`。 |
| 关键错误 | Provider 不支持、shim 二进制缺失、冲突条目不归 Pinset 所有或文件系统失败。 |

### `activate`

| 字段 | 说明 |
| --- | --- |
| 用途 | 输出把 Pinset 命令路由目录放到 `PATH` 前面的 Shell 代码。 |
| 语法与参数 | `pinset activate <bash|zsh|fish|powershell>`。 |
| 修改状态 | 否。调用者自行决定是否执行或保存输出代码。 |
| 示例 | `eval "$(pinset activate zsh)"` |
| JSON | 不支持。 |
| 退出码 | 成功为 `0`；Shell 或路径配置无效为 `2`。 |
| 关键错误 | Shell 值不支持或路由目录无效。 |

### `completions`

| 字段 | 说明 |
| --- | --- |
| 用途 | 为受支持 Shell 生成 Pinset 补全代码。 |
| 语法与参数 | `pinset completions <bash|zsh|fish|powershell>`。 |
| 修改状态 | 否；Shell 重定向可能创建文件。 |
| 示例 | `pinset completions fish > ~/.config/fish/completions/pinset.fish` |
| JSON | 不支持。 |
| 退出码 | 成功为 `0`；Shell 值无效为 `2`。 |
| 关键错误 | Shell 值不支持或输出写入失败。 |

## 下载源命令

自定义源配置目前适用于 Node.js、Go、Python 和 Flutter。制品镜像与可信元数据镜像拥有不同安全权限：只有指定 `--trust-metadata`，自定义 HTTPS 源才可以决定版本或完整性元数据。对于 Node.js，可信元数据源还必须提供签名清单。

### `source`

| 字段 | 说明 |
| --- | --- |
| 用途 | 组合本机 Provider 下载源检查、选择、策略与验证操作。 |
| 语法与参数 | `pinset source <list|add|use|fallback|remove|test> ...`；必须指定二级命令。 |
| 修改状态 | 取决于二级命令；`add`、`use`、`fallback` 与 `remove` 会修改本机源配置。 |
| 示例 | `pinset source list` |
| JSON | 不支持。 |
| 退出码 | 二级命令成功为 `0`；二级命令缺失/无效或源失败为 `2`。 |
| 关键错误 | 二级命令缺失、Provider 不支持、URL/信任策略无效、别名未知或元数据验证失败。 |

### `source list`

| 字段 | 说明 |
| --- | --- |
| 用途 | 列出内置与自定义源，可限制为一个 Provider。 |
| 语法与参数 | `pinset source list [node|go|python|flutter]`。 |
| 修改状态 | 否。 |
| 示例 | `pinset source list node` |
| JSON | 不支持。 |
| 退出码 | 成功为 `0`；配置/Provider 验证失败为 `2`。 |
| 关键错误 | 源 Provider 不支持或源配置格式错误。 |

### `source add`

| 字段 | 说明 |
| --- | --- |
| 用途 | 添加具名自定义制品源，并可选择授予可信元数据权限。 |
| 语法与参数 | `pinset source add <provider> <alias> --base-url <url> [--allow-insecure | --trust-metadata]`。HTTP 必须指定 `--allow-insecure`，且该选项与元数据权限冲突。 |
| 修改状态 | **是。** 写入本机 `sources.toml`；项目锁文件不变。 |
| 示例 | `pinset source add node mirror --base-url https://mirror.example/node` |
| JSON | 不支持。 |
| 退出码 | 成功为 `0`；URL、信任策略或写入失败为 `2`。 |
| 关键错误 | Provider 不支持、别名保留/重复、URL 无效、未显式允许 HTTP 或信任选项组合无效。 |

### `source use`

| 字段 | 说明 |
| --- | --- |
| 用途 | 为一个受支持 Provider 选择活动源。 |
| 语法与参数 | `pinset source use <provider> <alias>`。 |
| 修改状态 | **是。** 更新本机源配置；已有锁文件不变。 |
| 示例 | `pinset source use go mirror` |
| JSON | 不支持。 |
| 退出码 | 成功为 `0`；查找或写入失败为 `2`。 |
| 关键错误 | 别名未知、Provider 不支持或配置无效。 |

### `source fallback`

| 字段 | 说明 |
| --- | --- |
| 用途 | 替换一个 Provider 的有序回退源列表。 |
| 语法与参数 | `pinset source fallback <provider> [alias]...`；不传别名会清空列表。 |
| 修改状态 | **是。** 替换本机回退顺序。 |
| 示例 | `pinset source fallback python mirror-a mirror-b official` |
| JSON | 不支持。 |
| 退出码 | 成功为 `0`；验证/写入失败为 `2`。 |
| 关键错误 | 别名未知或重复、与活动源冲突、Provider 不支持或配置格式错误。 |

### `source remove`

| 字段 | 说明 |
| --- | --- |
| 用途 | 删除一个非活动的自定义源。 |
| 语法与参数 | `pinset source remove <provider> <alias>`。 |
| 修改状态 | **是。** 删除本机源条目。 |
| 示例 | `pinset source remove flutter old-mirror` |
| JSON | 不支持。 |
| 退出码 | 成功为 `0`；不允许删除或无法保存为 `2`。 |
| 关键错误 | 内置源、活动源、仍被回退列表引用、别名未知或 Provider 不支持。 |

### `source test`

| 字段 | 说明 |
| --- | --- |
| 用途 | 对一个源执行只读连接与 Provider 元数据验证。 |
| 语法与参数 | `pinset source test <provider> [alias]`；省略别名时使用活动源。 |
| 修改状态 | 否。 |
| 示例 | `pinset source test node mirror` |
| JSON | 不支持。 |
| 退出码 | 连接与元数据验证均成功为 `0`；其他情况为 `2`。 |
| 关键错误 | 网络失败、响应超限、元数据无效、Node 签名缺失/无效、签名者未知、不安全策略或别名未知。 |

## Provider Registry 命令

v1.8 Registry 是只读预览。已验证 manifest 可以描述命令、依赖、共享 capability 和来源证明方法，但不能安装、激活或执行 Provider；只有编译进当前 Pinset 二进制的 Provider 才会生效。Registry 必须是大小受限的普通文件，并且只包含一个由 Pinset 固定 Registry 公钥验证通过的 cleartext OpenPGP 签名。

### `provider list`

| 字段 | 说明 |
| --- | --- |
| 用途 | 验证并列出内嵌声明式 Provider Registry。 |
| 语法与参数 | `pinset provider list [--json]`。 |
| 修改状态 | 否。不联网、不安装运行时、不激活 Provider，也不执行 manifest 内容。 |
| 示例 | `pinset provider list --json` |
| JSON | **支持**；命令名为 `provider.list`，包含签名文档与签名者指纹。 |
| 退出码 | 签名、schema、capability、依赖图及内置声明全部验证通过为 `0`；否则为 `2`。 |
| 关键错误 | 内嵌密钥/签名无效、capability 未知、命令重复、依赖缺失、循环或声明漂移。 |

### `provider verify`

| 字段 | 说明 |
| --- | --- |
| 用途 | 验证内嵌 Registry 或一个本地 clear-signed Registry 文件，但不激活它。 |
| 语法与参数 | `pinset provider verify [REGISTRY] [--json]`；省略路径时验证内嵌 Registry。 |
| 修改状态 | 否。本地文件只读；不会安装、激活或执行 Provider。 |
| 示例 | `pinset provider verify registry/providers.json.asc --json` |
| JSON | **支持**；命令名为 `provider.verify`，包含验证后的文档与签名者指纹。 |
| 退出码 | 密码学、schema、capability 与依赖验证全部通过为 `0`；否则为 `2`。 |
| 关键错误 | 符号链接/非文件、输入超过 256 KiB、未签名或多重签名、签名者不匹配、内容篡改、未知字段/capability、依赖缺失或循环。 |

## 加密项目环境命令

Pinset 用相互独立的 [age](https://age-encryption.org/) 密文 profile 管理项目级字符串环境变量。公开 recipient 和密文文件应进入仓库；私有 identity 和恢复口令不应进入仓库。Pinset 不会自动读取 `.env`、生成临时明文文件、插值变量，也不是通用 Secrets Vault。

第一台电脑的常规流程如下：

```sh
# 仅现有 schema 1–3 项目需要；新的 `pinset init` 项目已是 schema 4。
pinset migrate

# 创建 pinset.env/development.age、设备身份和已加密的恢复身份。
pinset env init --profile development --auto \
  --recovery ~/pinset-development-recovery.age
pinset env set DATABASE_URL --profile development
pinset env list --profile development
pinset trust add

# 直接 shim 选择运行时并注入已信任的 auto-profile。
node app.js
```

提交 `pinset.toml` 和 `pinset.env/*.age`，但把恢复文件和 identity 文件留在仓库外。恢复文件和口令应分开备份；如果丢失所有匹配 identity，密文将无法恢复。

Profile 名称限制为 1–64 位 ASCII 字母、数字、点、下划线或短横线。配置的密文路径必须位于规范化项目边界内，并解析为非符号链接普通文件。解密后 profile 数据上限为 1 MiB；继承环境加注入环境超过平台环境块限制时，Pinset 也会拒绝启动。

### Profile 选择与注入

对于 `--profile` 可选的命令，依次使用显式 `--profile`、`PINSET_ENV_PROFILE`、`[environment].auto-profile`。三者都没有时，Pinset 会要求指定 `--profile`。语法中必填 `--profile` 的命令不使用这个回退顺序。

`node`、`python`、`cargo`、`flutter` 等直接 Provider 命令只在项目已信任，且 `PINSET_ENV_PROFILE` 或 `auto-profile` 选中 profile 时注入。`pinset exec --profile <名称> -- <命令>` 可显式选择。`PINSET_ENV_DISABLE=1` 和 `pinset exec --no-env -- <命令>` 可对单次启动禁用注入。

`[environment].collision` 不区分变量名大小写，默认为 `error`：

- `error`：加密变量名已存在于进程或运行时环境时，启动前报错。
- `process-wins`：保留现有值，丢弃该名称的加密值。
- `encrypted-wins`：用加密值替换现有值。

`PINSET_IDENTITY`、`PINSET_IDENTITY_FILE`、`PINSET_ENV_PROFILE` 和 `PINSET_ENV_DISABLE` 会在业务进程启动前被移除。这可避免无意透传，但不是进程隔离：已获得秘密的进程仍可以把它传给其他程序。

### `env init`

| 字段 | 说明 |
| --- | --- |
| 用途 | 创建一个空的加密 profile、一个设备 age X25519 identity，通常还会创建独立恢复 identity。 |
| 语法与参数 | `pinset env init --profile <名称> [--auto] (--recovery <路径> | --no-recovery) [--identity-file <路径>] [--cwd <路径>]`。`--identity-file` 会把设备 identity 保存到口令保护文件，而非系统密钥库。 |
| 修改状态 | **是。** 创建 `pinset.env/<profile>.age`、更新 schema 4 `pinset.toml`、保存设备 identity，并可能创建恢复文件。`--auto` 把该 profile 设为 `auto-profile`。 |
| 示例 | `pinset env init --profile ci --recovery ~/pinset-ci-recovery.age` |
| JSON | 不支持。 |
| 关键错误 | 项目不是 schema 4、profile/文件已存在、profile 名无效、密钥库不可用、路径不安全、恢复输出已存在或加密/写入失败。最后配置写入失败时会删除新密文；操作前面已创建的 identity 或恢复文件可能仍保留，需人工检查。 |

只有在已有其他经过验证的 identity 备份方案时才使用 `--no-recovery`。在没有可用系统密钥库的 Linux/SSH 环境中，使用 `--identity-file <路径>`，后续交互命令通过 `PINSET_IDENTITY_FILE` 指向该受保护文件。

### `env set`

| 字段 | 说明 |
| --- | --- |
| 用途 | 在 profile 中新增或替换一个加密变量。 |
| 语法与参数 | `pinset env set <变量名> [--profile <名称>] [--stdin] [--cwd <路径>]`。不使用 `--stdin` 时，Pinset 通过隐藏终端输入读值；值永远不是位置参数。 |
| 修改状态 | **是。** 解密、修改、使用新的 age 文件密钥材料重新加密，并在文件锁下原子替换密文。 |
| 示例 | `pinset env set DATABASE_URL --profile development` |
| JSON | 不支持。 |
| 关键错误 | 没有选定/匹配 identity、变量名无效、输入失败、profile 畸形/超限、路径不安全或加密/写入失败。 |

可移植变量名匹配 `[A-Za-z_][A-Za-z0-9_]*`。名称按 ASCII 大小写不敏感唯一；`PATH` 和所有 `PINSET_*` 名称为保留项。值可为空或多行，但不能包含 NUL。`--stdin` 读取完整标准输入并删除一个末尾换行；需确保输入生产者不会在自身参数、日志或文件中泄露该值。

### `env unset`

| 字段 | 说明 |
| --- | --- |
| 用途 | 不区分大小写地匹配并删除一个变量。 |
| 语法与参数 | `pinset env unset <变量名> [--profile <名称>] [--cwd <路径>]`。 |
| 修改状态 | **是。** 即使报告该名称原本未设置，也会原子重新加密 profile。 |
| 示例 | `pinset env unset LEGACY_TOKEN --profile development` |
| JSON | 不支持。 |
| 关键错误 | 名称无效、profile 或 identity 缺失、密文不安全/已损坏或写入失败。 |

### `env list`

| 字段 | 说明 |
| --- | --- |
| 用途 | 列出一个 profile 的变量名，不向输出写入变量值。 |
| 语法与参数 | `pinset env list [--profile <名称>] [--json] [--cwd <路径>]`。 |
| 修改状态 | 否。profile 只在内存中解密。 |
| 示例 | `pinset env list --profile ci --json` |
| JSON | **支持。** 命令名为 `env.list`，包含 `profile` 和 `names`，永远不包含值。 |
| 关键错误 | 没有选中 profile、没有匹配 identity、密文不安全/已损坏或 profile schema 不支持。 |

### `env reveal`

| 字段 | 说明 |
| --- | --- |
| 用途 | 有意进行人工检查时，打印一个解密值。 |
| 语法与参数 | `pinset env reveal <变量名> --profile <名称> [--cwd <路径>]`。标准输入和输出都必须是交互终端。 |
| 修改状态 | 否。 |
| 示例 | `pinset env reveal DATABASE_URL --profile development` |
| JSON | 不支持。 |
| 关键错误 | 重定向/非交互终端、变量未设置、没有匹配 identity 或密文无效。 |

值会写到终端，可能留在滚动回溯中。日常检查应优先使用 `env list`。

### `env import`

| 字段 | 说明 |
| --- | --- |
| 用途 | 把显式指定的明文 dotenv 文件导入一个加密 profile。 |
| 语法与参数 | `pinset env import --from <路径> --profile <名称> [--cwd <路径>]`。 |
| 修改状态 | **是。** 同名变量替换 profile 中的值，其他现有变量保留。不修改或删除来源文件。 |
| 示例 | `pinset env import --from .env --profile development` |
| JSON | 不支持。 |
| 关键错误 | UTF-8/赋值/名称无效、变量名按大小写折叠后重复、`export`、插值、命令替换、Shell 表达式、不支持的转义、引号未闭合、identity 缺失或加密失败。 |

可移植子集支持空行、`#` 注释、空值、无引号值、单/双引号、引号内多行值，以及双引号内的 `\n`、`\r`、`\t`、`\\` 和 `\"` 转义。它永远不执行输入。验证导入后，需自行删除或保护明文来源。

### `env export`

| 字段 | 说明 |
| --- | --- |
| 用途 | 当目标系统无法使用 Pinset 注入时，有意把一个 profile 导出为明文 dotenv 文件。 |
| 语法与参数 | `pinset env export --profile <名称> --format dotenv --output <路径> --allow-plaintext [--cwd <路径>]`。`dotenv` 是唯一格式。 |
| 修改状态 | **是。** 创建新明文文件，永远不覆盖现有路径。 |
| 示例 | `pinset env export --profile development --format dotenv --output ./local.env --allow-plaintext` |
| JSON | 不支持。 |
| 关键错误 | 缺少同意标志、输出已存在/不安全、权限收紧失败、identity 缺失或密文无效。 |

输出会限制为当前用户可读（Unix 上为 `0600`，Windows 上为仅当前用户 ACL），但它仍是明文。不要提交，明确用途完成后应立即删除。

### `env recipient add`

| 字段 | 说明 |
| --- | --- |
| 用途 | 允许另一个 age X25519 identity 解密某个 profile。 |
| 语法与参数 | `pinset env recipient add <age1...> --profile <名称> [--cwd <路径>]`。 |
| 修改状态 | **是。** 先用现有 identity 解密，再原子重新加密到去重后的新 recipient 集合，最后更新 `pinset.toml`。 |
| 示例 | `pinset env recipient add age1example... --profile production` |
| JSON | 不支持。 |
| 关键错误 | recipient 无效、无现有匹配 identity、profile 未声明、密文不安全/已损坏或事务写入失败。 |

### `env recipient remove`

| 字段 | 说明 |
| --- | --- |
| 用途 | 在证明当前解密权限后，从 profile 删除一个 recipient。 |
| 语法与参数 | `pinset env recipient remove <age1...> --profile <名称> [--cwd <路径>]`。 |
| 修改状态 | **是。** 以事务方式重新加密并更新配置；配置更新失败时恢复原密文。 |
| 示例 | `pinset env recipient remove age1example... --profile production` |
| JSON | 不支持。 |
| 关键错误 | recipient 无效、尝试删除最后一个 recipient、无匹配 identity 或重新加密/配置失败。 |

新增或删除 recipient 会改变受信任的环境策略。需同时提交 `pinset.toml` 和新密文，然后在每台电脑和 CI 中重新执行 `pinset trust add`。删除 recipient 可阻止该 identity 未来解密，但无法撤回之前已获取的明文。

### `env recipient list`

| 字段 | 说明 |
| --- | --- |
| 用途 | 打印某个 profile 配置的公开 recipients。 |
| 语法与参数 | `pinset env recipient list --profile <名称> [--cwd <路径>]`。 |
| 修改状态 | 否。只读取已提交配置，不解密 profile。 |
| 示例 | `pinset env recipient list --profile production` |
| JSON | 不支持。 |
| 关键错误 | 缺少项目/配置，或 profile 未声明。 |

### `env identity create`

| 字段 | 说明 |
| --- | --- |
| 用途 | 生成一个额外 age X25519 identity，并打印其 ID 和公开 recipient。 |
| 语法与参数 | `pinset env identity create [--output <路径>]`。没有 `--output` 时私有 identity 存入系统密钥库；指定后创建新的口令保护 identity 文件。 |
| 修改状态 | **是。** 写入密钥库和本地 identity 元数据，或创建受保护的输出文件。 |
| 示例 | `pinset env identity create` |
| JSON | 不支持。打印的 `age1...` recipient 是公开信息；私有 identity 永远不打印。 |
| 关键错误 | 密钥库不可用、输出已存在、口令确认不匹配、权限失败或密码学失败。 |

把打印的 recipient 交给 `env recipient add`。仅创建 identity 不会自动获得现有 profile 的访问权。

### `env identity import`

| 字段 | 说明 |
| --- | --- |
| 用途 | 把口令保护的恢复/备份 identity 还原到当前电脑的系统密钥库。 |
| 语法与参数 | `pinset env identity import --from <路径>`。口令通过隐藏输入读取。 |
| 修改状态 | **是。** 把解密 identity 加入密钥库和本地 identity 元数据；不修改来源备份。 |
| 示例 | `pinset env identity import --from ~/pinset-development-recovery.age` |
| JSON | 不支持。 |
| 关键错误 | 口令错误、输入已损坏/不是 identity、密钥库不可用或元数据写入失败。 |

新电脑 Clone 后，执行 `pinset install --locked`、导入匹配的恢复 identity、执行 `pinset trust add`，之后即可正常直接使用 shim。

### `env identity list`

| 字段 | 说明 |
| --- | --- |
| 用途 | 列出已注册 identity 的 ID、公开 recipient 和存储后端，不包含私钥。 |
| 语法与参数 | `pinset env identity list [--json]`。 |
| 修改状态 | 否。 |
| 示例 | `pinset env identity list --json` |
| JSON | **支持。** 命令名为 `env.identity.list`。 |
| 关键错误 | 本地 identity 元数据无效或无法读取。 |

### `env identity backup`

| 字段 | 说明 |
| --- | --- |
| 用途 | 把一个密钥库 identity 备份为新的口令保护 age 文件。 |
| 语法与参数 | `pinset env identity backup <id> --output <路径>`。会要求输入并确认新备份口令。 |
| 修改状态 | **是。** 创建受保护输出，不覆盖现有文件。 |
| 示例 | `pinset env identity backup 4c5652e4-... --output ~/pinset-device-backup.age` |
| JSON | 不支持。 |
| 关键错误 | ID 未知、密钥库访问失败、输出已存在、口令不匹配或加密/写入失败。 |

### `env identity export`

| 字段 | 说明 |
| --- | --- |
| 用途 | 把密钥库 identity 导出为明文，主要用于受明确保护的 CI Secret。 |
| 语法与参数 | `pinset env identity export <id> --output <路径> --allow-plaintext`。 |
| 修改状态 | **是。** 创建新的仅当前用户可读明文文件，永远不覆盖。 |
| 示例 | `pinset env identity export 4c5652e4-... --output ./ci-identity.txt --allow-plaintext` |
| JSON | 不支持。 |
| 关键错误 | 缺少同意标志、ID 未知、密钥库失败、输出已存在或权限收紧失败。 |

把文件内容复制到 CI Secret 后应安全删除。永远不要提交该文件，也不要把私有 identity 作为命令行参数。

### `trust add`

| 字段 | 说明 |
| --- | --- |
| 用途 | 批准当前规范化项目及其精确环境策略进行自动注入。 |
| 语法与参数 | `pinset trust add [--project-id <id>] [--cwd <路径>]`。可选 ID 使自动化在检出的项目 `project-id` 不同时失败。 |
| 修改状态 | **是。** 在 `PINSET_HOME/state/trust` 写入本地信任记录；不向仓库添加内容。 |
| 示例 | `pinset trust add --project-id 4c5652e4-0000-4000-8000-000000000000` |
| JSON | 不支持。 |
| 关键错误 | 没有环境配置/`project-id`、期望 ID 不匹配、项目根不安全或信任存储写入失败。 |

信任绑定规范化根路径、`project-id`、`auto-profile`、冲突策略、每个 profile 路径和每个 recipient。修改任意这些内容都必须重新信任；只修改加密变量值不需要。信任不代表项目代码安全。

### `trust status`

| 字段 | 说明 |
| --- | --- |
| 用途 | 检查当前项目和环境策略是否匹配本地信任记录。 |
| 语法与参数 | `pinset trust status [--cwd <路径>] [--json]`。 |
| 修改状态 | 否。 |
| 示例 | `pinset trust status --json` |
| JSON | **支持。** 命令名为 `trust.status`，包含 `trusted`、reason `trusted`/`trust_missing`/`trust_changed` 和规范化 `root`。 |
| 退出码 | 状态检查完成为 `0`，包括未信任状态；自动化应检查 JSON `trusted` 字段。 |
| 关键错误 | 项目环境配置缺失/无效，或信任存储无法读取。 |

### `trust revoke`

| 字段 | 说明 |
| --- | --- |
| 用途 | 删除当前电脑对某项目的自动注入批准。 |
| 语法与参数 | `pinset trust revoke [--cwd <路径>]`。 |
| 修改状态 | **是。** 只删除本地信任记录；密文、配置、identities 和已泄露值不变。 |
| 示例 | `pinset trust revoke` |
| JSON | 不支持。 |
| 关键错误 | 项目发现或信任存储访问失败。对本来就未信任的项目撤销仍为成功，并报告原本无记录。 |

### GitHub Actions identity 流程

创建专用 identity，只把它的公开 recipient 加入 `ci` profile，并把私有 identity 文本存为 GitHub Actions Secret `PINSET_IDENTITY`。该 Secret 可以按行包含多个 identities。显式选择 profile，并固定预期的公开项目 ID：

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

Action 输入不是秘密，也不保存 identity。Pinset 会在子进程启动前移除 `PINSET_IDENTITY`。不要把服务端秘密注入 Flutter、Web、移动端或其他会把环境值编译进客户端制品的构建。

## Pinset 2.0 维护命令

`pinset paths [tool] [--json]` 会报告 CLI、相邻 shim、Pinset home、shim 目录、安装根，以及可选工具的已安装版本。`pinset list [tool] --long` 增加收据 schema、安装根、文件数量、总大小、关键入口与完整性状态。`pinset doctor --deep` 会重新扫描这些统计，但不宣称逐文件密码学验证。`pinset install <tool@精确版本> --repair` 只修复所有权收据与工具、版本、平台和目标目录全部匹配的安装。`pinset shim install --all` 注册所有内置 Provider 命令，但不下载运行时。

`pinset self outdated [--channel stable|prerelease] [--json]` 只在用户明确执行时检查固定官方仓库。`pinset self update [--version <版本>]` 验证平台、语义版本、归档结构与 `SHA256SUMS`，校验新 CLI 后成对替换 CLI/shim，并支持备份与回滚。普通命令和 `doctor` 不会后台检查更新。

## 稳定协议边界

Pinset v2.0 写入 schema 4 项目配置，以及 schema 3 全局配置/运行时锁。schema 1–3 项目仍可读取，并通过显式迁移升级。安装收据独立使用 schema 3，同时继续读取 schema 1/2。项目 `[policy]` 支持可选的 `verification-strength = "checksum" | "signed-checksum" | "provenance"` 和 `minimum-release-age = "<正整数><d|h|m|s>"`；新锁可以记录可选的上游 `released-at`。配置策略会在状态写入、项目安装、包括 dry-run 在内的更新和锁审计中执行；缺少发布时间会失败关闭，已有工具锁也不允许被更弱验证静默替换。

v2.0 不修改 JSON schema 1 外层结构。新增 JSON 命令包括 `paths`、`env.list`、`env.identity.list`、`trust.status` 与 `self.outdated`。自动化应依据稳定的 command 与 reason/code 字段分支，不要匹配面向用户的消息；JSON 输出和错误绝不包含环境变量值、身份或口令。
