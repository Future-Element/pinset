# Contributing to Pinset

感谢你帮助改进 Pinset。项目当前开发版本支持 Node、pnpm、Bun、Go、Flutter、Python、Java 与 Rust Provider，优先保证跨平台行为、安全边界和可复现安装。

## 开发环境

- Rust 1.85 或更高版本；
- Windows x64、Linux x64 或 macOS arm64；
- Linux 构建 TAR.XZ 支持时需要常规 C toolchain、CMake 与 liblzma 开发包。

```shell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
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

这些测试不会在维护者开发机运行。格式化、Clippy、构建、测试和真实运行时验收必须在 GitHub Actions 临时虚拟机中进行，并在 PR 中说明平台与结果；开发机和 WSL 只做编辑及静态差异检查。

## Pull Request

- 从最新 `main` 创建短生命周期分支；
- 不混入无关格式化或本地环境文件；
- 对安全边界和跨平台差异补回归测试；
- 明确写出已验证与未验证的平台；
- 不提交下载缓存、运行时归档、密钥或凭据。

涉及配置执行、远程脚本、遥测、权限提升、来源信任或自动修改 shell profile 的变更，需要单独的安全评审。
