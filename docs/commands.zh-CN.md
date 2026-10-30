# Pinset 命令参考

先按你要做的事选命令：新项目使用 `init` → `use`；迁移已有版本声明使用 `import` → `install`；克隆已配置的项目后运行 `install` → `check`。

内置工具有 Node、pnpm、Bun、Go、Python、Temurin OpenJDK、Rust 和 Flutter/Dart。额外 CLI 见[自定义工具指南](generic-tools.zh-CN.md)。配置、锁文件、版本选择和范围见[基础概念](concepts.zh-CN.md)，自动还原见[CI 指南](ci.zh-CN.md)。

通用参数：`-C/--cwd <目录>`、`--lang auto|en|zh-CN`、`--json`、`--help`、`--version`。本页省略这些通用参数，可用 `<命令> --help` 查看完整语法。除 `exec` 转发子进程状态外，成功退出码为 `0`，操作失败为 `1`，参数错误为 `2`。示例版本、路径和 ID 用于说明；标为“执行结果”的文字不是终端原样输出。

管理命令的 `--json` 成功报告、操作错误和参数错误统一包含 `protocol: "pinset/3"` 与 `schema_version: 1`。已有字段名称、类型和含义保持稳定，消费者须接受新增字段；stdout 与 stderr 分开解析。import → install → check 流程与原始/交互输出例外见 [AI 编程 Agent 指南](agents.zh-CN.md)。

## 工具管理

### `init`

在当前目录创建 Pinset 项目配置，开始管理这个项目的工具版本。

#### 什么时候用

新项目还没有 `.pinset/config.toml` 时使用。已有 `.nvmrc` 等版本文件、想沿用它们时，使用 `import`，无需先执行 `init`。

#### 示例

```sh
pinset init
```

执行结果：当前目录生成 `.pinset/config.toml`，包含新的项目 ID。下一步运行 `pinset use node@24` 选择并安装工具。

#### 行为细节

只初始化指定目录，不自动导入原生版本文件。不联网、不安装工具。项目未选择的工具可以继承全局默认，项目选择优先；`[policy] require_project_tools = true` 可限制为项目工具。项目损坏或已选工具不可用时仍报错，详见[基础概念](concepts.zh-CN.md)。

#### 参数、退出码与常见错误

无专用参数。成功返回 `0`；已有配置返回 `1`（`PINSET_PROJECT_EXISTS`），直接使用该项目或选择其他目录。

### `import`

把项目已有的版本声明迁入 Pinset，先查看计划，再确认写入配置和锁文件。

#### 什么时候用

项目正在使用 `.tool-versions`、`.nvmrc` 等文件时使用。已有 Pinset 选择只补缺失项；主动更换版本用 `use`。

#### 示例

假设项目根目录的 `.nvmrc` 声明 Node 24：

```sh
pinset import --plan
pinset import
pinset install
```

计划显示声明及来源、确切版本、保留项、冲突和不能导入的工具。第二条确认后写入配置和锁；第三条才安装工具。

#### 行为细节

检查项目根目录的 `.tool-versions`、`package.json`（`engines`、`volta`、`packageManager`）、`.nvmrc`、`.node-version`、`go.mod`、`rust-toolchain.toml`、`.python-version`。已有项目采用发现的项目根目录，新项目使用指定目录。保留来源文件。

范围和别名通过 Provider 元数据解析为确切版本并标注。同一工具的兼容声明共同约束结果，不兼容声明跳过。`go.mod` 优先 `toolchain` 并检查 `go` 最低要求。Rust 同时迁移 profile、components、targets，并保留 nightly 的固定日期；本地路径和未指定的默认 channel 无法推断。npm、pip、独立 Dart 无独立 Provider；pnpm 缺少项目 Node 选择时无法导入。已声明的通用工具只接受明确版本或 tag。

已有选择和有效锁始终保留；缺锁时按已有 selector 补锁。解析可能联网；通用工具预览下载到临时文件计算摘要。`--plan` 不写项目或 Pinset 状态。输入 `y`、`yes` 或“是”才写入，回车取消。审阅期间来源、配置或锁变化会要求重新生成计划。

