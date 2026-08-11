# Pinset

[![CI](https://github.com/Future-Element/pinset/actions/workflows/ci.yml/badge.svg)](https://github.com/Future-Element/pinset/actions/workflows/ci.yml)
[![Release](https://github.com/Future-Element/pinset/actions/workflows/release.yml/badge.svg)](https://github.com/Future-Element/pinset/actions/workflows/release.yml)

Pinset 是一个面向多语言项目的本地优先运行时版本管理 CLI。它希望用一套一致的命令替代 fnm/nvm、uv、FVM 等工具在“选择和安装运行时版本”上的重叠工作。

当前 `0.1.0-alpha.1` 是 Node-first MVP：

- 支持 Node.js 精确版本 `x.y.z`；
- 支持 Windows x64、Linux x64、macOS x64/arm64 的官方预编译产物；
- 生成可提交的 `pinset.toml` 与 `pinset.lock`；
- 从 Node 官方 HTTPS `SHASUMS256.txt` 取得哈希，镜像只改变传输位置；
- 校验 SHA-256 后安全解压 ZIP/TAR.XZ，并以事务方式提交安装；
- 提供 `use`、`install`、`current`、`which`、`exec`、`doctor` 和 shim；
- 支持配置国内、企业内网或其他自定义镜像及有序回退。

Python、Flutter、浮动版本选择器、PGP 验签、缓存管理和中央包管理器分发不属于这个 MVP，后续按路线图实现。项目不维护第三方 Homebrew Tap 或 Scoop Bucket。

## 五分钟开始

当前预发布版在 Linux x64 和 macOS Apple Silicon 上可以执行：

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/Future-Element/pinset/releases/download/v0.1.0-alpha.1/install.sh |
  sh -s -- --version 0.1.0-alpha.1
```

安装器识别平台，从同一个 GitHub Release 下载归档和 `SHA256SUMS`，强制核对 SHA-256，然后把 `pinset` 与 `pinset-shim` 原子安装到 `$HOME/.local/bin`。它不使用 `sudo`、不改 shell profile，也不安装 Node。

稳定版本发布后可以把固定版本 URL 换成 `https://github.com/Future-Element/pinset/releases/latest/download/install.sh`。也可以从 [GitHub Releases](https://github.com/Future-Element/pinset/releases) 手动下载当前系统的归档。

安装后将 `pinset` 放入 `PATH`，再进入一个项目：

```shell
pinset init
pinset use node@24.0.0
pinset current
pinset which node
pinset exec -- node --version
pinset doctor
```

`pinset use` 会解析并锁定四个平台的官方 Node 产物，然后只为当前平台安装。要先生成配置和锁文件、暂不下载运行时：

```shell
pinset use node@24.0.0 --no-install
pinset install --locked
```

完整的 Windows、macOS、Linux、WSL、shim、镜像切换和故障排查说明见 [PRD 使用指南](docs/PRD.md#17-当前版本使用指南)。

## 安装源

安装源是本机配置，不写入项目锁文件。下面只是格式示例，请使用你信任且与 Node 官方目录结构兼容的镜像：

```shell
pinset source add node my-mirror --base-url https://mirror.example/node/
pinset source use node my-mirror
pinset source fallback node official
pinset source list node
```

网络错误可按用户配置回退；哈希不匹配会立即停止，不会换源重试来掩盖异常。内置 `official` 源不可覆盖或删除。

首次生成锁文件仍需访问 Node 官方 HTTPS 校验清单；已有并提交的锁文件可以在受限网络中只通过镜像执行 `install --locked`。详见使用指南。

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
