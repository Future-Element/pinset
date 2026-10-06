# Pinset 3.0 完整命令

八个 Provider：Node、pnpm、Bun、Go、Python、完整 Temurin OpenJDK、Rust、Flutter/Dart。配置 .pinset/config.toml，精确锁 .pinset/lock.toml。

通用参数：-C/--cwd、--lang auto|en|zh-CN、--json、--help、--version。预览仅使用 --plan。十二个顶层：init use remove install list which check exec upgrade env clean self。

### `init`

只在指定目录创建新配置和项目 ID，不自动导入原生版本文件。

```text
pinset init
```


### `use`

明确选择并解析官方制品，事务更新配置和精确锁；禁止重复工具。--no-install 只提交选择和锁。

```text
pinset use <tool@selector>... [--global] [--no-install] [--plan]
pinset use node@24 pnpm@10 python@3.14 java@21
```


### `remove`

独立取消工具选择和锁记录，保留 SDK 与 venv；SDK 删除交给 clean。

```text
pinset remove <tool>... [--global] [--plan]
```


### `install`

严格按现有锁安装，不解析新版本。离线缓存仍校验摘要；repair 不接管外部安装。只有有效 Pinset 环境可明确重建。

```text
pinset install [tool...] [--global] [--offline] [--repair]
               [--recreate-venv] [--plan]
```


### `list`

查看选择与安装记录；--remote 查询官方可用版本。

```text
pinset list [tool]
pinset list <tool> --remote
```


### `which`

显示选定入口、精确版本、安装身份与 SDK/JDK 根路径；--explain 给出绑定依据。JDK 不提供的工具明确报错，不能回退到系统 JDK。

```text
pinset which [command] [--global] [--explain]
pinset which javac --explain
```


### `check`

默认只读，不解密、运行工具、联网或写盘。--deep 检查安装；--probe 明确运行有超时的探针并记录实际路径、版本、宿主和时间。Java 检查独立工作；target 要求项目上下文。配置、安装、绑定、实际探针与用户验证结果分别报告。

```text
pinset check [--global] [--deep] [--probe]
             [--target android|ios|windows|macos|linux|web]
```


### `exec`

执行原始命令，保留输出和退出码，不接受 --json，不自动安装。Maven/Gradle 使用项目 wrapper。原生联网与显式外部 toolchain 保留，执行入口不是沙箱。

```text
pinset exec [--profile <profile>|--no-env] -- <command...>
pinset exec -- ./mvnw verify
pinset exec -- ./gradlew build
```


### `upgrade prepare`

准备单项目候选。省略工具处理全部选择；仅工具名沿用 selector，精确选择不自动跨版本，无变化不建立候选。快照上限 20,000 文件、单文件 16 MiB、总计 256 MiB、深度 64、附加路径 64；排除 Git、local、venv、应用依赖和明文环境文件，拒绝越界与不安全链接。

```text
pinset upgrade prepare [tool|tool@selector...] [--plan]
```


### `upgrade test`

在独立基线与候选副本中执行原始命令，由命令准备应用依赖，不引入 task。默认超时 300 秒，在 verification.timeout 调整。最近一次结果为准；秘密和外部状态标为有限证据。保留原生输出，不接受 --json。

```text
pinset upgrade test <id> [--compare]
                     [--profile <profile>|--no-env] -- <command...>
```


### `upgrade status`

查看候选；--history 查看已完成的升级历史。

```text
pinset upgrade status [id]
pinset upgrade status --history
```


### `upgrade apply`

只应用指纹新鲜且最近验证成功的候选。--allow-limited 仅接受证据边界，不能覆盖失败、超时或过期。配置、锁、本地绑定和历史均纳入事务完成条件。

```text
pinset upgrade apply <id> [--allow-limited] [--plan]
```


### `upgrade restore`

恢复已完成升级的工具链状态，不回滚源码、应用包、数据库或 IDE 进程；不复制候选 venv。

```text
pinset upgrade restore <history-id> [--plan]
```


### `upgrade recover`

处理中断日志。在事务未完成时，读取和执行被阻止，恢复原配置、锁和本地绑定后解除标记。

```text
pinset upgrade recover [--plan]
```


### `env init`

在 .pinset/env/ 创建 age 按值加密的 profile。私钥仅使用系统凭据库或显式 PINSET_IDENTITY，不创建私钥文件。

```text
pinset env init <profile>
```


### `env use`

默认更改本地选择；--project 修改共享默认，--reset 清除选择。优先级：参数、进程环境、本地、共享；显式禁用优先。

```text
pinset env use <profile> [--project]
pinset env use --reset [--project]
```


### `env remove`

取消 profile 和默认选择，--plan 预览。

```text
pinset env remove <profile> [--plan]
```


### `env list`

查看 profile 或公开变量名，不输出解密值。

```text
pinset env list [--profile <profile>]
```


### `env set`

值来自隐藏交互或 stdin；禁止覆盖工具链与 Pinset 控制变量，执行时变量冲突报错。

```text
pinset env set <name> --profile <profile> [--stdin]
```


### `env unset`

删除指定加密值，其他值保持加密。

```text
pinset env unset <name> --profile <profile>
```


### `env access request`

请求文件仅包含公开信息。常规身份保存在凭据库；--ci 只在 TTY 一次显示私钥供人工转存平台 Secret，不能使用 --json，不写私钥文件。

```text
pinset env access request [--new]
pinset env access request --ci
```


### `env access grant`

按公开请求重新加密指定 profile 并授权。

```text
pinset env access grant <request-file> --profile <profile>
```


### `env access revoke`

移除请求对应的接收者并重新加密；不能撤销对方已读取或复制的秘密。

```text
pinset env access revoke <request-id> --profile <profile>
```


### `env access list`

查看指定 profile 的公开授权记录。

```text
pinset env access list --profile <profile>
```


### `env trust add`

明确信任当前项目。信任绑定项目 ID、目录身份和配置/profile 指纹，外来变更使信任失效。

```text
pinset env trust add
```


### `env trust status`

只读检查当前信任是否仍有效。

```text
pinset env trust status
```


### `env trust revoke`

撤销本地项目信任。

```text
pinset env trust revoke
```


### `clean cache`

保守清理不再引用的缓存；保护登记项目、全局、venv、候选、历史和恢复日志，不确定对象保留。

```text
pinset clean cache [--plan]
```


### `clean installs`

清理未引用 SDK，只接受 tool@exact 筛选；remove 不负责 SDK 删除。

```text
pinset clean installs [tool@exact...] [--plan]
```


### `clean history`

只清理满足时间与保留策略的历史，duration 使用 s/m/h/d，如 30d。

```text
pinset clean history --older-than <duration> [--plan]
```


### `self info`

查看 Pinset 3 版本、平台、路径和八个 Provider。

```text
pinset self info
```


### `self shell`

输出接入片段，不写用户 profile。

```text
pinset self shell <bash|zsh|fish|powershell>
```


### `self completions`

输出指定 shell 的补全。

```text
pinset self completions <bash|zsh|fish|powershell>
```


### `self repair`

修复受管入口与相邻同版本 shim，不接管外部命令。

```text
pinset self repair
```


### `self update`

从官方 Pinset 3 发布安装对应平台制品，校验 checksum；--plan 只预览。

```text
pinset self update [version] [--plan]
```


选择器使用 `latest`、Java/Node 的 `lts`、数字版本和精确构建。Rust 还支持 `stable` 和 `nightly-YYYY-MM-DD`，不选择未固定日期的 nightly。旧 `current` 选择器拒绝解析。
