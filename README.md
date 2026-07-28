# Pinset

[![CI](https://github.com/Future-Element/pinset/actions/workflows/ci.yml/badge.svg)](https://github.com/Future-Element/pinset/actions/workflows/ci.yml)

Pinset 是一个面向多语言项目的本地优先运行时版本管理 CLI。它计划用一套一致的命令管理 Node.js、CPython 和 Flutter，并重点解决跨平台差异、旧管理器冲突、可复现安装和供应链校验。

> 当前状态：Phase 0 技术验证中。Spike A 已完成 Windows x64 功能原型，但端到端 shim 性能尚未达到候选目标；Spike B 已完成本地 HTTP + ZIP 的事务安装内核、本机安装源配置和无网络 Node 产物计划，真实上游校验与 Linux/macOS `tar.xz` 安装仍待接入。

## 核心定位

Pinset 不试图复制 mise/asdf 的全部能力。首版聚焦三个承诺：

1. **可预测**：项目配置是纯数据；选择优先级、实际命令来源和回退行为都可解释。
2. **可验证**：锁文件记录精确版本、目标平台、下载来源和校验信息；安装失败不会留下半成品。
3. **可迁移**：识别 fnm、nvm、nvm-windows、uv、FVM、mise、asdf、vfox 等既有环境，遵循 “detect many, activate one”，不静默删除或改写用户环境。

国内或受限网络可以显式切换安装源。安装源只替换下载位置，精确版本、官方产物身份和预期哈希仍来自锁文件/可信 provider；网络失败可以按用户配置回退，校验失败必须停止。

## 首版范围

- 运行时：Node.js、CPython、Flutter（Flutter 自带 Dart）
- 平台：Windows、macOS、Linux
- 使用方式：全局选择、项目选择、锁文件安装、临时执行、冲突诊断
- 明确不做：npm/pnpm/pip/pub 依赖管理、任务运行器、环境变量/密钥管理、远程同步、GUI、任意脚本插件

规划中的命令示例：

```shell
pinset init
pinset use node@24
pinset use python@3.13
pinset use flutter@3.44.8
pinset install
pinset current
pinset which node
pinset exec node@22 -- node -v
pinset doctor
pinset import --dry-run
```

当前 spike 已实现且不会下载运行时的安装源命令：

```shell
pinset source list [provider]
pinset source add <provider> <alias> --base-url <https-url>
pinset source use <provider> <alias>
pinset source fallback <provider> [aliases...]
pinset source remove <provider> <alias>
```

这些命令只读写 `$PINSET_HOME/sources.toml`。`official` 是不可覆盖、不可删除的内置源；自定义源默认必须使用 HTTPS。`source test` 仍是规划命令，当前不会由 Pinset 主动探测或测速第三方源。

## 项目文档

- [项目章程](docs/PROJECT_CHARTER.md)
- [深度调研](docs/RESEARCH.md)
- [产品规格](docs/PRODUCT_SPEC.md)
- [技术架构](docs/ARCHITECTURE.md)
- [路线图与验证计划](docs/ROADMAP.md)
- [决策记录](docs/DECISIONS.md)
- [Spike A：跨平台 shim](docs/spikes/SPIKE_A_SHIM.md)
- [Spike B：事务安装内核](docs/spikes/SPIKE_B_INSTALL_TRANSACTION.md)
- [WSL 构建与安全测试](docs/WSL_TESTING.md)

## 当前开发

```shell
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo run --release -p pinset-core --example resolve_bench -- 5000
cargo build --release -p pinset-cli -p pinset-shim
cargo run --release -p pinset-shim --example process_bench -- 1000
```

每次推送到 `main` 或手动运行 workflow 时，GitHub Actions 会先运行格式、Clippy 和测试，再并行生成三个可下载 artifact：

- `pinset-linux-x86_64`
- `pinset-windows-x86_64`
- `pinset-macos-aarch64`

Pull Request 只运行质量检查，避免私有仓库重复消耗三平台构建分钟。构建文件保留 14 天。当前 artifact 未进行代码签名、公证或 GitHub Release 发布，只用于 Phase 0 跨平台测试。

当前 workspace 包含：

- `pinset`：用于 `which`、`current`、安装多调用 shim 和管理本机安装源的最小 CLI；
- `pinset-core`：严格配置读取、祖先目录查找、命令解析、安全 shim 安装、原子化安装源配置，以及 feature 隔离的事务安装内核；
- `pinset-shim`：根据调用文件名选择运行时，并完整传递参数与退出码。

这仍是技术 spike，不代表 v0.1 CLI 契约已经冻结。

## 名称与分发

产品名和命令名确定为 `Pinset` / `pinset`。npm 上已经存在同名旧包，因此 Pinset 不依赖无作用域的 npm 包名；预期通过 GitHub Releases、Homebrew、WinGet、Scoop 等原生渠道分发。如未来需要 npm 启动器，应使用组织作用域。商标和域名尚未完成法律层面的可用性检索。

## 许可证

开源许可证尚未决定。进入公开代码阶段前，需要在 Apache-2.0 与 MIT（或双许可证）之间做出明确选择。
