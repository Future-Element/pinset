# Team development environments / 团队开发环境

Development version 2.14.0; distribution defaults remain on the published release until the final combined release.

## Requirements / 环境要求

Existing schema 5 projects keep working. Preview `pinset migrate --dry-run --json` before explicitly migrating to schema 6. A changing project migration first creates `.pinset-migration-<UUID>/pinset.toml` and, if present, `pinset.lock` with the original bytes. Preview creates neither the backup nor a migrated file. Keep that local backup outside version control. To return to the old configuration, review and restore both original files together; do not merely change the schema number after adding new fields. Global migration does not use this project backup directory.

旧 schema 5 项目继续可用；先用 `pinset migrate --dry-run --json` 预览，再明确运行 `pinset migrate` 升到 schema 6。有变化的项目迁移会先把原始配置和已有锁文件逐字节备份到 `.pinset-migration-<UUID>`。预览不创建备份或修改文件。备份属于本地恢复材料，不应提交。回退时核对并一起恢复两个原文件，不能给包含新字段的配置只改 schema 数字。全局迁移不使用这个项目备份目录。

```toml
[requirements]
platforms = ["windows-x86_64", "linux-x86_64", "macos-aarch64"]
build-targets = ["android"]
# Only disable an individually reviewed rule, using its exact report ID:
# disabled-rules = ["compatibility.java.android/gradle.r1"]
```

`platforms` requires every selected SDK to have a locked upstream artifact for each listed host. It accepts Windows x64, Linux x64/ARM64 and macOS x64/ARM64. It does not claim that every Pinset CLI distribution or IDE entry has been tested on those hosts. A workspace member inherits root requirements or replaces the entire requirements object with its own declaration.

`platforms` 要求每个选定 SDK 都有对应主机的已锁定上游制品，支持 Windows x64、Linux x64/ARM64 和 macOS x64/ARM64；不代表 Pinset 安装包或 IDE 入口已完成该平台验收。工作区成员继承根要求，或用自己的整个 `[requirements]` 覆盖。

`build-targets` opts into Android, iOS, macOS, Windows or Linux build-condition checks. These inspect SDK/compiler footprints only. Android license records do not prove license acceptance; Xcode first-launch/licensing, GTK package compatibility, devices, signing and application builds need explicit native diagnostic/build tasks. Pinset never installs platform SDKs or accepts licenses through these checks.

`build-targets` 显式启用平台构建条件检查，只检查 SDK/编译器文件是否存在。Android 许可记录不证明许可已接受，Xcode 首次启动/许可、GTK 版本、设备、签名及应用构建仍需明确的原生诊断/构建任务。检查不会安装平台 SDK 或代为接受许可。

## Compatibility / 兼容性

`pinset status --report-version 2` collects static results without executing SDKs or decrypting profiles. `pinset check --probe` explicitly verifies the selected environment and runs bounded runtime probes. Node and Python report their executable path; Go, Rust, Java and .NET currently provide separate `managed-version` evidence without claiming a self-observed executable path. Flutter's mutable cache requires a declared diagnostic task. Probe success covers only that invocation.

`pinset status --report-version 2` 只收集静态结果，不执行 SDK、不解密 profile。`pinset check --probe` 明确检查所选环境并执行有超时/输出上限的探测。Node/Python 可报告自己的可执行路径；Go、Rust、Java 和 .NET 分别记录 `managed-version` 版本证据，不宣称进程自行确认了路径。Flutter 检查会写入缓存，需通过声明的诊断任务执行。探测通过只证明这次调用。

| Declaration / 声明 | Check / 检查 |
| --- | --- |
| Node `package.json#engines.node` | Bounded stable semver ranges; bundled npm checks its own `engines.node`. npm stays part of Node. / 检查稳定版范围及 Node 自带 npm 的要求，不独立选择 npm。 |
| Python `pyproject.toml#project.requires-python` | Stable PEP 440 comparisons, compatible releases and exclusions. / 稳定版 PEP 440 范围、兼容版本及排除规则。 |
| Java/Gradle/AGP | Wrapper Java matrix through Gradle 9.7; literal AGP 8 declarations require Java 17+. Dynamic declarations remain unknown. / 固定 wrapper 矩阵及 AGP 8 的最低 JDK，动态脚本保持未知。 |
| Go `go.mod`/`go.work` | Minimum `go` and `toolchain` declarations; report automatic toolchain or external workspace overrides. / 最低版本与 toolchain、自动下载或外部工作区覆盖。 |
| Rust `rust-toolchain[.toml]` | Stable channel, declared components and targets against locked options/expanded manifest; floating/custom selection remains unknown. / 稳定通道、组件和目标；浮动或自定义选择保持未知。 |
| .NET `global.json` | Stable SDK feature bands and supported `rollForward`; custom paths or unrecognized policy remain unknown. / 稳定 SDK 特征带及 rollForward，未支持的自定义选择保持未知。 |

Results use `pass`, `fail`, `unknown`, and `not_applicable`. Every compatibility rule includes a revision and primary reference. Unsupported syntax and future matrix versions remain unknown. `disabled-rules` changes only the named rule to not-applicable and reports that choice; it cannot bypass lock integrity, platform completeness, environment trust or installation ownership. Setup stops before installation when a known compatibility/platform conflict exists.

结果分为通过、失败、未知和不适用；兼容性规则附修订日期及一手来源。无法解析的声明和超出矩阵范围的新版本保持未知。`disabled-rules` 只关闭明确命名的一条规则并记录原因，不能绕过锁完整性、平台制品、环境信任或安装所有权。准备环境时，已知兼容性/平台冲突会在安装前阻止执行。

