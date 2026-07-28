# Pinset Node-first MVP 使用指南

适用版本：`0.1.0-alpha.1`

## 1. 当前能做什么

MVP 只管理 Node.js 精确版本，版本必须写成完整的 `x.y.z`，例如 `24.0.0`。它支持：

| 系统 | 目标标识 | Node 归档 |
| --- | --- | --- |
| Windows x64 | `windows-x86_64` | ZIP |
| Linux x64 (glibc) | `linux-x86_64` | TAR.XZ |
| macOS Intel | `macos-x86_64` | TAR.XZ |
| macOS Apple Silicon | `macos-aarch64` | TAR.XZ |

WSL 是独立的 Linux 环境，不与 Windows 原生安装目录或 PATH 混用。

## 2. 安装 Pinset

在 GitHub Releases 下载当前平台的归档和 `SHA256SUMS`：

- Windows x64：`pinset-windows-x86_64.zip`
- Linux x64：`pinset-linux-x86_64.tar.gz`
- macOS Apple Silicon：`pinset-macos-aarch64.tar.gz`

归档包含两个程序：

- `pinset`：配置、锁定、下载、安装和诊断；
- `pinset-shim`：多调用入口，由 `node`、`npm`、`npx`、`corepack` 等文件名调用。

Linux/macOS 校验示例：

```bash
sha256sum -c SHA256SUMS --ignore-missing
```

Windows PowerShell 校验示例：

```powershell
Get-FileHash .\pinset-windows-x86_64.zip -Algorithm SHA256
Get-Content .\SHA256SUMS
```

比较归档的哈希后，把 `pinset` 放到已在 PATH 中的个人目录。Pinset 不要求管理员权限，也不会自动改写 shell profile。

默认数据目录：

| 系统 | 默认目录 |
| --- | --- |
| Windows | `%LOCALAPPDATA%\Pinset` |
| Linux/macOS | `$XDG_DATA_HOME/pinset`，未设置时为 `$HOME/.local/share/pinset` |

可以用 `PINSET_HOME` 覆盖它。Windows 与 WSL 应各自使用本地文件系统中的独立目录。

## 3. 项目内选择并安装 Node

在项目根目录执行：

```shell
pinset init
pinset use node@24.0.0
```

结果：

1. `pinset init` 原子创建最小 `pinset.toml`，已有文件时拒绝覆盖；
2. `pinset use` 从 Node 官方 HTTPS 发布目录读取 `SHASUMS256.txt`；
3. 为四个 MVP 平台生成确定性的 `pinset.lock`；
4. 将精确版本写入 `pinset.toml`；
5. 下载当前平台的归档、核对锁定哈希、安全解压并提交安装。

建议把这两个文件提交到版本库：

```text
pinset.toml
pinset.lock
```

只锁定、不安装：

```shell
pinset use node@24.0.0 --no-install
```

根据已有锁文件安装：

```shell
pinset install --locked
```

MVP 中 `install` 始终执行锁定安装；`--locked` 用于明确表达 CI 意图。只要 `pinset.toml` 与 `pinset.lock` 不一致，安装会在联网和写入安装目录前失败。

## 4. 使用运行时

不安装 shim 也可以完整使用：

```shell
pinset current
pinset which node
pinset exec -- node --version
pinset exec -- npm --version
pinset exec -- node ./scripts/build.mjs
```

命令从当前目录向上查找最近的 `pinset.toml`。`exec` 会把子进程退出码传回调用者，适合脚本与 CI。

在其他目录检查项目：

```shell
pinset current --cwd /path/to/project
pinset doctor --cwd /path/to/project
```

## 5. 安装 shim

shim 让现有的 `node`、`npm`、`npx`、`corepack` 命令自动使用最近项目配置。它只创建在用户指定目录中，不自动修改 PATH，也不覆盖已有同名文件。

### Windows PowerShell

假设两个 exe 位于当前目录：

```powershell
$shimDir = Join-Path $env:LOCALAPPDATA "Pinset\shims"
pinset shim install --binary .\pinset-shim.exe --dir $shimDir
```

然后把 `%LOCALAPPDATA%\Pinset\shims` 加到用户 PATH，并新开终端。临时验证可以执行：

```powershell
$env:PATH = "$shimDir;$env:PATH"
node --version
pinset doctor
```

### Linux/macOS

