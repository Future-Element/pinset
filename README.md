# Pinset

[![CI](https://github.com/Future-Element/pinset/actions/workflows/ci.yml/badge.svg)](https://github.com/Future-Element/pinset/actions/workflows/ci.yml)
[![Release](https://github.com/Future-Element/pinset/actions/workflows/release.yml/badge.svg)](https://github.com/Future-Element/pinset/actions/workflows/release.yml)

Pinset 是一个面向多语言项目的本地优先运行时版本管理 CLI。它希望用一套一致的命令替代 fnm/nvm、uv、FVM 等工具在“选择和安装运行时版本”上的重叠工作。

当前 `0.1.0-alpha.4` 是 Node-first 预发布版：

- 支持 Node.js 精确版本、主版本/主次版本、`lts` 和 `current`；
- 支持 Windows x64、Linux x64、macOS x64/arm64 的官方预编译产物；
- 生成可提交的 `pinset.toml` 与 `pinset.lock`；
- 从 Node 官方 HTTPS `SHASUMS256.txt` 取得哈希，镜像只改变传输位置；
- 校验 SHA-256 后安全解压 ZIP/TAR.XZ，并以事务方式提交安装；
- 提供版本列表、安全卸载、内容寻址下载缓存、来源测试和旧管理器只读检测；
- 提供 `doctor --json` 和一次性 `exec node@<selector>`；
- 支持配置国内、企业内网或其他自定义镜像及有序回退；
- 支持独立的全局 Node 选择、项目覆盖和安全系统 PATH 透传；
- 支持英文与简体中文界面，并可按用户持久保存语言偏好。

alpha.4 新增一等全局默认命令、便捷 shim 安装和 Node 归档下载进度显示。Python、Flutter、
PGP 验签和中央包管理器分发仍按路线图推进。项目不维护第三方 Homebrew Tap 或 Scoop Bucket。

## 安装 Pinset

Linux x64 和 macOS Apple Silicon 可以执行：

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/Future-Element/pinset/releases/download/v0.1.0-alpha.4/install.sh |
  sh -s -- --version 0.1.0-alpha.4
```

安装器识别平台，从同一个 GitHub Release 下载归档和 `SHA256SUMS`，强制核对 SHA-256，然后把 `pinset` 与 `pinset-shim` 原子安装到 `$HOME/.local/bin`。它不使用 `sudo`、不改 shell profile，也不安装 Node。

稳定版本发布后可以把固定版本 URL 换成 `https://github.com/Future-Element/pinset/releases/latest/download/install.sh`。也可以从 [GitHub Releases](https://github.com/Future-Element/pinset/releases) 手动下载当前系统的归档。

## 使用说明

### 1. 将 Pinset 放入 PATH

Linux、macOS、WSL 当前终端：

```bash
export PATH="$HOME/.local/bin:$PATH"
pinset --version
```

确认无冲突后，可自行将同一行写入 `~/.bashrc`、`~/.zshrc` 等 shell profile。Pinset 不会
自动修改这些文件。Windows 解压 Release ZIP 后，把解压目录加入当前 PowerShell：

```powershell
$pinsetBin = "C:\path\to\pinset"
$env:PATH = "$pinsetBin;$env:PATH"
pinset --version
```

Windows 与 WSL 是两个独立环境，需要分别安装并分别配置 PATH。

### 2. 设置界面语言

持久使用简体中文：

```shell
pinset --lang zh-CN
```

持久恢复英文：`pinset --lang en`。只对单次命令临时切换：
`pinset --lang en doctor`。语言设置保存在当前系统的 `PINSET_HOME/settings.toml`，不会写入项目。

### 3. 设置全局 Node 版本

推荐使用：

```shell
pinset global node@24
pinset global
pinset current
```

`pinset global node@24` 会查询官方版本索引、锁定精确版本并默认安装当前平台；不带版本的
`pinset global` 只读显示全局默认。只生成全局配置和锁、不下载 Node：

```shell
pinset global node@lts --no-install
pinset install --global --locked
```

原有入口继续兼容：

```shell
pinset use node@24 --global
pinset use node@lts --global --no-install
pinset install --global --locked
```

全局状态位于 `$PINSET_HOME/state/global.toml` 和 `global.lock`，不会在当前目录创建项目文件。

### 4. 设置项目 Node 版本

在项目根目录执行：

```shell
pinset init
pinset use node@22
pinset current
```

这会创建或更新 `pinset.toml` 与 `pinset.lock`，并默认安装当前平台的精确版本。建议把两个
文件一起提交。只生成配置和锁：

```shell
pinset use node@22 --no-install
pinset install --locked
```

项目版本优先于全局版本；离开项目目录后自动恢复全局版本。项目或全局已声明但安装缺失时，
Pinset 会明确失败，不会静默改用系统 Node。

### 5. 执行和检查当前版本

不安装 shim 也能完整使用 Pinset：

```shell
pinset current
pinset which node
pinset exec -- node --version
pinset exec -- npm --version
pinset exec -- npm ci
pinset doctor
pinset doctor --json
```

一次性使用某个已经安装的精确版本，不修改项目或全局状态：

```shell
pinset exec node@24.0.0 -- node --version
```

### 6. 直接使用 node、npm、npx 和 corepack

Pinset 可以自动找到与 `pinset` 同目录的 `pinset-shim`，并安装到用户级目录：

```shell
pinset shim install
pinset shim path
```

Linux、macOS、WSL 当前终端：

```bash
PINSET_SHIM_DIR="$(pinset shim path)"
export PATH="$PINSET_SHIM_DIR:$PATH"
node --version
npm --version
```

Windows PowerShell：

```powershell
$shimDir = pinset shim path
$env:PATH = "$shimDir;$env:PATH"
node --version
npm --version
```

目标目录已有任意同名文件时，Pinset 会拒绝整组安装，不覆盖 fnm、nvm、Volta 或用户文件。
高级场景仍可显式传入 `--binary` 和 `--dir`，具体说明见
[PRD 使用指南](docs/PRD.md#175-安装-shim)。

### 7. 下载进度

下载 Node 归档时，交互终端会显示进度条、百分比、已下载大小和总大小：

```text
downloading node-v24.0.0-linux-x64.tar.xz [============            ]  50% 15.0 MiB/30.0 MiB
```

只有 SHA-256 校验通过后才显示完成。重定向输出或 CI 环境使用简洁的开始/完成行；缓存命中
不会显示伪下载进度。

### 8. 查询、卸载和缓存

```shell
pinset list node
pinset list node --available
pinset uninstall node@20.19.0
pinset cache list
pinset cache clean
pinset import --dry-run
```

卸载默认拒绝删除当前项目或全局仍引用的版本。`--force` 只跳过引用保护，不会删除 Pinset
数据目录之外或缺少匹配安装收据的文件。`import --dry-run` 只检测 `.nvmrc`、`.node-version`、
Volta、asdf 和 mise 配置，不会修改它们。

完整的 Windows、macOS、Linux、WSL、shim、镜像切换和故障排查说明见 [PRD 使用指南](docs/PRD.md#17-当前版本使用指南)。

## 安装源

安装源是本机配置，不写入项目锁文件。下面只是格式示例，请使用你信任且与 Node 官方目录结构兼容的镜像：

```shell
pinset source add node my-mirror --base-url https://mirror.example/node/
pinset source use node my-mirror
pinset source fallback node official
pinset source list node
pinset source test node my-mirror
```

网络错误可按用户配置回退；哈希不匹配会立即停止，不会换源重试来掩盖异常。内置 `official` 源不可覆盖或删除。

首次生成锁文件仍需访问 Node 官方 HTTPS 校验清单；已有并提交的锁文件可以在受限网络中只通过镜像执行 `install --locked`。详见使用指南。

## alpha.3 新增功能

alpha.3 可以测试以下新增命令：

```shell
pinset list node
pinset list node --available
pinset use node@24 --no-install
pinset exec node@24.0.0 -- node --version
pinset uninstall node@24.0.0
pinset cache list
pinset cache clean
pinset doctor --json
pinset import --dry-run
```

浮动选择器在显式选择时读取官方版本索引，最终仍写入精确版本。一次性 `exec` 只执行已经
安装的匹配版本，不写项目或全局状态。卸载默认保护当前项目和全局引用；`--force` 不会扩大
删除范围，只表示接受引用暂时失效。缓存清理保留未知文件和非普通文件。

## 开发

```shell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --locked -p pinset-cli -p pinset-shim
```

测试使用临时目录、本地假 HTTP 服务和假运行时，不会安装真实 Node、Python 或 Flutter。

开发分支应优先在本机或 WSL 使用增量编译和测试。非草稿 Pull Request 在新建、重新打开和后续推送时自动运行 Quality，草稿 PR 跳过检查。推送到 `main` 运行质量门禁，版本标签执行完整三平台构建并自动发布 GitHub Release。

## 文档

- [PRD](docs/PRD.md)：产品、功能契约、技术架构、使用与故障排查的统一基线。
- [Plans](docs/PLANS.md)：版本范围、研究与决策、验证证据、开发和发布流程。
- [发布说明](docs/RELEASE_NOTES.md)：已经交付的用户可见变化、安装和已知限制。
- [贡献指南](CONTRIBUTING.md)
- [安全策略](SECURITY.md)

## 许可

Pinset 使用 [MIT License](LICENSE) 开源。
