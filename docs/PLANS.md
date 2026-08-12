# Pinset Plans

当前公开版本：`v0.1.0-alpha.6`
当前发布目标：`v0.1.0-beta.1`
后续稳定目标：`v0.1.0`
状态：`Beta 候选已实现，等待一次完整发布门禁`
更新时间：`2026-08-12`

## 1. 文档边界

- [PRD](PRD.md) 定义产品定位、Node Beta 功能、安全边界和验收标准；
- 本文定义版本范围、执行顺序、发布门禁、风险和后续路线；
- [Release Notes](RELEASE_NOTES.md) 只记录实际交付或正在准备发布的用户可见变化；
- `README.md` 是用户安装和使用入口。

计划项只有在代码、测试和相应发布证据均满足时才视为发布完成。静态检查不等于真实运行时验收，三平台构建不等于三平台 Node 工作流验收。

## 2. 路线概览

| 版本 | 主题 | 状态 | 结果 |
| --- | --- | --- | --- |
| `v0.1.0-alpha.1` | Node Project MVP | 已发布 | 项目配置/锁、精确安装、镜像、shim、三平台 Release |
| `v0.1.0-alpha.2` | 全局与项目解析 | 已发布 | 全局选择、项目覆盖、系统 PATH、中文 |
| `v0.1.0-alpha.3` | 生命周期与迁移 | 已发布 | 浮动选择、本地列表、卸载、缓存、doctor JSON、旧配置检测 |
| `v0.1.0-alpha.4` | Node 工作流收口 | 已发布 | `global` 入口、下载进度、README 使用路径 |
| `v0.1.0-alpha.5` | Provider 路由闭环 | 已发布 | 自动 node/npm/npx/corepack 路由、独立安装、导入和 shim 迁移 |
| `v0.1.0-alpha.6` | 完整卸载 | 已发布 | 安全完整卸载脚本和跨 Shell 修复 |
| `v0.1.0-beta.1` | Node 公测闭环 | 当前目标 | 简短安装、下载韧性、可信镜像、离线导入、三平台真实验收、供应链信息 |
| `v0.1.0` | Node 稳定版 | 后续 | 上游 PGP、schema/兼容冻结、Beta 反馈和发布治理 |
| `v0.2.0+` | 新 Provider | 未排期 | 在 Node 稳定后评估 Python、Flutter |

版本号不是日期承诺。上一阶段安全和兼容门禁未通过时，不以增加 Provider 代替风险收敛。

## 3. v0.1.0-beta.1 范围

### 3.1 纳入

- Node.js 全局、项目和系统 PATH 的完整解析；
- `node`、`npm`、`npx`、`corepack` 直接与显式执行；
- 精确、主版本、主次版本、LTS、Current 选择器；
- 简化为 `curl -fsSL <install.sh> | sh` 的推荐安装入口；
- 一行刷新且自适应终端宽度的下载进度；
- 同一安装跨进程互斥；
- HTTP Range 断点续传；
- SHA-256 内容寻址缓存和离线归档导入；
- 普通归档镜像、有序 fallback、显式可信元数据镜像；
- 中英文界面、诊断、迁移、单版本和完整卸载；
- Linux x64、Windows x64、macOS Apple Silicon Release；
- 三个平台隔离 Runner 中的真实 Node 全局/项目验收；
- SHA256SUMS、CycloneDX SBOM、GitHub 构建来源证明；
- 面向首次用户的完整 README。

### 3.2 明确排除

- Python、Flutter 或其他 Provider；
- npm 包、全局包或包管理器版本管理；
- Node 上游 SHASUMS OpenPGP 验签；
- Linux arm64 和 macOS Intel Pinset Release；
- Homebrew Tap、Scoop Bucket 或第三方分发仓库；
- shell profile、系统 PATH、IDE 的自动永久修改；
- 云同步、GUI 和后台服务。

## 4. Beta 实施状态

### A. 进度显示收口

状态：`已实现，本地测试通过`

- [x] TTY 同一行刷新；
- [x] 按终端宽度缩放；
- [x] 使用 Unicode 显示宽度处理中文；
- [x] 文件名中间截断；
- [x] 最后一列留空，避免终端自动折行产生多行；
- [x] 24/40/60/80 列与中英文测试；
- [x] 非 TTY 输出保持简洁。

### B. 简化安装入口

状态：`已实现，待 Release 资产验证`

- [x] `install.sh` 内置 `0.1.0-beta.1` 推荐版本；
- [x] 主入口缩短为 `curl -fsSL raw.../install.sh | sh`；
- [x] 保留 `--version` 和 `PINSET_VERSION` 精确覆盖；
- [x] 安装器内部继续限制 HTTPS/TLS 并校验 SHA-256；
- [x] 发布门禁验证 workspace 版本、tag 与安装器默认版本一致；
- [x] README 解释旧 curl 参数的含义和固定版本用法；
- [ ] 发布后从公开 URL 重新执行隔离安装器验收。

