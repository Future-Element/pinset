# Pinset 决策记录

本文件记录已确定方向与待验证事项。`Accepted` 表示产品方向已确定；`Provisional` 表示必须由技术 spike 或用户研究确认。

| ID | 状态 | 决策 | 理由 |
| --- | --- | --- | --- |
| D-001 | Accepted | 产品名为 Pinset，命令为 `pinset` | 简短、可作为统一运行时选择品牌；分发不依赖无作用域 npm 名称 |
| D-002 | Accepted | 首版管理 Node.js、CPython、Flutter | 覆盖用户提出的三类割裂工具，同时保持可控范围 |
| D-003 | Accepted | 面向 Windows、macOS、Linux | 跨平台是核心问题，不把 Windows 留作后补 |
| D-004 | Accepted | 只管理运行时，不管理包依赖 | 避免与 uv/pnpm/pub 重叠并造成产品失控 |
| D-005 | Accepted | 项目配置是不可执行的 TOML | 降低项目进入目录或执行命令时的信任风险 |
| D-006 | Accepted | 从 v0.1 提供可提交的稳定锁文件 | 可复现与来源追踪是差异化核心，不作为后期功能 |
| D-007 | Accepted | “detect many, activate one” | 允许旧管理器共存，但命令解析必须唯一且可解释 |
| D-008 | Accepted | v0.1 只有内置 provider | 先保证供应链和跨平台质量，不开放任意脚本插件 |
| D-009 | Accepted | Rust 单一核心 + 独立轻量 shim | 适合跨平台原生分发、性能与文件系统安全实现 |
| D-010 | Provisional | 日常项目选择以 shim 为主 | 功能语义成立，但 Windows x64 spike 的 p95 额外进程开销约 18.0 ms，需与 Shell activation/显式 exec 比较 |
| D-011 | Accepted | 安装必须事务化并先校验后可见 | 防止半安装、缓存污染和中断后错误选择 |
| D-012 | Accepted | 镜像不能替换可信校验值 | 镜像是传输来源，不是信任根 |
| D-013 | Accepted | 默认无账号、无遥测 | 保持本地优先、低权限和可离线 |
| D-014 | Provisional | Windows x64、macOS x64/arm64、Linux x64 为完整认证矩阵 | 与当前 Flutter 官方稳定归档范围一致，需要持续从元数据验证 |
| D-015 | Provisional | 项目内稳定 SDK 别名解决 Flutter IDE 路径 | Windows junction 和 IDE 缓存行为必须实测 |
| D-016 | Provisional | CPython 使用固定的 python-build-standalone 清单 | 清单签名、许可证和更新流程必须完成 spike |
| D-017 | Provisional | 系统 PATH 作为未声明工具的最终透传 | 必须证明无 shim 递归、无意外遮蔽且行为可解释 |
| D-018 | Provisional | shim 热路径 p95 额外开销目标 ≤ 10 ms | 需在认证硬件基准后冻结 |
| D-019 | Accepted | 安装源是传输位置，不是信任根 | 镜像不能替换锁文件中的 canonical 产物身份、哈希、签名或 provenance |
| D-020 | Accepted | 活动安装源属于本机配置，不写入项目 lock | 国内、海外和企业内网成员应共享同一项目配置与锁文件 |
| D-021 | Accepted | 仅网络类失败允许自动源回退 | 校验失败必须硬停止，防止掩盖镜像篡改或同步错误 |
| D-022 | Provisional | 随 Pinset 发布社区镜像 preset | 需要持续可用性检查、来源分类和更新机制，且绝不默认自动启用 |
| D-023 | Accepted | 自定义安装源默认强制 HTTPS | HTTP 仅允许用户对受信任局域网源逐项显式启用；URL 不接受凭据、query 或 fragment |
| D-024 | Accepted | `official` 是不可覆盖和不可删除的内置别名 | 用户自定义源不能自行取得官方分类，避免来源身份混淆 |
| D-025 | Provisional | Node provider 首先只解析精确稳定版本 `x.y.z` | 浮动选择器必须等待可信版本索引与 lock 流程；预发布版本语义后续单独设计 |
| D-026 | Accepted | canonical URL 与有序下载候选分别构造 | active/fallback 只能改变传输位置，不能改变官方产物路径和身份 |
| D-027 | Accepted | 分发首选 GitHub Releases，不维护第三方 Homebrew Tap 或 Scoop Bucket | 避免长期维护额外仓库；中央包管理器渠道只在满足官方接收政策后评估 |

## 尚待决策

- 开源许可证：MIT、Apache-2.0 或双许可证；
- 默认数据目录的精确跨平台约定；
- Node 发布密钥轮换和离线密钥环；
- Python provider 清单的发布/签名方式；
- Flutter provenance 的自动验证层级；
- Shell profile 是否由安装器修改，还是只提供复制命令；
- `pinset.toml` 与旧版本文件冲突时的默认兼容开关；
- 缓存保留与显式清理策略；
- 首批申请哪些中央包管理器官方渠道，以及各渠道的接收时机。

## 变更规则

- Accepted 决策如需推翻，新增 ADR 说明证据和迁移影响；
- Provisional 决策必须附 spike 结果才能转为 Accepted；
- 不把竞品功能列表当作需求证据；
- 任何扩大远程执行、权限、数据收集或项目文件写入的决策都需要单独安全评审。
