# 管理自定义 CLI 工具

内置 Provider 之外的 CLI 也可以由 Pinset 管理。提供一个确切版本、公开 HTTPS 下载规则、包格式和可执行文件路径，就能使用相同的 `use`、`install`、`which` 和命令入口。

[English](generic-tools.md) · [命令参考](commands.zh-CN.md)

## 从 Terraform 开始

新项目先执行 `pinset init`，已有项目直接编辑 `.pinset/config.toml`，添加下载定义：

```toml
[generic.terraform]
url = "https://releases.hashicorp.com/terraform/{version}/terraform_{version}_{os}_{arch}.zip"
format = "zip"
executable = "terraform{exe}"
os = { macos = "darwin" }
arch = { x86_64 = "amd64", aarch64 = "arm64" }
```

然后预览并选择版本：

```sh
pinset use terraform@1.9.0 --plan
pinset use terraform@1.9.0
pinset which terraform --explain
pinset exec -- terraform version
```

`use` 会将选择记录在 `[tools]` 中，最终配置包含下面两部分；已有 `[tools]` 表时向其中增加字段，不要重复表头：

```toml
[tools]
terraform = "1.9.0"

[generic.terraform]
url = "https://releases.hashicorp.com/terraform/{version}/terraform_{version}_{os}_{arch}.zip"
format = "zip"
executable = "terraform{exe}"
os = { macos = "darwin" }
arch = { x86_64 = "amd64", aarch64 = "arm64" }
```

`1.9.0` 是演示用的明确版本，不代表推荐的最新版本。首次选择时下载产物并计算真实 SHA-256；完成后提交配置和锁文件。其他人运行 `pinset install` 即可按同一锁还原。需要只生成锁、稍后安装时，用 `pinset use terraform@1.9.0 --no-install`。

## 配置字段与变量

| 字段 | 含义 |
| --- | --- |
| `[tools]` 中的工具名 | 对外命令名及版本选择，值仍为字符串。 |
| `[generic.<工具名>].url` | 公开 HTTPS 下载地址模板。 |
| `format` | `tar.gz`、`zip` 或 `binary`，不能省略。 |
| `executable` | 解压后包内的主可执行文件相对路径；裸二进制按此路径保存。 |
| `os` | 可选，替换发布方操作系统命名。 |
| `arch` | 可选，替换发布方架构命名。 |

`url` 与 `executable` 均支持这些变量：

| 变量 | 默认值或含义 |
| --- | --- |
| `{version}` | 明确选择的版本或 tag，原样替换。 |
| `{os}` | `linux`、`macos`、`windows`，可由 `os` 映射。 |
| `{arch}` | `x86_64`、`aarch64`，可由 `arch` 映射。 |
| `{target}` | Pinset 平台名，如 `linux-x86_64`，不受映射修改。 |
| `{exe}` | Windows 为 `.exe`，其他平台为空。 |

工具名须为可移植的小写 ASCII 名称，使用字母、数字、`-`、`_`，且以字母或数字开头。不能占用内置工具及其命令、`pinset`、`pinset-shim` 和系统保留名。对外命令名可以与包内文件名不同。

路径使用 `/` 分隔，不能为绝对路径，也不能含 `..` 等不安全部分。未知模板变量会报错。需要多个平台时，在配置顶层设置 `platforms`，再执行 `use` 生成各平台产物；发布方缺少所需平台产物时会失败。

## 锁与安装如何配合

`use` 替换各平台模板、下载文件、计算摘要，将实际 URL、SHA-256、格式和入口路径写入 `.pinset/lock.toml`。正常选择会复用下载缓存；`--plan` 仍可能下载到临时文件以计算真实摘要，但不写项目或 Pinset 托管状态。

`install` 只读取锁中的地址、摘要、格式和入口路径，不重新解析版本、不重新替换模板。修改合法下载规则不会改变旧锁的安装行为；下一次明确执行 `use` 才更新对应记录。版本选择变化必须重新生成匹配锁。缓存命中和重新下载都检查摘要：

```sh
pinset install terraform --offline
pinset check --deep --probe --format json
```

离线安装要求本地已有有效安装或完整缓存，不能凭空取得产物。检查中的收据摘要匹配和安装内容指纹检查分别有明确字段，见 [CI 指南](ci.zh-CN.md#json-检查契约)。

首次 HTTPS 下载得到的内容决定锁定摘要，保证后续使用相同内容，不等于验证发布方签名。通用工具的校验强度为 checksum，不能满足 signed-checksum 要求；没有可用发布时间时，也不能满足依赖该时间的最短发布年龄策略。项目策略不会因工具是通用类型而被跳过。

## 命令入口和范围

shell 接入后可直接运行 `terraform version`；也可以一直使用 `pinset exec -- terraform version`。路由、项目优先级、全局默认和严格项目策略沿用内置工具规则。

全局工具的下载定义要写在全局配置中，再执行 `pinset use terraform@1.9.0 --global`。项目 `remove terraform` 只取消选择，`clean installs` 再清理无引用安装。已声明的通用工具也能由 `import` 迁入明确版本，但 `list --remote` 不为它发现版本。

## 首版支持范围

支持公开 HTTPS 上的预编译原生程序，包括 GitHub Releases 和保持 HTTPS 的 CDN 重定向。安装检查入口的操作系统和架构，不执行包内内容来完成安装。

不支持源码构建、脚本安装、安装钩子、私有仓库认证、浮动版本或范围发现。拒绝 HTTP 降级、URL 用户凭据和 fragment、本地地址、不安全路径，以及作为主入口的脚本。需要这些能力的工具不能用通用 Provider 安装。

遇到 `PINSET_GENERIC_INVALID`，先检查名称、URL、模板、格式、相对路径和明确版本；下载失败检查生成 URL 是否有目标平台产物。摘要失败时停止安装，核对上游文件与已审阅锁，不要删除摘要来绕过检查。