### C. 国内镜像与信任模型

状态：`已实现，本地测试通过`

- [x] 自定义源默认仅改变归档传输；
- [x] 官方元数据继续作为默认信任根；
- [x] `--trust-metadata` 允许受信 HTTPS 镜像提供 index 和 SHASUMS；
- [x] 源列表显示 `trusted-metadata`；
- [x] HTTP 与元数据信任互斥；
- [x] 网络错误 fallback，哈希/格式/安全错误硬停止；
- [x] README 给出普通镜像、可信元数据镜像和内网 HTTP 示例。

### D. 下载和安装韧性

状态：`已实现，本地测试通过`

- [x] 按 `tool + version + target` 获取跨进程文件锁；
- [x] 相同运行时并发安装只下载和提交一次；
- [x] 断点文件按预期 SHA-256 命名；
- [x] 续传前重新哈希已有内容；
- [x] 发送 Range 并校验 Content-Range 起点；
- [x] 服务端忽略 Range 或返回 416 时安全重新下载；
- [x] 完整校验成功后才写内容寻址缓存；
- [x] 哈希失败删除不可信断点；
- [x] `cache clean` 清理完整缓存和受识别断点。

### E. 离线缓存导入

状态：`已实现，本地测试通过`

- [x] `pinset cache import <archive> --sha256 <hash>`；
- [x] 普通文件和符号链接边界；
- [x] 最大大小限制；
- [x] 流式 SHA-256；
- [x] 临时文件和 no-clobber 原子提交；
- [x] 已存在缓存重新验证；
- [x] CLI 与核心测试；
- [x] README 离线流程。

### F. 三平台真实 Node 验收

状态：`工作流已实现，等待 tag 运行`

Release 的 Linux、Windows、macOS 构建任务分别在随机临时 `PINSET_HOME` 中：

- [x] 设置并持久化中文；
- [x] 安装 Node 24.0.0 全局版本；
- [x] 安装 Node 22.0.0 项目版本；
- [x] 验证项目覆盖；
- [x] 验证离开项目后恢复全局版本；
- [x] 验证 `pinset exec` 的 node/npm/npx/corepack；
- [x] 验证 PATH 直接调用的 node/npm/npx/corepack；
- [x] 验收完成后删除 Runner 临时目录；
- [ ] `v0.1.0-beta.1` tag 的三平台任务实际全部成功。

### G. Release 供应链

状态：`工作流已实现，等待 tag 运行`

- [x] tag、workspace、安装器版本一致性检查；
- [x] 锁定格式、Clippy、workspace 测试和 release build；
- [x] 三个 CycloneDX JSON SBOM；
- [x] 三个平台归档的 GitHub 构建来源证明；
- [x] 安装器、卸载器、校验和、SBOM 的来源证明；
- [x] SHA256SUMS 覆盖全部 Release 资产；
- [x] GitHub Actions 使用固定完整 SHA；
- [ ] 签名 tag 已推送；
- [ ] GitHub Release 自动创建并标记 prerelease；
- [ ] 发布后重新下载并复算全部校验；
- [ ] `gh attestation verify` 验证三平台归档。

### H. 文档

状态：`候选版完成`

- [x] README 从安装到卸载的主路径；
- [x] 解释短 curl 与旧参数；
- [x] 全局和项目版本示例；
- [x] 路由、镜像、续传、离线、诊断和迁移；
- [x] Release 完整性验证；
- [x] PRD、Plans、Release Notes 与 Node-only 范围一致；
- [ ] 发布后补充实际 tag、Actions run 和资产验证结果。

## 5. 本地验证策略

本地验证不安装真实 Node，也不触发 GitHub Actions：

```shell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --locked -p pinset-cli -p pinset-shim
git diff --check
```

测试范围：

- 随机临时 `PINSET_HOME`；
- 本地假 HTTP 服务；
- 构造 ZIP/TAR.XZ 和假 Node 命令；
- 并发线程和文件锁；
- 断点、Range、校验失败和回退；
- POSIX/PowerShell 卸载器对假目录的删除；
- 不访问真实用户安装，不下载语言运行时。

WSL 仅用于 Linux 编译和同一套离线测试；目标目录与开发机真实 `PINSET_HOME` 隔离。

## 6. 一次性发布流程

为了避免反复消耗 GitHub Actions，Beta 只在所有本地门禁通过后触发一次正式链路。

### 6.1 发布前

1. 确认工作树只包含 Beta 相关文件；
2. 本地完成格式、Clippy、测试、release build、脚本语法和差异检查；
3. 确认 README、PRD、Plans、Release Notes 与版本一致；
4. 确认 `Cargo.lock` 已更新且 locked build 成功；
5. 创建并签名 Beta 候选提交；
6. 推送 feature 分支；
7. fast-forward 合并到 `main` 并推送；
8. 等待唯一一次 main Quality 成功。

