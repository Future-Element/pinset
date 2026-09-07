# Contributing to Pinset

感谢你帮助改进 Pinset。项目当前开发版本支持 Node、pnpm、Bun、Go、Flutter、Python、Java、Rust 与 .NET Provider，优先保证跨平台行为、安全边界和可复现安装。

## 开发环境

- Rust 1.97 或更高版本；
- Windows x64、Linux x64 或 macOS arm64；
- Linux 构建 TAR.XZ 支持时需要常规 C toolchain、CMake 与 liblzma 开发包。

```shell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo install cargo-audit --version 0.22.2 --locked
cargo audit --deny warnings --ignore RUSTSEC-2023-0071
```

Unix curl 安装器使用完全离线的假 Release 测试：

```sh
sh scripts/tests/install_sh_test.sh
sh scripts/tests/uninstall_sh_test.sh
```

Windows 完整卸载脚本使用独立 PowerShell 临时目录测试：

```powershell
./scripts/tests/uninstall_ps1_test.ps1
```

这些检查不会在维护者开发机运行。格式化、Clippy、构建、测试、依赖安全审计和真实运行时验收必须在 GitHub Actions 临时虚拟机中进行，并在 PR 中说明平台与结果；开发机和 WSL 只做编辑及静态差异检查。

`RUSTSEC-2023-0071` 仅影响 RSA 私钥操作的可观察计时；Pinset 的 OpenPGP 路径只解析公开证书并验证公开签名，不持有私钥或执行解密。该无修复版本的中危告警是当前唯一的可达性例外；高危运行时依赖告警不得忽略。

## Pull Request

- 从最新 `main` 创建短生命周期分支；
- 不混入无关格式化或本地环境文件；
- 对安全边界和跨平台差异补回归测试；
- 明确写出已验证与未验证的平台；
- 不提交下载缓存、运行时归档、密钥或凭据。

涉及配置执行、远程脚本、遥测、权限提升、来源信任或自动修改 shell profile 的变更，需要单独的安全评审。

## 发布流程

1. 通过 Pull Request 将版本号、安装脚本默认版本和 Changelog 合并到 `main`；
2. 在 GitHub Actions 中从 `main` 手动运行 **Release preflight**；
3. 一次性修复预检报告的格式、Clippy、测试、脚本、安全审计和重点发布平台构建问题；
4. 仅在预检成功后的 24 小时内，为同一个提交创建并推送签名版本标签；
5. **Release** 工作流验证预检记录，然后重新构建、签署并发布最终产物。

不要通过移动已有标签来逐项试错。普通 CI 的临时跨平台构建不生成发布归档；正式归档、SBOM、校验和与 provenance 只由标签触发的 Release 工作流生成。

从 2.2 开始，每个功能版本先发布 `-rc.N`（N 为正整数），通过验收后再发布正式版。RC 与正式版分别遵循上述 preflight 和签名标签流程。

发布后从 `main` 运行 **Verify published release**，填写发布标签、完整目标提交 SHA 及用于自更新验收的较早版本号。该流程在四个平台核验实际下载的制品、签名标签、校验和、SBOM 和来源证明，并执行安装、加密环境及 CLI/shim 配套测试、自更新验收。所有检查通过后才记录发布完成；失败时保留标签与制品，修复使用新的 RC 或补丁版本。