Primary references: [npm engines](https://docs.npmjs.com/cli/v11/configuring-npm/package-json#engines), [PEP 440](https://packaging.python.org/en/latest/specifications/version-specifiers/), [Gradle Java matrix](https://docs.gradle.org/current/userguide/compatibility.html), [AGP 8](https://developer.android.com/build/releases/agp-8-0-0-release-notes), [Go toolchains](https://go.dev/doc/toolchain), [rustup overrides](https://rust-lang.github.io/rustup/overrides.html), [.NET global.json](https://learn.microsoft.com/en-us/dotnet/core/tools/global-json).

## Delivery and comparison / 交付与比较

```sh
pinset -e dev check --delivery --json
pinset status --report-version 2 --save alice.json
pinset check --delivery --compare alice.json --json
pinset check --network --target linux-x86_64 --json
pinset cache prefetch --target linux-x86_64,windows-x86_64
pinset check --offline --target linux-x86_64,windows-x86_64 --json
pinset bundle export --target linux-x86_64 --output linux.bundle.tar.gz
pinset bundle import linux.bundle.tar.gz
pinset setup --yes --offline
```

The team checklist distinguishes configuration, artifact/routing checks, trust, registered versus usable identity, and each applicable variable contract. Background status never opens identities. Explicit checks may decrypt a trusted selected profile in memory, validate it, and zeroize returned values; no values, defaults, secret digests or private keys are exported. Missing keyring credentials and non-interactive encrypted identity files still require the existing explicit identity flow.

团队清单分别报告配置、制品/路由、信任、已注册与实际可用的身份，以及每个适用变量的约定。后台状态不打开身份；显式检查可在内存中解密已信任的所选 profile、校验并清除返回值。导出不含值、默认值、秘密摘要或私钥。缺少密钥环凭据、非交互场景使用加密身份文件时，仍需按现有身份流程准备。

Report comparison matches runtimes by name and compares exact versions, options, full artifact identities, public project/profile identity, requirements and variable contracts. Ordering and machine paths are ignored. Declared cross-platform differences are separate from configuration differences. A legacy report lacking artifact identities produces an incomplete-comparison result. Independent successful probes do not establish identical secret values or equivalence of unobserved execution. Comparison findings are returned as data; delivery exit status describes the selected readiness check.

报告按运行时名称比较精确版本、选项、完整制品身份、公开项目/profile 标识、环境要求和变量约定；忽略排列顺序和机器路径。已声明的平台差异单独列出。旧报告缺少制品身份时会标记比较不完整。两次探测通过不证明秘密值相同，也不证明未观察入口等价。差异记录在数据中；交付命令退出状态表示本次选择的就绪检查结果。

`--network` sends bounded HEAD requests only to each locked artifact's selected, automatic official and configured fallback sources, stopping on success. Merely registered sources are not contacted. HTTP success proves reachability, not archive integrity. DNS, proxy/connect/authentication, TLS, timeout, missing artifact and server failures are classified without exporting private source URLs. An unsupported HEAD returns unknown. At most 32 source attempts run, with a 15-second limit each.

`--network` 只按既有顺序检查锁定制品的当前源、自动官方回退和已配置回退；成功即停止，不枚举仅注册的源。HTTP 成功只证明可达，不证明归档完整。DNS、代理连接/认证、TLS、超时、制品不存在和服务端失败分别报告，不导出私有源地址。不支持 HEAD 时为未知；最多 32 次尝试，每次上限 15 秒。

Set `PINSET_CA_BUNDLE` to a regular PEM certificate bundle of at most 1 MiB to add organization roots to normal trust. It applies to metadata, SDK archives, source diagnostics and self-update for the current process. Configure `HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY` at process scope. TLS certificate/hostname verification remains enabled.

把 `PINSET_CA_BUNDLE` 指向不超过 1 MiB 的普通 PEM 文件即可补充企业根证书，作用于当前进程的元数据、SDK 下载、来源检查和自更新。代理通过进程级 `HTTP_PROXY`、`HTTPS_PROXY`、`NO_PROXY` 设置，继续验证 TLS 证书及主机名。

`--offline` hashes every required platform artifact and overlay without network requests. Prefetch defaults to declared platforms, then the current host. A portable Bun x64 target requires both baseline and AVX2 archives. Bundle imports reject missing/extra entries, inconsistent manifests and corrupt content before adding cache entries. SDK delivery does not include npm/pip/Maven/Gradle project dependencies, Android/Xcode installations or application build outputs; prepare those caches through explicit project tasks.

`--offline` 不联网，逐一校验所需平台的基础归档与覆盖归档。预取默认使用声明的平台，再回退当前主机。可移植 Bun x64 包需同时包含 baseline 和 AVX2。离线包缺失/多出条目、清单不一致或内容损坏时，在写入缓存前拒绝。SDK 交付不涵盖 npm/pip/Maven/Gradle 项目依赖、Android/Xcode 或应用构建产物；相关缓存通过明确的项目任务准备。

The Action's optional `prepare: "true"` runs setup after the requested project trust step; `environment-report: "true"` saves a portable report and emits a small job summary. Both require a release with the environment commands. They default to false while the Action's public version default remains 2.12.3. Summary output contains only readiness booleans and runtime/version/target, not decrypted values or paths.

Action 可选 `prepare: "true"` 在指定信任步骤后执行准备；`environment-report: "true"` 保存可移植报告和简短作业摘要。这两项需要包含新环境命令的版本，当前默认关闭，公共版本默认值仍为 2.12.3。摘要仅含就绪布尔值及运行时/版本/平台，不含解密值或路径。