字段映射、npm 限制、PATH 对照和回退步骤见[从 Volta 迁移项目](migrate-from-volta.zh-CN.md)。`volta.extends` 会被忽略，不会追踪引用。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `--plan` | 只显示计划。 |
| `-y, --yes` | 显示计划后直接确认。 |
| `--json` | 不带 `--yes` 时只输出计划；写入时计划到 stderr，结果到 stdout。 |

成功或取消返回 `0`。无来源返回 `1`（`PINSET_IMPORT_NO_SOURCES`）并列出检查文件；格式错误为 `PINSET_IMPORT_SOURCE_INVALID`；计划失效为 `PINSET_IMPORT_PLAN_STALE`，需重新运行。计划内跳过项需要审阅，不代表整体操作失败。

### `use`

为项目选择或更换工具版本，更新配置和锁文件，并默认安装所选工具。

#### 什么时候用

首次选版本、升级或切回旧版本时使用。`use` 决定版本，`install` 按已经确定的锁还原版本。

#### 示例

```sh
pinset use node@24 pnpm@10
pinset use python@3.14 --plan
```

执行结果：第一条解析确切版本，更新 `.pinset/config.toml` 和 `.pinset/lock.toml`，安装并准备命令入口；第二条只预览。

#### 行为细节

通常联网查询版本和产物，安装时下载或复用已校验缓存。配置和锁通过事务更新；安装、本地绑定写入 Pinset home 和项目本地状态，Python 还可能准备 `.venv`。`--no-install` 仅提交选择和锁。预览可能联网，不写状态。

pnpm 必须具备 Node 选择。修改配置 selector 或 Rust 选项后，明确执行 `use tool@selector` 重新生成锁；安装和执行不自行补锁。同一调用不能重复工具。额外 CLI 见[自定义工具指南](generic-tools.zh-CN.md)。

注意事项：支持 pnpm 10 CJS、11 ESM、12 原生发布包；原生包与共享文件都会落锁并校验。项目声明与锁定 pnpm 或运行时不匹配时明确报错，不自动切换工具链。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `<tool@selector>...` | 一个或多个工具选择，如 `node@24`。 |
| `--global` | 修改全局默认。 |
| `--no-install` | 只更新配置和锁。 |
| `--plan` | 只解析和预览。 |

成功返回 `0`，操作失败返回 `1`。`PINSET_SELECTOR_REQUIRED` 表示缺少选择器；`PINSET_SELECTION_DUPLICATE` 表示重复工具；`PINSET_PLATFORM_UNAVAILABLE` 表示平台无产物。调整选择后重试，不要手改锁绕过校验。

### `install`

按项目锁文件安装工具，让新电脑或 CI 还原已经确定的工具链。

#### 什么时候用

克隆项目后、补装缺失工具或恢复损坏安装时使用。升级或切换版本用 `use`。

#### 示例

```sh
pinset install
pinset install node pnpm --offline
```

执行结果：第一条安装锁中工具并建立绑定；第二条只用本地缓存还原 Node、pnpm 及其锁定依赖，缓存缺失就失败。

#### 行为细节

配置和锁必须有效且匹配。只使用锁中版本、地址、摘要；缓存和下载均校验，不重新解析版本。写入安装、命令入口、本地绑定，并按锁准备受管 Python 环境或 Flutter 链接，不更改工具选择。指定工具时只处理这些工具及其运行时依赖，不处理无关环境或链接。

`--repair` 不接管外部安装。`--recreate-venv` 只重建有效受管环境，且本次选择须包含 Python。`use` 和 `install` 在交互 stderr 展示阶段、下载进度和缓存复用；大小已知时显示百分比、速度和剩余时间。JSON 或重定向 stderr 时隐藏进度界面。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `[tool...]` | 只安装指定工具及依赖；省略则安装项目工具。 |
| `--global` | 按全局锁安装。 |
| `--offline` | 不下载，只用有效安装或已校验缓存。 |
| `--repair` | 修复受管工具安装。 |
| `--recreate-venv` | 明确重建受管 Python 环境。 |
| `--plan` | 只列安装计划，不下载或写入。 |

成功返回 `0`，失败返回 `1`。`PINSET_LOCK_MISSING` 需先由维护者生成锁；`PINSET_LOCK_MISMATCH` 需用 `use` 对齐选择；`PINSET_TOOL_NOT_SELECTED` 表示工具未被选择。外部 `.venv` 会保留，先检查所有权。

### `remove`

取消项目对工具的版本选择，保留已经下载的工具和 Python 环境。

