# Pinset v0.1 产品规格

状态：方案候选，关键技术验证后冻结  
更新日期：2026-07-28

## 1. 支持矩阵

### 1.1 认证平台

| 操作系统 | Pinset CLI | Node.js | CPython | Flutter |
| --- | --- | --- | --- | --- |
| Windows x64 | 必须 | 必须 | 必须 | 必须 |
| macOS arm64 | 必须 | 必须 | 必须 | 必须 |
| macOS x64 | 必须 | 必须 | 必须 | 必须 |
| Linux x64 (glibc) | 必须 | 必须 | 必须 | 必须 |

Windows arm64、Linux arm64/musl 可在后续支持 Pinset CLI 和部分运行时，但不进入 v0.1 的完整认证承诺。WSL 按独立 Linux 环境处理，不与 Windows 原生安装目录混用。

### 1.2 运行时范围

- Node.js：官方预编译版本；精确版本、major/minor 选择器、`lts`、`current`。
- CPython：3.10–3.14 常规 python-build-standalone 产物；精确版本与 major/minor 选择器。
- Flutter：stable channel 的精确版本与 `stable` 别名；Dart 随 Flutter 一起管理。

别名只在生成/更新锁文件时解析。`--locked` 安装只接受锁定的精确产物。

## 2. 核心文件

### 2.1 `pinset.toml`

项目配置应提交到版本库，只包含数据：

```toml
schema = 1

[tools]
node = "24"
python = "3.13"
flutter = "3.44.8"
```

禁止在配置中定义 Shell 命令、安装后脚本、任务、动态表达式或远程 include。

### 2.2 `pinset.lock`

锁文件应提交到版本库。概念结构：

```toml
schema = 1
generated_by = "pinset 0.1.0"

[[tool]]
name = "node"
requested = "24"
version = "24.x.y"
provider = "nodejs-official"

[[tool.artifact]]
target = "windows-x86_64"
canonical_url = "https://nodejs.org/dist/..."
artifact_path = "v24.x.y/node-v24.x.y-win-x64.zip"
sha256 = "..."
verification = "signed-shasums"
```

最终 schema 必须满足：

- 同一输入稳定序列化；
- 未知字段和 schema 升级有明确策略；
- 每个平台产物可以独立更新；
- URL、哈希、来源和验证方法不可缺省；
- `pinset install --locked` 不修改该文件。

锁文件不记录当前机器选择的镜像别名。中国、海外和企业内网用户使用同一 lock，只有本机传输来源不同。安装收据额外记录本次实际使用的源，便于审计。

### 2.3 本机安装源配置

源配置保存在 Pinset 本机配置目录，不允许项目 `pinset.toml` 定义任意 URL：

```toml
schema = 1

[providers.node]
active = "npmmirror"
fallback = ["official"]

[providers.node.sources.npmmirror]
base_url = "https://npmmirror.com/mirrors/node/"
```

`official` 是每个 provider 的内置源，不写入自定义 source 表，也不能被覆盖或删除。自定义源必须由用户显式添加；项目最多引用已经在本机批准的别名，不能通过克隆仓库静默改变下载主机。自定义源默认只接受 HTTPS；仅在用户显式传入 `--allow-insecure` 时允许 HTTP，面向受信任的局域网代理。

## 3. 命令契约

### `pinset init`

- 在当前目录创建最小 `pinset.toml`；
- 已存在时不覆盖；
- 可检测旧版本文件并展示建议，但不自动导入。

### `pinset use <tool>@<selector> [--global]`

- 默认更新当前项目配置；
- 解析精确版本，更新锁文件并安装缺失产物；
- `--global` 更新 Pinset 全局选择，不修改项目文件；
- 所有文件写入必须原子化。

### `pinset install [<tool>@<selector>] [--locked] [--source <alias>]`

- 无参数：根据当前项目配置/锁文件安装；
- 指定工具：只安装该选择器，不必改变项目选择；
- `--locked`：配置与锁不一致或目标产物缺失时失败；
- `--source`：仅本次安装覆盖本机活动源，不修改项目配置或锁文件；
- 已验证且完整的安装应幂等返回成功。

### `pinset list [tool] [--available]`

- 默认只展示本机安装；
- `--available` 允许访问网络并展示 provider 可用版本；
- 明确区分 installed、selected、cached、system。

### `pinset current [tool]`

- 显示每个工具的请求选择器、精确版本、配置来源和实际安装路径；
- 未声明时说明回退来源，不用空白或模糊的 “system” 掩盖细节。

### `pinset which <command> [--sdk]`

- 显示 shim 最终会执行的真实文件；
- `--sdk` 对 Flutter 等工具返回 SDK 根目录，供 IDE/脚本使用；
- 检测到 PATH 中另一个工具遮蔽 Pinset 时给出诊断。

### `pinset exec [<tool>@<selector>] -- <command...>`

- 在当前进程的子进程环境中显式使用指定或项目版本；
- 不修改父 Shell；
- 适用于 CI、一次性验证和不安装 shim 的场景。

### `pinset doctor [--json]`

