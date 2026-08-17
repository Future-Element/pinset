# Pinset 命令文档

[English](commands.md) | [简体中文](commands.zh-CN.md) · [README](../README.zh-CN.md)

本文档描述 Pinset v1.0 命令行协议。可以运行 `pinset <command> --help` 查看当前二进制附带的精确参数帮助。

## 通用约定

### 选择器与作用域

选择表达式格式为 `<tool>@<selector>`，例如 `node@22`、`pnpm@latest`、`java@lts` 或 `rust@stable`。Pinset 会先把选择器解析成精确版本，再写入锁文件。

支持的工具为 Node.js、pnpm、Bun、Go、Python、Java、Rust、.NET 和 Flutter。Dart 由所选 Flutter SDK 提供。最近项目的 `pinset.toml` 优先于全局状态；之后 Pinset 可以回退到符合条件的系统命令。

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
- `2`：Pinset 使用、配置、元数据、完整性或安装失败。
- `pinset exec`：成功启动子进程后，透传子进程的精确退出码；启动前的 Pinset 错误返回 `2`。

下表会在必要位置重复特殊行为；其余命令均遵循这些退出码。

## 项目与选择命令

### `init`

| 字段 | 说明 |
| --- | --- |
| 用途 | 在当前目录创建最小项目配置。 |
| 语法与参数 | `pinset init`；没有命令专属选项。 |
| 修改状态 | **是。** 创建 `pinset.toml`，但不选择或安装运行时。 |
| 示例 | `mkdir app && cd app && pinset init` |
| JSON | 不支持。 |
| 退出码 | 成功为 `0`；无法安全创建文件为 `2`。 |
| 关键错误 | 配置已存在、不安全路径或文件系统权限失败。 |

### `global`

| 字段 | 说明 |
| --- | --- |
| 用途 | 查看全局选择，或设置一个全局默认版本。 |
| 语法与参数 | `pinset global [<tool>@<selector>] [--no-install]`。`--no-install` 必须与选择表达式一起使用。 |
| 修改状态 | 不带选择表达式：**否**。带选择表达式：**是**，写入全局配置和锁；除非指定 `--no-install`，否则同时安装。 |
| 示例 | `pinset global node@lts` |
| JSON | 不支持。 |
| 退出码 | 成功为 `0`；Pinset 失败为 `2`。 |
| 关键错误 | Provider 不支持、选择器无效、元数据不可用、清单不受信任、下载/完整性失败或目标平台不支持。 |

### `use`

| 字段 | 说明 |
| --- | --- |
| 用途 | 为最近项目解析并锁定一个运行时，或写入全局作用域。 |
| 语法与参数 | `pinset use <tool>@<selector> [--no-install] [--global]`。 |
| 修改状态 | **是。** 写入所选作用域的配置和锁；除非指定 `--no-install`，否则同时安装。 |
| 示例 | `pinset use pnpm@10` |
| JSON | 不支持。 |
| 退出码 | 成功为 `0`；Pinset 失败为 `2`。 |
| 关键错误 | 缺少项目配置、选择器无效、元数据/签名失败、平台不支持或安装失败。 |

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
| 语法与参数 | `pinset which <command> [--cwd <path>] [--json]`。 |
| 修改状态 | 否。 |
| 示例 | `pinset which node --json` |
| JSON | **支持**；命令名为 `which`。 |
| 退出码 | 成功解析为 `0`；无法解析可用命令为 `2`。 |
| 关键错误 | 未知受管命令、所选运行时缺失、锁无效或没有符合条件的系统回退。 |

### `current`

| 字段 | 说明 |
| --- | --- |
| 用途 | 显示当前生效的项目、全局或系统运行时选择和可执行文件。 |
| 语法与参数 | `pinset current [tool] [--cwd <path>] [--json]`；默认工具为 Node.js。 |
| 修改状态 | 否。 |
| 示例 | `pinset current python --cwd ./app` |
| JSON | **支持**；命令名为 `current`。 |
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
| 用途 | 将项目和全局所选运行时与当前稳定版本比较。 |
| 语法与参数 | `pinset outdated [tool] [--global | --cwd <path>] [--json]`。 |
| 修改状态 | 否。 |
| 示例 | `pinset outdated --cwd ./app --json` |
| JSON | **支持**；命令名为 `outdated`，结果位于 `data.runtimes`。 |
| 退出码 | 完整比较后为 `0`；作用域、锁或元数据验证失败为 `2`。 |
| 关键错误 | 项目缺失、工具不支持、锁无效或 Provider 元数据失败。 |

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

### `doctor`

| 字段 | 说明 |
| --- | --- |
| 用途 | 诊断项目、锁、安装、命令路由、环境变量和 PATH 状态。 |
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

## 稳定协议边界

Pinset v1.0 为项目/全局配置、锁文件、全局状态与安装收据写入 schema 2。同一主版本内允许兼容地增加字段。删除字段或改变字段类型/含义需要新的主版本；未来磁盘格式变更必须保持可读，或提供显式迁移。仅记录 HTTPS 摘要的 v1 前 Node 锁会被拒绝，并提示重新运行 `pinset use`，因为它缺少 v1 OpenPGP 验证证据。
