# Pinset Plans

当前版本：`v0.1.0-beta.1`

下一个版本：`v0.1.0`

更新时间：`2026-08-12`

Beta 已发布。接下来继续完善 Node 支持，处理实际使用中的兼容问题，并准备稳定版。

## 版本路线

| 版本 | 状态 | 主要内容 |
| --- | --- | --- |
| `v0.1.0-alpha.1` | 已发布 | 项目配置、锁文件、精确版本安装、镜像、shim 和三平台构建 |
| `v0.1.0-alpha.2` | 已发布 | 全局版本、项目覆盖、系统 PATH 回退和中文界面 |
| `v0.1.0-alpha.3` | 已发布 | 浮动版本、本地版本列表、卸载、缓存、诊断和旧配置检测 |
| `v0.1.0-alpha.4` | 已发布 | `global` 命令、下载进度和使用文档 |
| `v0.1.0-alpha.5` | 已发布 | 自动注册 node/npm/npx/corepack、独立安装和旧 shim 迁移 |
| `v0.1.0-alpha.6` | 已发布 | 完整卸载脚本和跨 Shell 修复 |
| `v0.1.0-beta.1` | 已发布 | 简短安装命令、断点续传、可信镜像、离线导入和三平台 Node 测试 |
| `v0.1.0` | 计划中 | 上游签名验证、兼容性确认和 Beta 问题修复 |
| `v0.2.0+` | 未排期 | 评估 Python、Flutter 等 Provider |

版本号不对应固定日期。稳定版完成前不会加入新的 Provider。

## Beta 已完成

### Node 使用

- 设置和查看全局 Node 版本；
- 使用项目配置覆盖全局版本；
- 离开项目目录后恢复全局版本；
- 支持精确版本、主版本、主次版本、LTS 和 Current；
- 直接运行 `node`、`npm`、`npx` 和 `corepack`；
- 独立安装版本，不改变项目或全局选择；
- 导入 `.nvmrc`、`.node-version`、Volta、asdf 和 mise 配置；
- 卸载单个 Node 版本或完整删除 Pinset。

### 下载和安装

- 下载进度在同一行刷新，并适配窄终端和中文；
- 同一版本的并发安装使用文件锁；
- 支持 HTTP Range 断点续传；
- 归档按 SHA-256 缓存，可手动导入离线归档；
- 支持下载镜像、备用源和可信 HTTPS 元数据镜像；
- 校验失败、归档格式错误或解压安全检查失败时立即停止；
- 安装在临时目录完成，校验通过后再写入最终目录。

### 平台和发布

- Pinset 安装包：Linux x64、Windows x64、macOS Apple Silicon；
- Node 归档：Windows x64、Linux x64、macOS x64 和 macOS arm64；
- Release 包含 `SHA256SUMS`、CycloneDX SBOM 和 GitHub 构建来源证明；
- Linux、Windows、macOS 的 Release 任务都测试了全局版本、项目版本和四个 Node 命令；
- curl 安装器只安装 `pinset` 与 `pinset-shim`，不会自动安装 Node 或修改 shell profile。

Beta 暂不支持 Python、Flutter、Linux arm64 Pinset 安装包和 macOS Intel Pinset 安装包，也不维护 Homebrew Tap 或 Scoop Bucket。

## 本地检查

日常开发使用以下命令，不下载真实 Node，也不会触发 GitHub Actions：

```shell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --locked -p pinset-cli -p pinset-shim
git diff --check
```

测试使用临时 `PINSET_HOME`、本地 HTTP 服务、测试归档和假运行时。WSL 中也应设置独立的临时目录，避免碰到已有的 Pinset 或 Node 安装。

真实 Node 测试只在目标系统或 Release 工作流的隔离 Runner 中运行。普通提交不运行这类测试，避免不必要的 GitHub Actions 消耗。

## 发布流程

1. 更新版本号、README、PRD、Plans 和 Release Notes；
2. 在本地完成格式检查、Clippy、测试、release build 和脚本测试；
3. 合并到 `main`，确认 Quality 工作流通过；
4. 在该提交上创建签名 tag；
5. Release 工作流构建三个平台，并运行真实 Node 测试；
6. 生成归档、校验和、SBOM 和构建来源证明；
7. 发布后下载公开文件，复查归档内容和 `SHA256SUMS`。

如果任一平台测试失败，本次 Release 不发布残缺资产，也不手动绕过工作流。

## v0.1.0 计划

稳定版仍然只做 Node，主要工作有：

1. 验证 Node 上游 `SHASUMS256.txt.sig`，整理发布密钥的更新和撤销方式；
2. 确认 schema 1 的兼容规则和迁移方式；
3. 修复 Beta 用户在代理、镜像、断点续传、离线导入和旧管理器迁移中发现的问题；
4. 增加更多三平台实际使用测试；
5. 制定 Release 撤回和安全公告流程；
6. 决定是否发布 Linux arm64 和 macOS Intel 安装包。

## 后续 Provider

Python 和 Flutter 放在 Node 稳定版之后评估。

Python 需要先确定解释器来源、校验方式、虚拟环境职责和系统 Python 回退规则。Flutter 需要确定 SDK 索引、channel、精确版本、Flutter/Dart 命令路由，以及 Android/iOS 工具链与 Pinset 的职责范围。

新 Provider 应复用现有的配置、锁文件、安装源、缓存、安装器、命令路由、诊断和卸载机制。

## 主要风险

| 风险 | 当前处理 | 后续工作 |
| --- | --- | --- |
| 国内网络无法访问官方元数据 | 可配置可信 HTTPS 元数据镜像 | 收集常用镜像的兼容反馈 |
| 下载中断浪费流量 | Range 续传和 `Content-Range` 校验 | 测试更多代理和 CDN |
| 并发安装损坏目录 | 文件锁和临时目录安装 | 增加三平台压力测试 |
| 镜像归档被替换 | 使用可信元数据中的 SHA-256 | 稳定版增加上游 PGP 验证 |
| 命令路由覆盖外部工具 | 写入前检查所有权，冲突时停止 | 增加旧管理器共存测试 |
| CI 消耗过高 | 本地测试优先，真实 Node 只在 Release 测试 | 保持普通提交不下载运行时 |
| 预发布配置发生变化 | schema 版本和 Release Notes | 稳定版确认兼容规则 |
