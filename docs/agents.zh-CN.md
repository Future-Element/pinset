# 配合 AI 编程 Agent 使用 Pinset

让编程 Agent 通过稳定 JSON 读取工具选择、按确切锁安装并验证实际工具链。Agent 根据字段和退出码决定下一步，工具安装或检查失败时立即停止。

[English](agents.md) · [完整命令](commands.zh-CN.md) · [CI 与检查报告](ci.zh-CN.md)

## 一次迁移的完整流程

先声明这次任务必须具备的工具，例如 `node`，然后逐条执行并独立解析 stdout：

```sh
pinset -C /path/to/project import --plan --json
pinset -C /path/to/project import --yes --json
pinset -C /path/to/project install --json
pinset -C /path/to/project check --deep --probe --json
```

`import --plan` 可联网解析版本元数据，展示计划而不写入项目或安装工具。`import --json` 不带 `--yes` 时也只输出计划。应用计划须显式使用 `--yes`；该命令会重新生成并应用本次计划，因此应再次核对确切版本和跳过项。

| 步骤 | 继续前检查 |
| --- | --- |
| 计划 | `imports` 与 `retained` 的并集包含所有预期工具，记录每项 `version`；检查 `unsupported`、`conflicts`、`unresolved`。 |
| 应用 | `plan` 为 `false`；再次检查工具、确切版本及跳过项。`applied: false` 可能只是已经没有需要写入的变更。 |
| 安装 | `installed` 为 `true`；`tools` 包含预期工具；`installed_tools` 给出各工具的确切 `version`、`provider`、`target` 和 `install_identity`。 |
| 检查 | `passed` 为 `true`，`report.checks` 包含全部预期工具，每项 `passed` 为 `true`；使用 `--probe` 时还核对 `actual_version`。 |

跳过项需要 Agent 停下来报告，或根据事先获得的授权逐项处理。`import` 可以报告跳过项并返回 `0`；空工具集合的检查也可以返回 `passed: true`。因此，“预期工具均存在”是验收条件，空数组不能作为工具链已准备好的证据。

已有 `.pinset/lock.toml` 的项目通常从 `install --json` 开始。`install` 直接使用锁里的确切产物和摘要，不重新选择版本。普通的 `which`、`list`、`check` 不会替 Agent 安装缺失工具。

## 解析协议与退出码

管理命令添加 `--json` 后，stdout 为一份 JSON 对象；不要合并 stdout 与 stderr。所有此类成功报告、操作错误及参数错误都含：

```json
{"protocol":"pinset/3","schema_version":1}
```

字段名称、类型和含义保持稳定，只增加字段；消费者必须忽略未知字段。对象键顺序不属于契约。`null` 表示相应值没有观测或不适用，不等于成功、空字符串或 `false`。

| 退出码 | 处理方式 |
| --- | --- |
| `0` | 命令执行成功；仍核对目标工具、跳过项和状态字段。 |
| `1` | 操作或工具检查失败。停止依赖步骤，读取 `error.code` 或完整检查报告。 |
| `2` | 参数错误。根据 `error.code: PINSET_ARGUMENT_INVALID` 修正调用。 |

操作错误包含 `error.code` 和 `error.message`。按稳定的 `code` 分支，`message` 用于向用户解释；不要匹配终端文案。工具检查失败时可能没有 `error`，而是保留完整的 `report.checks` 并给出 `passed: false`。JSON 解析失败或协议不匹配时也应停止。

`import --yes --json` 的 stdout 只有最终结果，stderr 另有一份 JSON 计划。该计划用于审阅，不能拼接到 stdout。其他 JSON 管理命令关闭交互进度输出。

## 常用查询

```sh
pinset list --json
pinset which node --json
pinset which --json
pinset env list --json
pinset env list --profile dev --json
```

| 查询 | 读取的字段 |
| --- | --- |
| `list` | `selected` 是所选锁或 `null`；`installs` 是本机安装清单，按工具、版本、身份、平台、路径排序。 |
| `which node` | 单个路由对象的 `executable`、`sdk`、`version`、`source`、`provider`。 |
| `which` | `commands` 内的路由对象集合。 |
| `env list` | `profiles`、`shared_default`、`local_default`。 |
| `env list --profile dev` | `profile` 和变量名数组 `variables`；不返回变量值。 |

`check` 默认只读，检查配置、绑定和安装收据；通用工具的 `source_verification` 还会区分 `signature-verified` 与 `digest-only`，并离线复验锁内 GitHub 证据。`--deep` 验证安装内容指纹，`--probe` 明确运行版本探针。未运行探针时 `actual_version` 为 `null`，未执行内容检查时 `payload_verified` 为 `null`。这些报告不代表应用构建通过。

## 原始输出和交互例外

`exec` 保留应用原始输出与退出状态，明确拒绝 `--json`，返回结构化 `PINSET_NATIVE_OUTPUT` 错误和退出码 `1`。先通过 JSON 查询和检查准备环境，再单独运行应用命令：

```sh
pinset exec --no-env -- node --version
```

该命令的输出属于子进程；退出码也来自子进程，可能超出 `0/1/2`。环境准备失败仍返回 `1`。

`env access request --ci` 仅在交互终端一次性展示身份私钥，拒绝 `--json`，返回 `PINSET_CI_IDENTITY_TTY`。`env set` 的自动化调用须使用 `--stdin` 提供值，避免密码提示；秘密值不应放在命令参数或日志中。

`self shell --json` 和 `self completions --json` 用 `text` 字段包装生成的脚本。`--help`、`--version` 保留文本显示，不属于 JSON 报告接口；需要机器读取 Pinset 版本时使用 `self info --json`。
