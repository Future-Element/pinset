# Pinset 路线图与验证计划

路线图以风险消除为顺序，不以功能数量为顺序。任何阶段通过前，不承诺具体发布日期。

## Phase 0：立项与关键技术验证

当前状态：进行中。Spike A 已完成 Windows x64 功能原型，但性能门槛暂未通过；macOS/Linux 尚未运行。详见 [Spike A 记录](spikes/SPIKE_A_SHIM.md)。

### Spike A：跨平台 shim

状态：**Provisional**

交付：

- 最小 Rust CLI + 多调用 shim；
- 向上查找项目配置；
- Node 假运行时切换；
- Windows、macOS、Linux 基准测试。

通过条件：

- 无网络路由；
- 无递归；
- 嵌套目录选择正确；
- 热路径 p95 候选目标 ≤ 10 ms；
- Windows 不需要管理员权限完成日常执行。

当前结果：

- Windows x64 的配置查找、假 Node 切换、实际多调用文件名、递归保护和退出码传递已通过自动化测试；
- 核心解析 p95 为 195 μs；
- 完整 shim 额外进程开销估算 p95 为 18.0 ms，未达到候选 10 ms；
- macOS/Linux 功能与性能验证仍未完成，因此 Spike A 尚不能转为 Accepted。

### Spike B：三类真实产物

状态：**Provisional — 事务安装内核已通过，真实产物未接入**

交付：

- 每个 provider 安装一个固定版本；
- 校验、解压、版本验证、原子提交；
- 本地假镜像与错误哈希测试。

通过条件：

- 认证平台均能执行真实命令；
- 校验失败不可降级；
- 中断不留下可选择的半安装。

当前结果：

- 已实现本地 HTTP 下载、SHA-256 强制校验、安全 ZIP 解压、临时目录和原子 rename；
- 坏哈希、截断响应、ZIP 路径穿越和展开大小超限均不会产生最终安装；
- 安装收据不保存 URL 用户信息、查询参数或 fragment；
- 安装器支持有序源候选，网络失败可回退到用户批准的下一个源；
- 哈希失败立即停止，不允许通过自动换源掩盖；
- 本机 `sources.toml`、内置 official 源，以及 `source list/add/use/fallback/remove` 已实现；
- source 配置 CLI 已在随机临时 `PINSET_HOME` 中验证，不下载运行时、不修改真实用户环境；
- Node provider 已能为六个首批 target 生成 canonical artifact path、归档格式、布局路径和 active/fallback URL 候选；
- Node provider 计划测试为纯字符串/配置测试，不访问 Node.js 上游；
- HTTP/TLS/ZIP 依赖通过 Cargo feature 与 shim 隔离；
- GitHub Actions 已配置 Linux x64、Windows x64、macOS arm64 三平台构建和 14 天 artifact；
- CI 使用 GitHub 托管 runner，先通过格式、Clippy 和测试再开始三平台 release build；
- 尚未接入 Node、Python、Flutter 真实产物，也未实现 tar.xz/tar.zst、跨进程锁和缓存。

详见 [Spike B 记录](spikes/SPIKE_B_INSTALL_TRANSACTION.md)。

### Spike C：Flutter IDE

交付：

- `which --sdk`；
- 项目稳定 SDK 路径原型；
- VS Code 与 Android Studio 至少各做一次观察性验证。

通过条件：

- 切换后路径可预测；
- 不写用户 IDE 配置也有可操作流程；
- Windows 权限与 junction 行为明确。

### Spike D：Python 来源与许可证

交付：

- 产物清单固定方案；
- `PYTHON.json`/license 元数据记录；
- 代理、镜像和离线缓存原型。

通过条件：

- 每个认证 target 能解释产物来源、校验与许可证；
- 不依赖未经固定的“最新”远程状态复现锁文件。

### Spike E：共存诊断

交付：

- PATH 枚举；
- fnm、nvm/nvm-windows、uv、FVM、mise、asdf、vfox 探针；
- `doctor --json` 草案。

通过条件：

- 在至少五种真实/可复现环境组合中解释实际命令来源；
- 不修改或卸载其他管理器；
- 修复建议可逆。

Phase 0 总体 go/no-go：

- 五项 spike 都有书面结果；
- 无法解决 Flutter IDE 稳定路径、Python 可信清单或 shim 性能时，不进入完整 v0.1；
- 产品访谈显示“可预测迁移与校验”不是显著痛点时，缩小或终止项目。

## Phase 1：Node 核心闭环

范围：

- 配置、锁文件、全局选择；
- Node provider；
- 安装事务；
- shim 与 `exec`；
- `current`、`which`、基础 `doctor`；
- 认证矩阵 CI。

