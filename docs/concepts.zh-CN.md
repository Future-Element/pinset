# Pinset 基础概念

Pinset 把“项目要用什么版本”和“这台电脑装了什么”分开管理。维护者选择版本并提交配置和锁，其他人按锁安装，避免每台电脑重新决定工具版本。

[English](concepts.md) · [命令参考](commands.zh-CN.md)

## 从哪条命令开始

| 现在的情况 | 操作 |
| --- | --- |
| 新项目，还没有版本声明 | `pinset init`，再 `pinset use node@24`。 |
| 已有 `.nvmrc`、`.tool-versions` 等文件 | `pinset import --plan` 审阅，再 `pinset import`，最后 `pinset install`。 |
| 克隆了已经使用 Pinset 的项目 | `pinset install`，再 `pinset check`。 |
| 要升级或切回工具版本 | `pinset use tool@selector`。 |
| 需要修复工具安装 | `pinset install tool --repair`。 |
| Pinset 操作意外中断 | 先 `pinset self repair --plan` 查看恢复内容。 |

`use` 默认会安装，`import` 不安装。`install` 不解析新版本，`exec` 不自动安装。

## 配置与锁文件

`.pinset/config.toml` 记录项目 ID、工具选择、目标平台、策略和公开环境授权。下面是工具选择部分，不是完整文件：

```toml
[tools]
node = "24"
pnpm = "10"
python = "3.14"
```

`.pinset/lock.toml` 记录解析出的确切版本、各平台下载地址、摘要和安装所需信息。`node = "24"` 表示选择 Node 24 系列；锁决定这次实际使用哪个补丁版本和哪份产物。

两份文件一起提交。`use` 会更新它们，`import` 确认后生成或补齐它们。其他人运行 `install` 时使用锁中的版本，而不是当天最新版本。手动修改工具选择或 Rust 选项后，明确运行 `use tool@selector` 生成匹配锁。不要把其他项目的锁直接复制过来：项目 ID 必须匹配。

项目本地绑定位于 `.pinset/local`，Python 项目可能有受管 `.venv`；这些不是共享版本选择。Pinset home 保存工具、缓存、事务和本地身份状态，默认根目录是用户目录下的 `.pinset`，v3 状态在其 `v3` 子目录。可以用绝对路径 `PINSET_HOME` 更改根目录；安装目录为 `PINSET_HOME/v3/installs`，命令入口在 `PINSET_HOME/v3/bin`。`self info` 显示实际路径。

## 版本选择器

| 选择器 | 用途 |
| --- | --- |
| 数字前缀，如 `node@24`、`python@3.14` | 解析对应系列的可用稳定版本。 |
| 确切版本，如 `node@24.0.0` | 明确选择一个版本；Java 也支持确切构建。 |
| `latest` | 查询 Provider 支持的最新版本。 |
| `lts` | Node、Java 的长期支持版本。 |
| `stable` | Rust 稳定版。 |
| `nightly-YYYY-MM-DD` | 固定日期的 Rust nightly。 |

不同 Provider 的版本写法不同，用 `list <tool> --remote` 查看官方候选。旧的 `current` 不接受；Rust 不接受未固定日期的 nightly。通用工具只接受明确版本或 tag，不发现 `latest` 或解析范围。

`import` 还会识别来源文件中的 npm 范围、比较符、通配符和 `lts/*` 等声明，先解析为确切版本，在迁移计划中标注。来源导入支持的范围不等于 `use` 对所有 Provider 都接受同样的范围语法。

需要让团队使用多个平台时，在配置的顶层 `platforms` 数组声明目标，再执行 `use` 生成对应产物。示例片段：

```toml
platforms = ["linux-x86_64", "macos-aarch64", "windows-x86_64"]
```

锁中必须有每台电脑或 CI 所需的产物；具体工具未发布的平台会解析失败。项目目标平台与 Pinset CLI 自身的发布平台是两个范围，CLI 支持情况见安装和 [CI 指南](ci.zh-CN.md)。

## 项目与全局范围

通常在项目目录或其子目录执行命令，Pinset 会在项目边界内查找 `.pinset/config.toml`。`init` 是例外：只在指定目录创建。`-C/--cwd` 可指定工作目录，无需先切换 shell 目录：

```sh
pinset -C ./packages/api install
pinset use node@24 --global
```

`--global` 仅用于支持它的命令，操作 Pinset home 中的全局配置和锁。项目明确选择优先，未选择工具可继承全局默认；继承工具及其运行时仍受项目策略约束。项目明确选择损坏、锁不匹配或安装缺失时会失败，不悄悄改用全局或系统版本。

要求所有工具都由项目选择时，在项目配置中设置：

```toml
[policy]
require_project_tools = true
```

其他校验强度、最短发布时间策略同样在安装、路由时生效。共享全局默认不是绕过项目策略的方法。

## 预览、只读与执行

`--plan` 是支持预览的修改命令的统一入口。它不写项目或 Pinset 托管状态，但不保证离线：`use`、`import` 可能解析元数据，通用工具可能临时下载计算真实摘要；`self update` 会读取发布清单。

`list` 本地查看、`which` 和普通 `check` 是只读操作。`list --remote` 会联网；`check --deep` 读取并校验内容；`check --probe` 会明确运行工具，并写入观测证据。`--format json` 和全局 `--json` 改变输出，不会自动启用深度检查或探针。

`exec` 保留原始程序输出和退出码。通过 `self shell` 接入后可以直接运行受管命令；未接入时也可以明确运行 `pinset exec -- node --version`。两种方式都不是沙箱，不会替应用阻止联网或文件写入。

## 环境档案与团队协作

profile 是一组按值加密的环境变量。以开发环境为例：

```sh
pinset env init dev
pinset env set API_TOKEN --profile dev
pinset env use dev
pinset exec -- node app.js
```

输入变量值时终端隐藏输入，项目只保存密文。新协作者先运行 `env access request`，将公开请求交给维护者；维护者 `env access grant` 后提交更新的密文和授权配置。协作者取得变更、审阅并 `env trust add` 后才能使用，且身份必须有解密权限。

环境选择顺序为命令参数、`PINSET_PROFILE`、本地选择、共享默认；`--no-env` 或 `PINSET_NO_ENV` 优先禁用。`env use --reset` 清除该范围选择，不等于禁止其他范围的默认。`env list` 只显示档案或公开变量名。

私钥使用系统凭据库，或通过 `PINSET_IDENTITY` 显式提供，不提交到仓库。CI 身份由交互式 `env access request --ci` 生成并只显示一次，需人工存入平台 Secret。信任只允许在这台电脑使用审阅后的项目，不替代访问授权；撤销访问不能收回已经复制的秘密。

## 阅读输出与错误

`--lang auto|en|zh-CN` 控制终端文字。大多数命令的全局 `--json` 输出结构化结果；`exec` 保留程序输出，因此不接受它。`check --format json` 提供稳定的 [CI 检查契约](ci.zh-CN.md#json-检查契约)，未运行的检查以 `null` 等字段明确表达。

成功一般返回 `0`，操作失败 `1`，参数错误 `2`；`exec` 转发程序状态。`check` 报告失败同样返回 `1`。`env trust status` 查询成功可以返回 `0` 但 `trusted: false`，脚本要读取字段。预览和状态查看的成功不表示应用构建已经通过。

