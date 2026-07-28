# Pinset 深度调研

调研日期：2026-07-28  
证据策略：产品行为和技术机制优先采用官方文档、官方仓库与官方机器可读索引；社区讨论只用于发现问题，不作为已确认结论。

## 1. 市场结论

“一个工具管理多种运行时”已经是成熟赛道。asdf、mise、proto 和 vfox 都覆盖多语言，因此仅仅统一命令不足以形成产品价值。

Pinset 值得继续的前提是把差异化收窄到：

- Node.js、CPython、Flutter 三个生态的高质量内置支持；
- 从首版开始稳定的、包含产物来源与校验信息的锁文件；
- 完全不可执行的项目配置；
- Windows、PATH 和旧管理器冲突的一等诊断体验；
- 镜像、代理和离线缓存下仍保持来源校验；
- 对 CI、终端和 IDE 的选择结果给出一致且可解释的路径。

如果访谈和可用性测试显示用户认为 mise/proto 已经充分解决这些问题，Pinset 应停止扩张语言范围，转向“迁移/诊断工具”或终止项目，而不是用更多功能掩盖需求不足。

## 2. 竞品比较

| 工具 | 核心机制 | 优势 | 对 Pinset 的启示 |
| --- | --- | --- | --- |
| [asdf](https://asdf-vm.com/guide/introduction.html) | `.tool-versions`、shim、Shell 脚本插件 | 生态成熟、约定广泛 | 插件脚本灵活但扩大执行面；Pinset v0.1 不开放任意脚本插件 |
| [mise](https://mise.jdx.dev/dev-tools/) | 多后端、Shell activation/shim、环境与任务 | 功能和工具覆盖面非常广 | 不在广度上竞争；稳定锁文件和配置信任模型是关键 |
| [proto](https://moonrepo.dev/docs/proto) | Rust 核心、WASM 插件、shim、锁文件 | 跨平台和供应链设计完整 | WASM 能作为未来扩展方向，但 v0.1 先做内置适配 |
| [vfox](https://vfox.dev/guides/intro.html) | 原生跨平台、Lua 插件、项目配置 | Windows 体验与扩展性好 | 再次证明“多语言”不是空白；Pinset 必须以可预测性和迁移为主 |
| [fnm](https://github.com/Schniz/fnm) | Rust、Shell 环境初始化、`.nvmrc` | Node 专用、快速、跨平台 | 旧配置兼容有价值，但 Shell hook 不是唯一方案 |
| [nvm-windows](https://github.com/coreybutler/nvm-windows) | Windows 目录符号链接切换 | Windows 用户基础大 | PATH 冲突、权限和既有 Node 遮蔽必须由 `doctor` 清楚展示 |
| [uv](https://docs.astral.sh/uv/concepts/python-versions/) | 管理 Python 构建、项目版本文件 | Python 体验和性能优秀 | Pinset 不接管 Python 依赖/虚拟环境；uv 是互补工具 |
| [FVM](https://fvm.app/) | Flutter SDK 缓存与项目选择 | Flutter 场景专注 | IDE 需要稳定 SDK 目录，不能只解决终端命令 |

竞品事实边界：

- mise 的项目配置可包含环境、任务和钩子，因此提供了信任机制；这不等于 mise “不安全”，而是与 Pinset 的纯数据配置模型不同。
- asdf/vfox 的插件机制并非缺陷；Pinset 暂不支持第三方插件是首版供应链范围选择。
- 本调研未使用 GitHub star 数量判断需求，GitHub API 查询受到速率限制，而且 star 本身不能证明迁移意愿或付费需求。

## 3. 官方运行时来源

### 3.1 Node.js

可使用的官方能力：

- [`dist/index.json`](https://nodejs.org/dist/index.json) 提供版本与目标产物索引；
- 官方发布目录提供压缩包和 `SHASUMS256.txt`；
- Node.js 官方仓库说明发布校验清单可用发布密钥验证，密钥在 [nodejs/release-keys](https://github.com/nodejs/release-keys) 管理；
- [发布周期页面](https://nodejs.org/en/about/previous-releases) 可用于解析 LTS/Current 别名。

方案结论：

- 只安装官方预编译包，不在 v0.1 本地编译 Node；
- 解析别名后必须在锁文件中写入精确版本和目标产物；
- 先验证签名清单，再验证产物 SHA-256；
- 镜像只能替换下载位置，不能替换预期哈希或授权签名身份。

待验证：

- 发布密钥轮换和离线密钥环的更新策略；
- 少数历史版本与当前目标平台的产物差异。

### 3.2 CPython

CPython 官方并不为所有目标平台提供统一的可搬运预编译包。uv 采用 Astral 维护的 [`python-build-standalone`](https://github.com/astral-sh/python-build-standalone/releases)，其[发行说明](https://gregoryszorc.com/docs/python-build-standalone/main/distributions.html)描述了 install-only 归档和 `PYTHON.json` 元数据。

方案结论：

- v0.1 使用 python-build-standalone，而不是把系统 Python 或 Python.org 安装器包装成统一格式；
- 首版支持现代 CPython 3.10–3.14 的常规构建，暂不承诺 PyPy、debug、free-threaded 和历史 GPL 依赖构建；
- Pinset 下载而不重新托管 Python 产物，但仍需保留上游来源、许可证元数据和归档校验；
- Python 依赖与虚拟环境继续交给 uv/pip，Pinset 只提供解释器。

待验证：

- 采用“随 Pinset 发布固定清单”还是“签名远程注册表”；
- 各平台证书、`pip` 引导和动态库行为；
- 许可证展示与缓存清理的产品交互。

### 3.3 Flutter

[Flutter SDK archive](https://docs.flutter.dev/install/archive) 提供 Windows、macOS、Linux 的官方 bundle。三大平台的 release JSON 包含版本、channel、Git 引用、归档路径和 SHA-256，例如：

- `https://storage.googleapis.com/flutter_infra_release/releases/releases_windows.json`
- `https://storage.googleapis.com/flutter_infra_release/releases/releases_macos.json`
- `https://storage.googleapis.com/flutter_infra_release/releases/releases_linux.json`

调研时观察到的稳定产物矩阵：

- Windows：x64
- macOS：x64、arm64
- Linux：x64

这是 2026-07-28 的上游快照，不应硬编码为永久事实；Pinset 必须以 release JSON 动态判断。

方案结论：

- v0.1 只认证 stable channel 的精确版本；
- Flutter bundle 自带 Dart，锁文件记录这一依赖关系；
- CLI 命令由 shim 路由，但 IDE 集成还需要稳定的 SDK 目录；
- beta/main、自建引擎和源码编译延后；
- 中国网络和企业镜像使用可配置 base URL，但校验仍来自可信上游元数据。

待验证：

- Windows 目录 junction、Unix 符号链接作为项目稳定 SDK 路径的 IDE 兼容性；
- Flutter 官方 provenance/attestation 的自动验证接口；
- Android Studio 与 VS Code 在 SDK 路径切换后的缓存行为。

## 4. 版本管理机制研究

### 4.1 Shell activation 与 shim

Shell activation 可以在每次提示符或目录变化时更新 PATH 和环境变量，但它依赖各 Shell 的初始化脚本，也无法改变已经启动且未加载集成的终端。shim 则在每次执行 `node`/`python`/`flutter` 时，根据当前目录解析版本。

Pinset 选择：

- 日常命令路由以 shim 为主，只需把一个 Pinset shim 目录加入 PATH；
- 项目选择无需每次 `cd` 执行 Shell hook；
- `pinset exec` 为 CI 和临时覆盖提供显式入口；
- 安装过程若修改 Shell 配置，必须先展示具体文件和变更，并诚实提示已有终端需要重启或手动加载。

### 4.2 配置优先级

拟定优先级：

1. 当前命令的显式覆盖（例如 `pinset exec node@22 -- ...`）；
2. 从当前目录向上找到的最近 `pinset.toml`；
3. 当 Pinset 未声明该工具时，可选读取兼容的旧版本文件；
4. Pinset 全局选择；
5. 安全透传到系统 PATH 中的非 Pinset 命令。

最后两项必须通过原型验证，避免 shim 自递归或意外遮蔽系统安装。

### 4.3 锁文件

锁文件不是“当前机器状态”的导出，而是可复现解析的契约，至少记录：

- 用户请求的选择器与解析后的精确版本；
- provider 和目标三元组；
- 下载 URL、SHA-256、签名/证明引用；
- 上游版本引用与发布时间（若提供）；
- 运行时之间的打包依赖，例如 Flutter 自带 Dart；
- lock schema 与生成器版本。

`pinset install --locked` 不得访问“最新版本”重新解析；锁文件与配置不一致时直接失败。

## 5. 迁移与共存

Pinset 将读取但不自动改写以下来源：

- Node：`.nvmrc`、`.node-version`；
- Python：`.python-version`；
- Flutter：`.fvmrc`（必须先用真实 fixture 验证 schema）；
- 管理器：fnm、nvm/nvm-windows、uv、FVM、mise、asdf、vfox；
- 系统或手工安装的 `node`、`python`、`flutter`。

`pinset import --dry-run` 输出拟议的 `pinset.toml`，由用户确认后才写入。`doctor` 输出 PATH 中每个同名命令、所属管理器、被选择原因和可逆修复建议。它不会卸载旧工具或批量重写 Shell 配置。

## 6. 安全与分发调研

Pinset 自身发布计划：

- GitHub Releases 提供各目标平台原生二进制；
- 发布 SHA-256、SBOM 和 [GitHub artifact attestation](https://docs.github.com/en/actions/concepts/security/artifact-attestations)；
- WinGet/Scoop/Homebrew 配方固定到已校验发布物；
- 安装脚本不是唯一分发方式，并提供可审计的手工安装路径。

需要明确：artifact attestation 可以把构建产物与工作流/源码关联起来，但不等于证明代码本身没有漏洞。

运行时安装威胁模型至少包含：

- 哈希不匹配、签名身份混淆和被替换的镜像；
- Zip Slip/Tar 路径穿越、绝对路径、符号链接/硬链接逃逸；
- 压缩炸弹、磁盘耗尽和超长路径；
- 下载中断、并发安装、进程崩溃和锁文件损坏；
- 企业 TLS 代理、自定义 CA、离线缓存污染；
- 卸载误删超出 Pinset 数据目录的内容。

### 6.1 国内镜像与企业代理

[Flutter 官方中国使用指南](https://docs.flutter.dev/community/china)明确列出 CFUG、上海交通大学和清华 TUNA 的社区镜像，同时强调 Flutter 团队不能保证镜像的可靠性或安全性。官方指南的 URL 替换方式证明“保持相同 artifact path，只替换 storage base URL”适合 Pinset 的 provider 模型。

[uv 环境变量文档](https://docs.astral.sh/uv/configuration/environment/)提供 `UV_PYTHON_INSTALL_MIRROR`，用镜像 URL 替换 python-build-standalone 的 release base；其可用版本清单仍随 uv 发布。这支持 Pinset 将可信产物清单与传输镜像分离。

Node.js 没有在官方发布文档中为中国指定唯一镜像。npmmirror/cnpm 提供国内二进制加速，但应标记为社区第三方来源，而不是“Node 官方中国源”。

产品结论：

- 内置 `official` 源始终存在；
- 可提供经过版本发布时检查的社区 preset，但不自动启用；
- 用户可以添加企业 Nexus/Artifactory、反向代理或其他 HTTPS 镜像；
- 本机源选择不改变 lock；
- 镜像下载仍使用 canonical 产物的官方/可信哈希；
- 网络失败可回退，校验失败不可回退；
- 不做地理位置探测或后台测速后静默换源。

## 7. 名称与渠道风险

- 产品名与命令名：Pinset / `pinset`；
- npm 已存在无作用域同名旧包，因此不能将 `npm install -g pinset` 作为官方渠道；
- 如需 npm 包，使用类似 `@pinset/cli` 的作用域名称；
- PyPI 名称在调研时未发现现有项目，但 Pinset 不需要通过 PyPI 分发；
- crates.io 查询未能完成，不能声称名称可用；
- 域名、组织名和商标尚未完成正式检索，公开发布前必须单独处理。

## 8. 已验证与未验证

已完成静态/官方来源验证：

- 主要竞品的配置、shim/activation 和插件机制；
- Node.js 官方索引与校验清单机制；
- python-build-standalone 的分发形态和元数据；
- Flutter 三平台 release JSON 的字段和当前架构范围；
- GitHub artifact attestation 的能力边界。

尚未完成：

- 真实三平台安装与性能测试；
- IDE 对 Flutter 稳定 SDK 别名的兼容性；
- 企业代理、离线镜像和自定义 CA 集成；
- 旧管理器共存的真实机器样本；
- 用户访谈、行为观察、使用留存和付费意愿；
- 商标、域名和所有包管理渠道的法律/名称清查。
