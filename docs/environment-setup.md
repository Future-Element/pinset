# Project environment preparation / 项目开发环境准备

Development version: 2.13.0. Public installers continue to install the published version until the combined roadmap release is complete.

## Prepare / 准备

```sh
pinset setup --plan
pinset setup
pinset setup --yes --json
pinset setup --resume <run-id> --yes
pinset setup --yes --offline
pinset -e dev setup --yes --task test
```

`--plan` reads configuration, locks and discovery sources without downloads, decryption, task execution or writes. Conflicts require an explicit `pinset use` selection. Setup retains existing lock versions and installation identities. Node supplies its bundled npm; setup does not independently select npm.

`--plan` 只读预览配置、锁及传统版本来源，不下载、不解密、不执行任务、不修改项目。冲突需明确 `pinset use` 选择，已有锁定版本继续沿用。npm 随 Node 安装，不独立选择。

Execution installs or revalidates locked SDKs, prepares owned Python environments and checks the selected variable contract. It never adopts an unmarked `.venv`. Non-interactive execution requires `--yes`. Failures retain a UUID run record under `PINSET_HOME/state/setup`; resume revalidates the canonical project directory, platform, effective configuration and locks. Changed inputs require a fresh plan. Offline setup requires existing locks and verified cached artifacts. `--task` explicitly runs a declared task after preparation succeeds.

执行时安装或校验 SDK、准备有所有权记录的 Python 环境、检查变量契约，不接管未标记的 `.venv`。非交互执行需 `--yes`。失败后记录运行编号；恢复时重新验证目录、平台、有效配置及锁，输入变化则拒绝旧计划。离线准备要求已有锁与验证过的缓存。`--task` 仅在准备成功后显式运行声明任务。

## Inspect / 检查

```sh
pinset status --report-version 2 --json
pinset check --report-version 2
pinset check --probe --json
pinset status --report-version 2 --save environment.json
pinset status --report-version 2 --compare environment.json
pinset editor context --protocol 2 --json
```

Report and editor protocol v1 remain the default. Four states are distinct: `pass`, `fail`, `unknown`, `not_applicable`. `environment_ready` requires applicable preparation checks to pass; it is not proof of execution. `execution_verified` covers only the listed observations. Controlled Node/Python probes disable startup hooks and enforce time/output limits. Background collection never runs builds or decrypts profiles.

Protocol 2 checks configuration, lock metadata, receipt ownership and routing without repeatedly hashing downloaded archives. Cache integrity remains an explicit `pinset cache verify` / `pinset lock audit` check and is validated before installation. Installed-environment readiness does not claim offline cache completeness.

旧报告与编辑器协议 1 保持默认。通过、失败、未知、不适用分别报告。“环境就绪”不等于“执行已验证”，执行结论仅覆盖列出的观察入口。Node/Python 探测禁用启动钩子并限制时长及输出。后台收集不运行构建、不解密 profile。

Portable reports remove local paths and fingerprints, do not include task arguments or secret values, and never compare secret-value hashes. Editor context and setup journals are local records containing paths, not portable exports. Explicit preparation/check operations distinguish trust, identity and variable contract errors.

分享报告移除本机路径与本地指纹，不包含任务参数、秘密值或其哈希。编辑器上下文和 setup 日志是含路径的本地记录，不应视为分享报告。显式准备/检查会分别报告信任、身份、变量契约错误。

## Editor / 编辑器

Extension 1.2 adds an Explorer environment panel per folder. The CLI runs in the workspace extension host, including remote hosts. Commands prepare, probe and bind Node debug configurations, Python active interpreters or Flutter SDKs. Binding previews show exact before/after values. Writes are folder scoped. Restore only changes fields still equal to Pinset's last write, preserving later user changes.

插件 1.2 增加各文件夹独立的环境面板，CLI 在工作区扩展宿主运行，远程环境不会套用本机路径。可准备、探测、绑定 Node 调试配置/Python 活动解释器/Flutter SDK。绑定前预览字段差异；恢复仅处理仍等于 Pinset 写入值的字段，保留用户后续修改。

Reopen existing terminals after binding. Missing language APIs/extensions are unavailable, never a pass. Controlled native debugger probes do not load encrypted profiles, clear Pinset identity fields and prove only their own launch configuration. They retain ordinary host environment variables. User debug/test configurations and existing terminals remain unverified until separately observed.

绑定后需重新打开终端。缺少语言扩展/API 时不报告通过。受控原生调试探测不加载加密 profile，并清除 Pinset 身份字段；仍会继承宿主的常规环境变量，只证明其自身启动配置。用户调试/测试配置与旧终端需分别验证。

Adapter references: [Python public API](https://github.com/microsoft/vscode-python/blob/main/src/client/api/types.ts), [Node debug options](https://github.com/microsoft/vscode-js-debug/blob/main/OPTIONS.md), [Dart Code SDK settings](https://dartcode.org/docs/settings/#dartfluttersdkpath). API support and runtime execution are validated separately.

## Validation and rollback / 验证与回退

Builds, formatting and tests run in local Docker before consolidated merge checks. Platform-specific native acceptance covers Windows x64, Linux x64/ARM64 and macOS ARM64; Linux containers do not replace Windows/macOS execution evidence. Native SDK/shell acceptance and real VS Code debug observations are separate tests. WSL/SSH/container host detection does not imply a tested remote debug session.

No intermediate release tags are created. Reverting a development PR restores old behavior; old protocol consumers retain v1. Receipt-owned installations can be removed through existing reference-aware uninstall/prune commands. Configurations created during setup remain explicit project files and are not silently deleted after a failed run.