出口条件：

- 新项目与克隆项目流程可复现；
- Node 签名清单与产物哈希验证；
- 常见 PATH 冲突有明确输出；
- 文档和 schema 进入兼容性管理。

## Phase 2：CPython 与离线能力

范围：

- python-build-standalone provider；
- Python 命令映射；
- 许可证/来源展示；
- 镜像、自定义 CA、离线缓存；
- `import` 的 `.python-version` 支持。

出口条件：

- 与 uv/pip 的职责边界通过真实项目验证；
- 认证矩阵通过；
- 锁文件在离线模式可复现。

## Phase 3：Flutter 与 IDE 流程

范围：

- Flutter stable provider；
- Dart 路由；
- `.fvmrc` 只读导入；
- 稳定 SDK 路径和 IDE 文档；
- 完整 `doctor`。

出口条件：

- 认证矩阵通过；
- 终端、CI 和选定 IDE 流程一致；
- 不依赖自动修改 IDE 项目文件。

## Phase 4：v0.1 公共预览

范围：

- GitHub Releases；
- SBOM、校验和、artifact attestation；
- Homebrew、WinGet、Scoop 渠道；
- 安装/卸载/迁移文档；
- 匿名错误报告仅由用户手动导出，不启用遥测。

发布门槛：

- 安全威胁模型完成评审；
- 全新机器与升级路径测试；
- 锁文件 schema 冻结；
- 许可证、商标和渠道名检查完成；
- 5–8 次观察性上手测试达到章程指标。

## v0.1 之后的候选项

只有用户证据支持时才进入：

- 更多运行时（Java、Go、Ruby、Bun 等）；
- 带能力约束的 WASM provider；
- 团队策略文件和允许的 provider/source 白名单；
- 缓存导入导出或局域网缓存；
- 显式 IDE 集成命令；
- Windows arm64、Linux arm64/musl 的扩展认证。

不因为竞品存在而自动加入：

- 任务运行器；
- Secret/环境变量管理；
- 包依赖管理；
- 云账号与同步；
- GUI。

## 用户研究计划

### 访谈（8–12 人）

招募条件：

- 最近三个月在至少两种目标运行时之间工作；
- 覆盖 Windows 主力、macOS 主力、Linux/CI；
- 至少一半经历过 PATH、版本或 IDE SDK 冲突。

重点问题：

- 最近一次版本不一致是如何发生的；
- 现有工具切换成本是否真的高；
- 锁文件、来源校验和离线是否改变决策；
- 是否愿意从 mise/fnm/uv/FVM 迁移，为什么；
- 哪些行为会让他们不信任 Pinset。

不要询问抽象的“你会不会用”，优先收集最近行为、现有替代方案和迁移成本。

### 可用性测试（5–8 人）

任务：

1. 安装 Pinset；
2. 克隆含锁文件的项目；
3. 安装三种运行时；
4. 解释当前 Node 来源；
5. 发现并处理一个故意制造的 PATH 冲突；
6. 为 Flutter IDE 找到 SDK 路径；
7. 在不破坏旧环境的情况下退出。

记录完成率、时间、错误恢复、是否能预测结果，以及需要研究者提示的次数。

## 主要风险

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| 与 mise/proto 差异不足 | 项目没有迁移动机 | 先验证冲突诊断和可验证安装，不先扩语言 |
| Flutter IDE 依赖稳定目录 | CLI 成功但真实开发失败 | Phase 0 独立 spike，失败即调整范围 |
| Python 产物不是 CPython 官方统一包 | 来源/许可证复杂 | 固定 provider 清单并保留完整元数据 |
| shim 性能或 PATH 冲突 | 每次命令都受影响 | 极小独立二进制、缓存、完整 PATH 诊断 |
| 上游元数据/密钥变化 | 安装中断或验证错误 | schema 化 provider、密钥轮换策略、集成测试 |
| 三平台测试成本 | 发布速度下降 | 认证矩阵分层，真实产物定时测试 |
| 插件扩大供应链攻击面 | 破坏信任定位 | v0.1 禁止第三方执行，未来能力沙箱 |

## 当前下一步

1. 在 macOS/Linux 运行 Spike A 测试和相同基准；
2. 比较 Windows shim、Shell activation 与稳定 PATH 别名的启动成本和语义；
3. 冻结本机 source config schema 和 `pinset source` CLI；
4. 接入一个固定 Node.js Windows x64 官方 ZIP，并用显式社区镜像验证相同 lock/哈希；
5. 准备 Python、Flutter 固定真实产物及验证规则；
6. 决定开源许可证；
7. 每个 spike 完成后写 ADR，再冻结 v0.1 实现计划。
