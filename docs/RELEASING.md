# Pinset 发布流程

Pinset 使用版本标签驱动的 GitHub Actions 自动发布，不手工上传 Release 资产。

## 发布前

1. 更新 workspace `version` 和 `Cargo.lock`；
2. 更新 README、使用文档和变更说明中的版本；
3. 在 Pull Request 中通过 Quality；
4. 合并到 `main` 并确认主分支 Quality 成功；
5. 确认目标版本在 Windows x64、Linux x64、macOS arm64 的已知限制。

## 触发发布

标签必须与 workspace 版本完全一致：

```shell
git tag -a v0.1.0-alpha.1 -m "Pinset 0.1.0-alpha.1"
git push origin v0.1.0-alpha.1
```

Release workflow 会自动：

1. 校验标签与 Cargo workspace 版本；
2. 运行格式、Clippy、Rust 测试和离线 curl 安装器测试；
3. 构建 Linux x64、Windows x64、macOS arm64；
4. 生成三个平台归档；
5. 发布 `install.sh`、平台归档和 `SHA256SUMS`；
6. 带连字符的版本自动标记为 GitHub Prerelease。

任一步失败都不会创建新的完整 Release。失败时修复代码并发布新版本，不覆盖已经公开使用的版本标签。

## 发布后验证

- 检查 Release 中五个资产是否齐全；
- 核对 `SHA256SUMS`；
- 在 Linux/WSL 使用固定版本 curl 命令安装到临时目录；
- 运行 `pinset --version`；
- 记录未执行的真实运行时或平台验证。