#### 什么时候用

项目不再需要某个工具时使用。释放安装空间用 `clean installs`；其他项目还引用的安装仍会保留。

#### 示例

```sh
pinset remove bun --plan
pinset remove bun
```

执行结果：预览后从配置和锁中移除 Bun 选择，本机 Bun 安装目录保留。

#### 行为细节

事务更新配置和锁，登记项目状态，不联网、不删除 SDK 或 `.venv`。取消项目选择后，允许继承全局默认的项目可能改用全局工具；严格项目策略下该工具不再可用。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `<tool>...` | 取消选择的工具名。 |
| `--global` | 取消全局默认。 |
| `--plan` | 只预览。 |

成功返回 `0`，配置或锁无效等操作错误返回 `1`。重复工具为 `PINSET_SELECTION_DUPLICATE`。保留其他已选工具所需的运行时。

### `list`

查看项目选择的版本、本机已有安装，或查询官方可用版本。

#### 什么时候用

了解选择和磁盘安装情况时使用。找命令入口用 `which`，检查可用性用 `check`。

#### 示例

```sh
pinset list
pinset list node --remote
```

第一条英文终端输出形式示例：

```text
Selected tools (exact lock)
  node  24.0.0  (24)
Installed
  node  24.0.0  linux-x86_64  /example/pinset/v3/installs/...
```

#### 行为细节

普通查看只读，不运行工具或联网。`--remote` 查询内置 Provider 官方元数据，不下载 SDK。通用工具不支持远程发现版本。安装记录不等于实际版本探针通过。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `[tool]` | 筛选一个工具。 |
| `--remote` | 查询远程版本，必须提供工具名。 |

成功返回 `0`，本地状态或网络查询失败返回 `1`；遗漏 `--remote` 所需工具名返回 `2`。网络失败时可先查看本地记录。

### `which`

查看某条命令会运行哪个程序，并按需解释它的版本和选择来源。

#### 什么时候用

排查项目是否用对 Node、Python、Java 等工具时使用。它解释 Pinset 路由；当前 shell 的 PATH 还需使用 shell 自己的命令查找功能确认。

#### 示例

```sh
pinset which node
pinset which javac --explain
```

第一条只输出入口路径，如 `/example/pinset/v3/installs/node/.../bin/node`；第二条展示版本、入口、SDK 路径、Provider 和选择来源。

#### 行为细节

只读、不联网、不安装、不运行工具。省略命令查看全部受管路由。要求有效锁和安装；项目明确选择不可用时不回退系统版本。JDK 不提供的命令明确报错，不回退系统 JDK。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `[command]` | 命令名，如 `node`、`python`、`javac`。 |
| `--global` | 查看全局选择。 |
| `--explain` | 显示路径以外的选择依据等详情。 |

成功返回 `0`，未选择、安装缺失或路由无效返回 `1`。先 `list` 确认选择，再 `install` 补装；裸命令仍不正确时检查 `self shell` 接入。

### `check`

检查工具配置、安装和命令绑定是否一致，并按需验证安装内容与实际版本。

#### 什么时候用

安装后确认工具链、排查版本或生成 CI 报告时使用。默认只读，`--deep` 复核内容，`--probe` 才运行工具并记录证据。

#### 示例

```sh
pinset check
pinset check --deep --probe --format json > pinset-check.json
```

普通检查的英文终端输出形式示例：

```text
Tool  Configured  Installed  Bound  Observed
node  yes  yes  yes  no
Check passed: yes
```

`Observed` 为 `no` 表示没有运行探针，不等于工具失败。第二条生成包含整体和逐工具 `passed` 的报告。

#### 行为细节

默认不解密、不运行工具、不联网、不写盘。`--deep` 计算安装内容指纹与收据比较；`--probe` 执行有超时的版本探针，并在 Pinset home 写入实际路径、版本、宿主和时间证据。Java/Android 额外观测不代表项目构建通过。