- 检查 PATH 顺序、shim 可达性、配置冲突、安装完整性、旧管理器和常见 IDE 路径；
- 默认只读；
- 输出精确、可逆的建议命令；
- `--json` 提供稳定机器可读结构。

### `pinset import [--dry-run]`

- 读取兼容旧文件，生成候选配置；
- 多个来源冲突时停止并解释；
- 默认建议先使用 `--dry-run`；
- 不删除或改写旧文件。

### `pinset source`

当前 spike 已实现：

```shell
pinset source list [provider]
pinset source add <provider> <alias> --base-url <url>
pinset source use <provider> <alias>
pinset source fallback <provider> <aliases...>
pinset source remove <provider> <alias>
```

仍在规划：

```shell
pinset source test <provider> [alias]
```

已实现约束：

- 输出内置 `official` 与用户添加的 `custom` 分类；`community` preset 尚未冻结；
- 添加、切换和删除只修改本机配置；
- 配置通过同目录临时文件原子替换；
- URL 不允许嵌入凭据、查询参数或 fragment；
- 不自动根据 IP、系统地区或语言选择第三方镜像；
- 删除活动源前必须先切换；
- 企业 Nexus/Artifactory 等代理与公共国内镜像使用同一抽象。

规划中的 `test` 应展示 DNS/TLS/HTTP、延迟、目标路径可达性和校验能力，而不只给“快/慢”结论；它不得修改活动源或安装运行时。

### `pinset uninstall <tool>@<exact-version>`

- 只接受 Pinset 自己登记的精确安装；
- 被项目或全局配置引用时默认拒绝；
- 不能删除 Pinset 数据根之外的路径；
- 缓存清理由独立显式命令处理，避免语义混淆。

## 4. 解析规则

1. 显式 `exec` 选择；
2. 最近祖先目录中的 `pinset.toml`；
3. 兼容旧文件（仅当 Pinset 未声明该工具，且用户未关闭兼容读取）；
4. Pinset 全局选择；
5. 系统 PATH 透传（需通过递归与安全原型验证）。

解析要求：

- 不跟随不受控的配置循环；
- 对路径规范化、符号链接和 Windows 大小写保持一致语义；
- 单次命令解析期间使用同一配置快照；
- 命令执行不访问网络；
- 错误必须指出查找过的配置和候选版本。

## 5. Shim 命令集合

- Node.js：`node`、`npm`、`npx`、`corepack`
- CPython：`python`、`python3`、`pip`、`pip3`；版本化命令需根据实际归档生成
- Flutter：`flutter`、`dart`

这些附带命令仅路由到运行时随附工具，不代表 Pinset 提供包依赖管理功能。

## 6. 关键用户流程

### 新项目

```shell
pinset init
pinset use node@24
pinset use python@3.13
pinset use flutter@stable
git add pinset.toml pinset.lock
```

### 克隆项目

```shell
git clone <repo>
cd <repo>
pinset install --locked
pinset current
```

### 旧环境迁移

```shell
pinset doctor
pinset import --dry-run
pinset import
pinset install
pinset doctor
```

旧管理器仍保留，除非用户在 Pinset 之外显式卸载。

### CI

```shell
pinset install --locked
pinset exec -- node --version
pinset exec -- python --version
pinset exec -- flutter --version
```

## 7. 错误与输出

- 人类输出默认简洁，错误包含“发生了什么、使用了什么来源、下一步怎么做”；
- 核心查询和诊断命令提供 `--json`；
- 日志不得输出认证令牌、代理凭据或完整敏感 URL 查询参数；
- 建议的退出码分类：
  - `0` 成功；
  - `2` 使用方式/配置错误；
  - `3` 版本无法解析；
  - `4` 网络或上游错误；
  - `5` 校验/供应链错误；
  - `6` 本地文件系统/权限错误；
  - `7` 锁文件不一致；
  - `8` 冲突或被其他管理器遮蔽。

退出码需在 CLI 原型后冻结，自动化不能依赖未冻结值。

## 8. 设置与隐私

- 无账号；
- 默认不收集遥测；
- 网络仅用于用户触发的版本解析、下载和更新检查；
- 支持显式离线模式；
- 支持系统代理、自定义 CA 和 provider 镜像；
- 源按用户指定顺序回退：仅网络错误、超时和不可用状态可以继续；
- 哈希、签名或 attestation 失败是安全错误，必须立即停止且不得自动换源；
- 所有配置、缓存和安装位置可用 `PINSET_HOME` 覆盖；
- `doctor` 的诊断包必须先脱敏并由用户明确导出。

## 9. v0.1 验收条件

- 认证矩阵端到端测试通过；
- 三种运行时的下载与校验链路有失败测试；
- 配置与锁文件 schema 有兼容性测试；
- shim 在嵌套目录、缺失配置、冲突 PATH、并发执行下行为确定；
- 安装中断后没有可选择的部分安装；
- `doctor` 至少覆盖 fnm、nvm/nvm-windows、uv、FVM、mise、asdf、vfox；
- Flutter 在 VS Code 或 Android Studio 至少有一个经过验证的显式 SDK 路径流程；
- 文档清楚区分 Pinset 管理的运行时与外部依赖管理器。