```bash
PINSET_DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}/pinset"
pinset shim install --binary ./pinset-shim --dir "$PINSET_DATA_HOME/shims"
export PATH="$PINSET_DATA_HOME/shims:$PATH"
node --version
pinset doctor
```

确认无冲突后，再把 `export PATH=...` 写入你使用的 shell profile。`doctor` 会显示 shim 目录是否在 PATH 中，并列出 PATH 上可见的 Node 候选。

## 6. 切换国内或企业镜像

自定义源必须保持 Node 官方发布目录的相对路径布局，例如 Pinset 会把：

```text
v24.0.0/node-v24.0.0-linux-x64.tar.xz
```

追加到你配置的 `base-url`。配置示例：

```shell
pinset source add node cn-mirror --base-url https://mirror.example/node/
pinset source use node cn-mirror
pinset source fallback node official
pinset source list node
```

截至 2026-07-28，npmmirror 的 Node 目录仍兼容该相对路径结构；如果你接受这个第三方传输来源，可以配置：

```shell
pinset source add node npmmirror --base-url https://npmmirror.com/mirrors/node/
pinset source use node npmmirror
pinset source fallback node official
```

Pinset 不内置或默认启用社区镜像。第三方服务的可用性和运营方可能变化，团队应自行评估并可改用企业代理。

恢复官方源：

```shell
pinset source use node official
pinset source fallback node
```

删除不再使用的自定义源：

```shell
pinset source remove node cn-mirror
```

活动源或回退列表正在引用的源不能删除。默认只接受 HTTPS；受信任局域网确实只有 HTTP 时必须显式添加 `--allow-insecure`。

安全边界：

- 精确版本、规范产物路径和 SHA-256 都来自项目锁文件；
- 锁文件中的 SHA-256 由 Node 官方 HTTPS `SHASUMS256.txt` 解析得到；
- 镜像只提供相同字节的下载传输，不能改变预期哈希；
- 连接、超时等网络错误可以回退；
- 哈希错误是硬失败，不会自动换源；
- MVP 尚未校验 `SHASUMS256.txt` 的 PGP 签名，这是后续供应链增强项。

首次执行 `pinset use` 仍需要从 `nodejs.org` 的 HTTPS 发布目录取得体积很小的 `SHASUMS256.txt`；镜像配置目前只替换运行时归档的下载位置。如果官方站在某个网络中完全不可达，可以在可访问网络中生成并提交 `pinset.lock`，然后在受限网络中配置镜像并执行 `pinset install --locked`。这个安装过程直接使用锁文件中的哈希，不再请求官方清单。

## 7. CI 使用

把 `pinset.toml` 和 `pinset.lock` 提交后，CI 可执行：

```shell
pinset install --locked
pinset exec -- node --version
pinset exec -- npm ci
pinset exec -- npm test
```

首次需要网络，重复安装同一版本和目标时会验证安装收据和关键路径，并直接复用。

## 8. 诊断

```shell
pinset doctor
```

它只读检查：

- Pinset 数据目录；
- 最近项目配置；
- 锁文件是否与项目版本一致；
- 当前目标的 Node 是否已安装；
- shim 目录是否在 PATH；
- PATH 上其他 Node 候选。

常见问题：

- `pinset.toml was not found`：在项目目录运行，或先执行 `pinset init`。
- `lockfile does not match`：重新执行 `pinset use node@完整版本 --no-install` 并审查锁文件。
- `runtime ... missing`：执行 `pinset install --locked`。
- shim 安装提示目标已存在：Pinset 为避免覆盖其他管理器或用户文件而停止；先用 `pinset doctor` 和系统命令确认冲突来源。
- 镜像 404：镜像 URL 不是 Node 发布目录根路径，或尚未同步该版本。
- 哈希不匹配：停止使用该镜像并调查，不要通过关闭校验绕过。Pinset 没有关闭校验的选项。

## 9. MVP 明确不包含

- Python、Flutter 安装；
- `node@24`、`lts`、`latest` 等浮动选择器；
- Node 发布清单 PGP 验签；
- 自动编辑 PATH 或 shell profile；
- 自动导入 nvm/fnm/uv/FVM 配置；
- 缓存清理、离线包导入、卸载；
- Homebrew Tap、Scoop Bucket 或其他第三方包仓库维护；
- 任意脚本插件、安装后脚本、远程代码执行配置。