### 6.2 发布

1. 在已验证 main commit 创建签名 tag `v0.1.0-beta.1`；
2. 推送 tag；
3. Release Quality 重新执行完整静态/单元门禁；
4. 三个平台构建并执行真实 Node 验收；
5. 生成 SBOM、来源证明和 SHA256SUMS；
6. 自动创建 prerelease GitHub Release。

### 6.3 发布后

1. 通过 GitHub API 确认 Release 公开且 tag/commit 正确；
2. 下载全部资产到随机临时目录；
3. 复算 SHA256SUMS；
4. 检查 Unix 归档只有 `pinset` 与 `pinset-shim`；
5. 检查 Windows ZIP 只有两个 `.exe`；
6. 执行公开 `install.sh` 与卸载器隔离测试；
7. 验证归档 attestation；
8. 在 Release Notes 记录实际 Actions run 与验证边界；
9. 使用 `[skip ci]` 的文档提交更新 main，避免重复运行 CI。

## 7. Beta 发布判定

允许发布必须同时满足：

- 所有本地门禁通过；
- main Quality 成功；
- tag Release Quality 成功；
- 三个平台真实 Node 验收成功；
- 三个平台归档均生成来源证明；
- 资产数量、名称、归档内容、版本和 SHA256SUMS 均正确；
- 安装器默认版本可从公开 Release 下载；
- README 不把 Python/Flutter 或上游 PGP 描述为已完成。

只要任一条件失败，就不把 Release 标记为完成，也不以手动上传部分资产绕过工作流。

## 8. v0.1.0 稳定版计划

稳定版继续只聚焦 Node：

1. 验证 Node 上游 `SHASUMS256.txt.sig`；
2. 明确受信 Node 发布密钥、轮换、撤销和离线密钥更新流程；
3. 冻结 schema 1 的兼容与迁移承诺；
4. 收集 Beta 在代理、可信镜像、断点续传、离线导入和旧管理器迁移中的反馈；
5. 修复三平台真实使用问题；
6. 形成 Release 撤回、安全公告和受影响版本策略；
7. 决定是否为 Linux arm64 和 macOS Intel 提供官方归档；
8. 发布稳定版前重新执行全部 Beta 门禁。

稳定版不以新增 Provider 为目标。

## 9. 稳定版之后

### 9.1 Python Provider 研究

需要先确定：

- 解释器分发来源和信任模型；
- CPython 与独立构建产物的边界；
- venv、uv 和包管理职责如何避免重叠；
- Windows/Linux/macOS 目标矩阵；
- 命令集合和系统 Python 回退策略。

### 9.2 Flutter Provider 研究

需要先确定：

- 官方 SDK 索引、归档和校验来源；
- Flutter/Dart 命令路由；
- channel 与精确版本锁定；
- Git 仓库式 SDK 与归档式安装的取舍；
- Android/iOS 工具链不纳入 Pinset 所有权。

任何新 Provider 都必须复用现有 config/lock、source、cache、installer、shim、doctor 和 uninstall 契约；如需破坏契约，先更新 PRD 和迁移方案。

## 10. 已接受决策

- `Accepted`：Node-only Beta，不为展示多语言愿景而加入半成品 Provider；
- `Accepted`：安装器只安装 CLI 和通用 shim；
- `Accepted`：正常选择/安装自动注册 Provider 命令；
- `Accepted`：推荐 curl 命令简短，安全限制在安装器内部继续执行；
- `Accepted`：官方元数据默认信任，自定义元数据必须显式 `--trust-metadata`；
- `Accepted`：不维护第三方 Tap/Bucket；
- `Accepted`：项目/全局声明缺失时失败关闭；
- `Accepted`：不修改 shell profile；
- `Accepted`：真实运行时测试只在隔离目标系统或一次性 Release CI；
- `Accepted`：上游 PGP 验签是稳定版门禁，不在 Beta 仓促引入。

## 11. 主要风险

| 风险 | 当前控制 | 后续动作 |
| --- | --- | --- |
| 国内网络无法访问官方元数据 | 显式可信 HTTPS 元数据镜像 | 收集真实镜像兼容反馈 |
| 下载中断重复浪费流量 | Range 续传、Content-Range 校验 | 在更多代理/CDN 下验收 |
| 同版本并发破坏安装 | 跨进程锁、事务提交 | 三平台压力测试 |
| 镜像篡改归档 | 官方/可信元数据 SHA-256，哈希失败硬停止 | 稳定版增加上游 PGP |
| shim 覆盖外部工具 | 所有权验证、整组拒绝、doctor | 增加真实旧管理器案例 |
| CI 成本上涨 | 本地门禁优先，真实 Node 只在 Release | 保持普通 PR 无真实下载 |
| 预发布 schema 变化 | 锁文件显式 schema、Release Notes | 稳定版冻结迁移承诺 |