JSON `schema_version` 为 `1`；字段名称、类型和含义稳定，只增加字段。每项含 `tool`、`expected_version`、`actual_version`、`artifact_digest`、`passed`。无探针观测时 `actual_version` 为 `null`；无内容检查时 `payload_verified` 为 `null`。收据与锁摘要匹配、安装内容与收据匹配是两种检查，不声称重算原始下载包摘要。详见 [JSON 检查契约](ci.zh-CN.md#json-检查契约)。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `--global` | 检查全局选择。 |
| `--deep` | 复核安装内容。 |
| `--probe` | 运行版本探针并记录证据。 |
| `--format text\|json` | 默认 `text`；全局 `--json` 也输出同一 JSON。 |
| `--target android\|ios\|windows\|macos\|linux\|web` | 增加目标环境检查，要求项目上下文，不能与 `--global` 并用。 |

所有已选工具通过本次要求的检查返回 `0`。失败仍输出完整报告并返回 `1`；操作错误也返回 `1`，JSON 含 `passed: false` 和 `error`；参数错误返回 `2`。按 `detail` 区分缺失安装、内容变化或实际版本不匹配后修复。

### `exec`

在项目选定的工具链和环境变量下运行命令，保留程序原始输出和退出状态。

#### 什么时候用

希望脚本、CI 或当前终端明确使用项目工具链时使用，无需依赖 shell 接入是否生效。Maven、Gradle 优先用项目 wrapper。

#### 示例

```sh
pinset exec -- node --version
pinset exec -- ./mvnw verify
pinset exec -- ./gradlew build
```

第一条输出项目 Node 版本，如 `v24.0.0`；后两条输出和退出状态来自构建本身。

#### 行为细节

不自动安装或改版本。准备锁定工具环境后运行原始命令。选中加密 profile 时要求信任和解密权限，原有进程变量冲突会报错。子进程可以联网、写文件和调用外部 toolchain，执行入口不是沙箱。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `--profile <profile>` | 本次指定环境档案。 |
| `--no-env` | 禁用加密环境，与 `--profile` 互斥。 |
| `-- <command...>` | 原始命令和参数。 |

启动后转发程序退出状态；准备失败为 `1`，参数错误为 `2`。不接受 `--json`（`PINSET_NATIVE_OUTPUT`）。`PINSET_ENV_COLLISION` 表示变量已存在，检查进程环境后重试。

## 环境与密钥

profile 是一组按值加密的环境变量。常用流程：`env init dev` → `env set ... --profile dev` → `env use dev` → `exec`。提交密文和公开授权信息，私钥留在系统凭据库或由 `PINSET_IDENTITY` 提供。已有档案出现外来变更后，先审阅再 `env trust add`。

### `env init`

创建一个加密环境档案，保存开发、测试等环境的变量。

#### 什么时候用

首次管理环境变量或增加另一套环境时使用。新增变量用 `env set`，切换档案用 `env use`。

#### 示例

```sh
pinset env init dev
```

执行结果：创建 `.pinset/env/dev.env` 并登记公开接收者，随后可 `pinset env set API_TOKEN --profile dev`。

#### 行为细节

写密文、配置和事务状态，刷新本地信任；不联网。身份通常使用系统凭据库，不生成私钥文件。已有其他 profile 时要求当前信任有效。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `<profile>` | 档案名称，如 `dev`。 |

成功返回 `0`，失败返回 `1`。`PINSET_PROFILE_EXISTS` 表示名称已存在；信任失效先审阅，再 `env trust add`。

### `env use`

为后续命令选择环境档案，默认只改变本机的项目选择。

#### 什么时候用

切换开发、测试环境时使用。单次指定用 `exec --profile`，删除档案用 `env remove`。

#### 示例

```sh
pinset env use dev
pinset exec -- node app.js
pinset env use --reset
```

执行结果：本地选择 `dev` 后运行应用；`--reset` 清除本地选择，之后仍可能采用共享默认。

#### 行为细节

默认写 `.pinset/local/profile.json`，`--project` 修改共享默认和事务状态。不联网、不解密、不启动应用。优先级：显式参数、`PINSET_PROFILE`、本地、共享；`--no-env` 或 `PINSET_NO_ENV` 优先禁用。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `<profile>` | 已有档案名。 |
| `--reset` | 清除该范围选择，不同时提供名称。 |
| `--project` | 修改共享默认。 |

成功返回 `0`，失败返回 `1`。`PINSET_PROFILE_MISSING` 时先 `env list` 确认名称；名称与 `--reset` 同用返回 `2`。

### `env remove`

删除不再需要的加密环境档案，同时清除指向它的默认选择。

#### 什么时候用

整个环境已停用时使用。移除一个变量用 `env unset`，只切换环境用 `env use`。

#### 示例

```sh
pinset env remove staging --plan
pinset env remove staging
```

执行结果：删除 `.pinset/env/staging.env` 和配置记录，清除匹配的共享、本地默认。

#### 行为细节

要求信任有效；事务更新档案、配置、锁和本地选择，不联网。`--plan` 不删除。当前文件删除不代表 Git 历史中的密文消失。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `<profile>` | 要删除的档案名。 |
| `--plan` | 只预览。 |

成功返回 `0`，失败返回 `1`。档案不存在为 `PINSET_PROFILE_MISSING`，先用 `env list` 确认。

### `env list`

查看项目有哪些环境档案，或查看一个档案包含的变量名。

#### 什么时候用

确认可选环境、默认选择或变量名称时使用。授权记录用 `env access list`。

#### 示例

```sh
pinset env list
pinset env list --profile dev
```

第二条英文终端输出形式示例：

```text
profile: dev
variables: 1
  API_TOKEN
```

#### 行为细节

只读、不联网、不解密值、不输出秘密。无名称时列档案和共享、本地默认；指定名称时显示公开变量名。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `--profile <profile>` | 查看指定档案变量名。 |

成功返回 `0`，读取失败返回 `1`。`PINSET_PROFILE_MISSING` 表示名称不存在。

### `env set`

向加密环境档案写入或更新一个变量，项目文件只保存密文。

#### 什么时候用

新增 API token、数据库地址或轮换变量值时使用。先用 `env init` 创建档案。

#### 示例

```sh
pinset env set API_TOKEN --profile dev
```

终端提示 `API_TOKEN:`，输入隐藏。结果只确认 `profile`、`variable`、`set`，不回显值；可用 `env list --profile dev` 查看名称。

#### 行为细节

读取后重新加密，事务写密文及状态，不联网。密文和公开授权可提交，私钥不能提交。不能覆盖工具链与 Pinset 控制变量；执行时与进程已有变量冲突也报错。

修改要求档案明确记录 `owner_recipients`。旧档案仍可读；维护者需确认公开 owner 和 grant，使 recipients 等于二者并集，再重新信任。配置中不能保存私钥。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `<name>` | 变量名。 |
| `--profile <profile>` | 必填，目标档案。 |
| `--stdin` | 从 stdin 读取，去除末尾换行，最大 1 MiB。 |

成功返回 `0`，失败返回 `1`。`PINSET_ENV_INPUT_LIMIT` 表示输入过大；`PINSET_PROFILE_MISSING` 表示档案不存在。不要把秘密直接写进命令行或日志。

### `env unset`

从加密环境档案删除一个变量，保留其他变量。

#### 什么时候用

变量已停用时使用。改值用 `env set`，删除整个档案用 `env remove`。

#### 示例

```sh
pinset env unset API_TOKEN --profile dev
```

英文输出形式示例：`removed: yes`；变量原本不存在时为 `removed: no`。

#### 行为细节

要求信任、解密身份和明确 owner 授权。事务更新密文和状态，不输出其他变量明文，不联网。旧密文仍可能保留在 Git 历史。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `<name>` | 要删除的变量。 |
| `--profile <profile>` | 必填，目标档案。 |

成功（含变量不存在）返回 `0`，失败返回 `1`。档案不存在为 `PINSET_PROFILE_MISSING`；解密失败需确认当前身份有访问权。

### `env access request`

生成公开访问请求，让维护者可以授权本机或 CI 读取加密环境。

#### 什么时候用

协作者或 CI 首次需要环境变量时使用。请求本身不授予权限，还需维护者 `env access grant`。

#### 示例

```sh
pinset env access request
```

执行结果：输出公开请求及 JSON 文件路径，把文件交给维护者，私钥留在本机。CI 使用交互命令 `pinset env access request --ci`，将仅显示一次的私钥转存平台 Secret。

#### 行为细节

不联网，在 Pinset home 保存公开请求；常规身份使用系统凭据库或显式身份。`--ci` 要求 stdin、stdout 均为 TTY，不接受 `--json`，不写私钥文件。CI 后续可通过 `PINSET_IDENTITY` 提供 Secret 身份。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `--new` | 创建新的常规身份请求。 |
| `--ci` | 创建独立 CI 身份并交互显示一次私钥，与 `--new` 互斥。 |

成功返回 `0`，失败返回 `1`。`PINSET_CI_IDENTITY_TTY` 需改在交互终端执行，不能重定向私钥；互斥参数错误返回 `2`。

### `env access grant`

根据公开请求授权协作者或 CI 读取指定环境档案。

#### 什么时候用

维护者审阅请求 ID 和公钥后使用。查看和撤销授权分别用 `env access list`、`env access revoke`。

#### 示例

```sh
pinset env access grant ./alice-request.json --profile dev
```

执行结果：登记授权并为有效接收者重新加密 `dev`，提交配置和密文供协作者取得。

#### 行为细节

要求当前身份能解密、信任有效、owner 明确确认。事务更新配置和密文，不联网。重复授权不写入；同 ID 改公钥报错。有效接收者为 owner 与有效 grant 的并集。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `<request-file>` | 请求命令生成的公开 JSON。 |
| `--profile <profile>` | 必填，目标档案。 |

成功或重复授权返回 `0`，失败返回 `1`。`PINSET_PROTOCOL_UNSUPPORTED` 表示非 Pinset 3 请求；ID、公钥或 owner 配置错误需先核对来源和授权。

### `env access revoke`

撤销一条环境访问授权，并为剩余接收者重新加密档案。

#### 什么时候用

协作者退出或 CI 身份轮换时使用，先从 `env access list` 找到 ID。

#### 示例

```sh
pinset env access revoke <request-id> --profile dev
```

将占位符换为真实 ID。执行结果：删除授权并更新密文，提交新的配置和档案。

#### 行为细节

要求信任和解密权限，事务更新配置、密文和状态，不联网。同公钥仍属于 owner 或其他有效授权时保留访问。不能收回已读取或复制的秘密，必要时同时在原服务轮换秘密值。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `<request-id>` | 要撤销的请求 ID。 |
| `--profile <profile>` | 必填，目标档案。 |

成功返回 `0`，档案、授权或解密错误返回 `1`。先查看授权列表，确认 ID 和目标档案。

### `env access list`

查看一个环境档案登记的公开访问授权。

#### 什么时候用

核对授权或查找要撤销的 ID 时使用。变量名用 `env list --profile`。

#### 示例

```sh
pinset env access list --profile dev --json
```

无额外授权时的 JSON 形式示例：

```json
{"protocol":"pinset/3","grants":{}}
```

#### 行为细节

只读公开 grants，不联网、不解密、不显示私钥。owner 与额外 grant 分开记录，空 grants 不代表 owner 无访问权。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `--profile <profile>` | 必填，目标档案。 |

成功返回 `0`，读取失败返回 `1`；`PINSET_PROFILE_MISSING` 表示档案不存在。

### `env trust add`

在本机信任已审阅的项目配置和加密档案，允许后续解密及编辑。

#### 什么时候用

首次使用项目环境或拉取档案变更后使用。先核对配置、公开接收者及档案变更，再信任。

#### 示例

```sh
pinset env trust add
pinset exec --profile dev -- node app.js
```

第一条英文输出形式示例：`trusted: yes`。第二条仍需本机身份已获授权；信任不授予解密权限。

#### 行为细节

信任绑定项目 ID、目录身份、配置和档案指纹，写本地信任记录，不改共享配置、不联网。外来变更会使旧信任失效。

#### 参数、退出码与常见错误

无专用参数。成功返回 `0`，项目或档案读取失败返回 `1`。先切到项目目录，不要直接重新信任尚未审阅的变更。

### `env trust status`

查看本机对当前项目的信任是否仍然有效。

#### 什么时候用

解密前或拉取更新后确认状态时使用。建立信任用 `env trust add`。

#### 示例

```sh
pinset env trust status
```

有效时英文输出形式为 `trusted: yes`；无效时为 `trusted: no`，并显示 `detail` 原因。

#### 行为细节

只读记录和当前指纹，不联网、不解密、不写信任。查询成功不代表已信任，脚本应检查 JSON 的 `trusted` 字段。

#### 参数、退出码与常见错误

无专用参数。查询成功返回 `0`，包括 `trusted: false`；文件读取失败返回 `1`。信任失效需先审阅变更，再决定是否重新信任。

### `env trust revoke`

撤销本机对当前项目的信任，停止后续使用其加密环境。

#### 什么时候用

暂停信任项目或排查环境注入时使用。撤销他人访问权限用 `env access revoke`。

#### 示例

```sh
pinset env trust revoke
```

英文输出形式示例：`revoked: yes`；原本没有记录时为 `revoked: no`。

#### 行为细节

删除本地信任记录，不改配置、密文或身份，不联网。需要再次使用时重新审阅并执行 `env trust add`。

#### 参数、退出码与常见错误

无专用参数。成功（含原本没有记录）返回 `0`，项目或本地记录读取失败返回 `1`。

## 清理

### `clean cache`

清理不再被项目或恢复记录引用的下载缓存，释放磁盘空间。

#### 什么时候用

下载缓存占用过多时使用，不删除已安装工具；旧工具用 `clean installs`。

#### 示例

```sh
pinset clean cache --plan
pinset clean cache
```

执行结果：先列出可删和保留对象，再删除无引用缓存。预览的 `removed` 是计划删除路径，尚未实际删除。

#### 行为细节

保护已登记项目、全局锁、venv 和未完成事务的引用，不确定对象保留。不联网、不改选择。仅删除受管缓存内安全对象；仍有引用的缓存留给离线还原。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `--plan` | 只列候选和保留项。 |

成功返回 `0`，失败返回 `1`。`PINSET_CLEAN_UNSAFE` 需检查路径；`uncertain_references: true` 表示保守保留，不是清理失败。

### `clean installs`

删除不再被项目、全局选择或受管 Python 环境引用的工具安装。

#### 什么时候用

升级后释放旧版本空间时使用。仍被选择的工具先 `remove`；其他项目引用的安装仍保留。

#### 示例

```sh
pinset clean installs node@22.0.0 --plan
pinset clean installs node@22.0.0
```

执行结果：只删除匹配筛选、安装记录有效且无引用的目录。没有可删对象也可成功。

#### 行为细节

不联网、不改配置或锁。保护登记项目、全局、受管 venv 和恢复记录。外部、无所有权证明或引用不确定的对象保留。省略筛选检查全部受管安装。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `[tool@exact...]` | 仅检查这些确切版本，不接受 `latest`、`lts` 等浮动选择。 |
| `--plan` | 只预览。 |

成功返回 `0`，失败返回 `1`。`PINSET_EXACT_REQUIRED` 需用确切版本；`PINSET_CLEAN_UNSAFE` 需检查路径，不要手删被引用目录绕过保护。

## 自维护

### `self info`

查看当前 Pinset 版本、平台和安装路径，方便确认安装或提交问题信息。

#### 什么时候用

确认运行哪个 Pinset、定位 home 或检查平台时使用，项目工具用 `list`。

#### 示例

```sh
pinset self info
```

输出含 `version: 3.1.0`、`platform`、`home`、`cli` 和八个内置 Provider，路径和平台因本机而异。

#### 行为细节

只读、不联网、不运行工具、不改配置。自定义工具不会改变内置 Provider 名单。

#### 参数、退出码与常见错误

无专用参数。成功返回 `0`，基础环境无效返回 `1`。CLI 与入口版本不匹配时检查 `self repair` 或重装匹配的一对程序。

### `self shell`

只打印 PATH 片段，不执行、不修改 shell profile、不持久化 PATH。默认安装也遵循此规则。当前没有 `--write` 参数或写入 profile 的确认向导。

#### 什么时候用

安装后直接运行 `node` 等仍未进入 Pinset 时使用。输出片段需在当前 shell 执行，或自行加入启动配置。

#### 示例

```sh
pinset self shell bash
eval "$(pinset self shell bash)"
```

第一条输出形式示例：

```sh
export PATH='/example/pinset/v3/bin':"$PATH"
```

PowerShell 当前会话使用 `pinset self shell powershell | Invoke-Expression`。

#### 行为细节

默认目录下，检查输出后可在 Bash/Zsh 自行执行 `export PATH="$HOME/.pinset/v3/bin:$PATH"`，Fish 执行 `fish_add_path --path --move --prepend "$HOME/.pinset/v3/bin"`，PowerShell 执行打印的 `$env:PATH` 赋值。Fish 的 `--path` 避免持久化 universal 变量。自定义 `PINSET_HOME` 会改变打印目录；自定义 CLI 安装目录不改变 Provider 入口。

可选持久化需要单独确认并自行编辑：明确目标 profile 文件和内容，先备份，只保留一个带重复保护的 Pinset 区块。回滚时删除该区块并新开 shell。[安装指南](https://github.com/Future-Element/pinset/blob/main/docs/installation.zh-CN.md#确认后可选持久化)列出四种 shell 的文件与幂等片段。[官网](https://pinset.future-element.com) · [主仓](https://github.com/Future-Element/pinset)。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `<bash\|zsh\|fish\|powershell>` | 必填，目标 shell。 |

成功返回 `0`，基础环境错误为 `1`，无效 shell 为 `2`。未生效时检查 PATH 顺序和 shell 命令缓存。

### `self completions`

生成 shell 命令补全脚本，减少输入 Pinset 子命令和参数的时间。

#### 什么时候用

想补全命令和选项时使用。PATH 接入另由 `self shell` 处理。

#### 示例

```sh
pinset self completions bash > pinset.bash
source ./pinset.bash
```

执行结果：由 shell 保存补全脚本，再在当前 Bash 会话启用，不是 Pinset 自动修改配置。

#### 行为细节

只输出脚本，不联网、不自动安装补全、不写用户 profile。按所用 shell 的约定保存并加载。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `<bash\|zsh\|fish\|powershell>` | 必填，目标 shell。 |

成功返回 `0`，基础环境错误为 `1`，无效 shell 为 `2`。不要在其他 shell 中执行生成脚本。

### `self repair`

恢复中断的 Pinset 操作，并修复 Pinset 管理的命令入口。

#### 什么时候用

切换版本、自更新或受管 Python 重建意外中断后使用。单个工具损坏用 `install --repair`，此命令处理事务和入口。

#### 示例

```sh
pinset self repair --plan
pinset self repair
```

执行结果：预览后恢复当前项目、全局和 CLI/shim 中断状态，不撤销已经完成的版本切换。

#### 行为细节

不联网。未完成事务恢复原配置、锁、加密档案和本地绑定；已完成事务只清残留标记。也恢复受管 venv 重建、入口发布和 CLI/shim 更新。符号链接启动时处理真实安装。

venv 创建进程仍运行时不删除目录。Windows 无法原子替换运行中入口时保留原文件并报错；外部或无所有权证明的入口也保留。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `--plan` | 只读预览。 |

成功返回 `0`，不能安全恢复返回 `1`。`PINSET_VENV_BUSY` 需等创建进程退出；`PINSET_BROKER_VERSION` 需匹配的 CLI/shim。外部文件冲突先检查所有权，不强行覆盖。

### `self update`

从官方发布更新 Pinset，或明确安装指定的 Pinset 3 版本。

#### 什么时候用

升级 Pinset 本身时使用。项目工具升级用 `use`；降级需指定确切版本。

#### 示例

```sh
pinset self update --plan
pinset self update 3.1.0
```

执行结果：第一条显示版本、平台包和摘要；第二条校验下载后成对更新 CLI/shim，报告是否需要重启。

#### 行为细节

会联网。省略版本读取官方 `/releases/latest` 重定向，只接受稳定 Pinset 3；指定时可选受支持的预发布。两种方式都不访问 GitHub REST API、不需 token；确定版本后固定同一发布目录。

检查 `SHA256SUMS` 中唯一合法 SHA-256；优先 `pinset-v<version>-<platform>.zip`，兼容固定平台包名。校验 CLI 实际版本后成对替换，启动符号链接解析到真实安装。同版本不重复下载，自动更新拒绝较旧 latest。`--plan` 仅读取重定向和清单，不下载程序、不写盘。

元数据请求有超时和有限重试；较长或 HTTP 日期格式 `Retry-After` 会提示等待，不提前重试。下载仍受网络和 GitHub 流量限制。

#### 参数、退出码与常见错误

| 参数 | 含义 |
| --- | --- |
| `[version]` | 确切 Pinset 3 发布版本；省略查询最新稳定版。 |
| `--plan` | 只预览发布信息。 |

成功（含已是当前版）返回 `0`，失败返回 `1`。拒绝访问、限流、资源或平台缺失、清单错误会分别说明，按消息处理。`PINSET_UPDATE_REPAIR` 可先运行 `self repair --plan` 检查恢复状态。
