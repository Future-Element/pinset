# Pinset v1.5 横向对比

调研日期：2026-08-19。对比只使用各产品官方文档或官方仓库，并区分“运行时版本管理器”和“完整开发环境管理器”。后者解决的问题更大，不应直接当作 Pinset 的功能缺口。

## 直接同类

| 产品 | 定位与路由 | 传统配置 | 可复现锁 | 扩展方式 | Pinset 相比之下 |
| --- | --- | --- | --- | --- | --- |
| Pinset v1.5 | 9 个内置 Provider；项目默认严格，项目外才按全局、系统顺序解析 | 只由显式 `detect` / `import` 读取 | schema 3 始终分离 requested selector 与 exact version，并记录平台制品与完整性 | 当前为内置 Provider 清单 | 更保守、可解释、供应链边界统一；生态宽度较小 |
| [proto](https://moonrepo.dev/docs/proto/detection) | CLI/环境变量、`.prototools`、生态文件、全局的上下文解析 | 日常 shim 会自动读取 `.nvmrc`、`package.json` 等 | [`.protolock`](https://moonrepo.dev/docs/proto/lockfile) 记录 spec、exact version、checksum、OS/arch，但目前标为 opt-in/unstable | 插件/后端体系 | proto 的扩展性和日常兼容更强；Pinset 不会被传统文件隐式改变运行时，且项目缺项默认失败关闭 |
| [mise](https://mise.jdx.dev/dev-tools/shims.html) | 工具、环境、任务和多种 backend 的综合平台；支持 PATH 激活、shim、exec/run | 广泛兼容生态配置 | [mise.lock](https://mise.jdx.dev/dev-tools/mise-lock.html) 可锁 exact version、URL、checksum，但创建需启用设置或显式命令，完整性能力取决于 backend | 大型 backend/plugin 生态 | mise 覆盖面和集成远强；Pinset 的默认系统回退更安全、协议更窄且更容易审计 |
| [asdf](https://asdf-vm.com/guide/introduction.html) | `.tool-versions` + shim 的经典多语言管理器 | 以自身统一配置为主 | 官方简介强调精确版本共享，未提供与 Pinset 同层级的平台制品锁模型 | 社区插件是核心 | asdf 成熟、工具覆盖广；Pinset 的制品身份、来源信任和锁验证更统一 |
| [Volta](https://docs.volta.sh/guide/understanding) | JavaScript 专用，自动按目录路由 Node/npm/Yarn/包命令 | 读取项目内 Volta 配置 | 项目 pin 与包命令绑定 Node engine，范围比通用运行时锁更窄 | 聚焦 JavaScript，不追求通用插件 | Volta 在 JS 体验和包命令上更深；Pinset 适合多语言仓库并提供统一供应链策略 |
| [vfox](https://vfox.dev/guides/intro.html) | Windows/Linux/macOS 原生、项目/会话/全局作用域与自动切换 | 日常兼容 `.node-version`、`.nvmrc`、`.sdkmanrc` | 本次官方资料未发现等价的平台制品锁协议 | Lua 插件生态 | vfox 的 Windows 体验、Shell 覆盖和可扩展性很有竞争力；Pinset 的锁与来源验证边界更明确 |

## 邻近产品

| 产品 | 它真正解决的问题 | 值得借鉴，但不应直接照搬 |
| --- | --- | --- |
| [aqua](https://aquaproj.github.io/docs/reference/security/checksum/) | 以安全安装 CLI 工具为中心，支持制品和 Registry 校验 | Registry 校验、自动生成 checksum 配置、CLI 工具目录；Pinset 不必扩张成通用包管理器 |
| [Devbox](https://jetify.mintlify.app/docs/devbox/quickstart/index) | 基于 Nix 的隔离开发 Shell，`devbox.json` + `devbox.lock` 覆盖整个工具环境 | 一条命令进入可复现环境、IDE/direnv 集成；代价是更重的 Nix 与子 Shell 模型 |
| [Flox](https://flox.dev/docs/concepts/environments) | 可组合、可共享的 Nix 环境，包含软件包、变量、脚本、服务和远端环境 | 环境分层、中心共享、manifest/lock；这属于 Pinset 目前有意不覆盖的平台级范围 |

## Pinset 的现实优势

1. **默认失败关闭。** 项目存在但未声明工具时，不会静默继承全局版本或命中系统同名程序；mise 官方文档明确说明 shim 在默认设置下可能自动安装或回退系统命令，Pinset 的行为更适合 CI、审计和高要求团队。
2. **迁移输入与运行时输入分离。** proto、vfox 的便利来自日常自动兼容传统文件；Pinset 将它们限制在显式只读检测/导入，因此普通 shim 的结果只由 Pinset 配置、锁和明确策略决定。
3. **一个受控供应链实现。** Provider 的元数据、可信来源、平台制品、摘要、解压、缓存、收据和卸载所有权由同一核心约束；插件型产品往往把最终保证交给不同 backend/plugin。
4. **Windows 是一等平台。** Pinset 的小型独立 shim、PowerShell 补全和 Windows 原生目标不是 Unix Shell 方案的附带移植。
5. **v1.5 的 selector/lock 语义清晰。** 配置回答“团队允许什么”，锁回答“这次实际用了什么”；`outdated`、`update`、`migrate` 使用同一语义。

## 当前缺点

1. **Provider 数量和插件生态不足。** proto、mise、asdf、vfox 都能让外部作者扩展工具；Pinset 新增 Provider 仍需进入主仓库、随主程序发布。
2. **选择器语法较窄。** 当前擅长精确版本、major/minor 前缀和 `latest`/`lts`/`stable` 等通道，还不是 mise/proto 那样完整、跨工具一致的版本要求语言。
3. **便利性有意让位于确定性。** 用户必须 `detect`/`import`，不会像 proto/vfox 一样放下 `.nvmrc` 就自动生效；需要通过提示、`doctor` 与初始化体验降低摩擦。
4. **不管理完整开发环境。** 没有 mise 的变量/hooks/tasks，也没有 Devbox/Flox 的系统包、服务和隔离 Shell。这既是范围控制，也是对需要“一键完整环境”团队的劣势。
5. **集成与分发仍薄。** IDE、direnv、Dev Container、Homebrew/Winget/Scoop、企业 Registry 和公开采用案例都落后于成熟产品。

## 下一阶段最值得学的内容

1. 设计**受能力限制的 Provider 扩展协议**：声明命令、版本语法、元数据源、制品和验证能力；第三方扩展不得绕过 HTTPS、完整性、路径和所有权规则。
2. 让解释能力覆盖失败路径：`which --explain` 即使因严格策略、锁缺失或未安装失败，也应输出结构化候选链和稳定 reason code。
3. 为所有 Provider 建立统一 selector AST 与兼容性测试，再考虑 `>=`、`^`、`~`、日期版本等语法；不要把字符串范围判断散落到 Provider 中。
4. 借鉴 proto/mise 的 lock audit：检查 stale record、重复 selector/platform、缺少 provenance、配置已删除但锁仍残留，并提供只读修复计划。
5. 借鉴 aqua 的 Registry/checksum 思路，对 Provider 清单、镜像策略和扩展包本身做签名或摘要验证。
6. 优先补齐 IDE/direnv/Dev Container 与主流包管理器分发，而不是把 Pinset 扩张成任务运行器或 Nix 环境平台。
