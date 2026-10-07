# Pinset 3

[English](README.md) · [简体中文](README.zh-CN.md) · [完整命令](docs/commands.zh-CN.md)

Pinset 管理并锁定项目工具链，解释实际执行入口。3.0 使用全新协议，不兼容旧文件和旧命令；源码版本不代表已经发布。

保留八个内置 Provider：Node、pnpm、Bun、Go、Python、完整 Temurin OpenJDK、Rust、Flutter/Dart。npm 随 Node，pip 随根目录标准库 `.venv`，Dart 随 Flutter。没有 task、workspace 编排、第三方 Provider、镜像配置或离线 bundle。

```text
pinset init
pinset use node@lts pnpm@10 python@3.13 java@21
pinset which javac --explain
pinset check --probe
pinset exec -- ./mvnw verify
pinset use java@25
pinset exec -- ./mvnw verify
```

## 文件和执行边界

提交 `.pinset/config.toml`、`.pinset/lock.toml` 和 `.pinset/env/<profile>.env`；只忽略 `.pinset/local/` 与根目录 `.venv/`。全局数据位于 `PINSET_HOME/v3/`，默认 `~/.pinset/v3/`。旧数据不会被接管或删除。

Git 项目最多向上查找到最近仓库根，支持 worktree；非 Git 项目最多查找到主目录，主目录之外只检查指定目录。最近项目配置独立生效，不合并父配置或全局配置。项目缺少受管工具选择时明确拒绝执行；无项目时才使用明确设置的全局默认。

`use` 解析官方制品并事务更新选择和锁；`install` 严格按现有锁安装。缓存和离线安装继续校验摘要。`remove` 只取消选择；SDK 由保守引用保护的 `clean` 清理。shim 与 `exec` 不自动安装工具，不发起隐式下载。原生工具自身的联网与显式外部 toolchain 不属于执行沙箱。

## 完整语言能力

Java 使用完整 Temurin JDK，保留该版本与平台提供的公开工具、源码、JNI 头文件、模块、原生库、证书和许可证。普通 Java 应用、服务端、编译、打包、文档、调试、JFR 和运行时镜像均属于独立使用场景。Maven/Gradle 使用项目 wrapper；构建运行 JDK、编译 toolchain 和字节码目标分别报告。IDE 项目 JDK 与语言服务器 JDK 分开设置。Flutter Android 另做集成验收。

Python 使用锁定解释器的标准库创建根目录 `.venv`，提供 pip，禁止继承系统和用户包；没有 uv。解释器变化时保留旧环境，需明确 `install --recreate-venv`，只重建有效的 Pinset 环境，并保留备份。工具锁不负责应用依赖锁定。

Rust 支持 channel、日期、profile、components 和 targets；Flutter/Dart 共用安装身份，稳定 SDK 路径是 `.pinset/local/flutter-sdk`。Android 检查实际 JDK、Gradle、AGP 和 Android SDK，不修改全局 Flutter JDK 设置。

工具命令入口始终位于 `PINSET_HOME/v3/bin`，默认 `~/.pinset/v3/bin`；自定义 CLI 安装目录也遵循这一规则。使用 `pinset self shell` 输出的片段将该目录放在 PATH 最前面，避免其他目录里的 2.x 启动脚本遮蔽新入口。PowerShell 当前会话可执行 `pinset self shell powershell | Out-String | Invoke-Expression`，该命令不会修改用户 profile。

## 检查、秘密与恢复

`check` 默认只读，不解密、运行工具、联网或写盘。`--deep` 检查安装内容；`--probe` 明确运行探针，记录路径、版本、宿主和时间。分别报告配置有效、安装有效、入口绑定与实际探针结果；项目构建由原始命令完成。

profile 按值使用 age 加密。私钥只存在系统凭据库或显式 `PINSET_IDENTITY`，不写私钥文件。项目信任绑定目录、项目和配置/profile 指纹；外来变更使信任失效。变量冲突以及覆盖工具链/控制变量均报错。详见[环境协议](docs/environment.md)。

升级和切换版本直接使用 `use`；切回旧版本也使用明确的版本选择。`self repair` 恢复当前项目与全局选择的中断事务，并处理 CLI/shim 更新中断，不撤销已完成的版本切换。应用依赖、源码、数据库与 IDE 进程仍由项目管理。

## 开发与发布

五个 Rust crate 分离只读模型、引擎、加密环境、CLI 与 shim，八个 Provider 共用安装、下载、缓存、解压和事务服务。XZ 采用纯 Rust 解码。

所有格式、lint、安全扫描、测试、独立 typecheck 和运行验收均使用本地 Docker：`verify.ps1 -Suite fast|acceptance|platform|integrations|all`；Unix 用 `./verify.sh <suite>`。原仓库只读挂载，测试使用容器副本。详见[验收入口](scripts/docker/README.md)。

按用户要求，Flutter/Android 大型 SDK 下载测试免测，实际 Flutter 运行和 APK 构建仍标记为未验收。元数据、锁、路由及 Android 证据契约仍须通过；本地报告和发布门禁明确保留这一边界。

CI 仅手动发布：核验绑定干净 commit 的本地 all 报告，编译四平台、打包扩展、生成 checksum/SBOM/来源证明并发布，不调用测试。Linux x64 原生验收，ARM64 使用 QEMU；Windows/macOS 原生行为明确不计入 Docker 验收。
