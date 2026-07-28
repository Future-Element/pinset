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

从 [GitHub Releases](https://github.com/Future-Element/pinset/releases) 下载当前系统的归档，校验 `SHA256SUMS` 后解压。将 `pinset` 放入 `PATH`，再进入一个项目：

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

完整的 Windows、macOS、Linux、WSL、shim、镜像切换和故障排查说明见 [MVP 使用指南](docs/USAGE.md)。

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

## 文档

- [MVP 使用指南](docs/USAGE.md)
- [项目章程](docs/PROJECT_CHARTER.md)
- [深度调研](docs/RESEARCH.md)
- [产品规格](docs/PRODUCT_SPEC.md)
- [技术架构](docs/ARCHITECTURE.md)
- [路线图](docs/ROADMAP.md)
- [决策记录](docs/DECISIONS.md)
- [WSL 构建与测试](docs/WSL_TESTING.md)

## 许可

开源许可证尚未决定。在许可证文件加入仓库前，不应把当前代码视作已授予通用开源许可。
