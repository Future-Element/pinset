# Homebrew 与 Scoop 分发

Pinset 的正式版本由 Git 标签触发 `.github/workflows/release.yml`。工作流会构建以下三个归档，生成 `SHA256SUMS`，并把可直接提交到 Tap/Bucket 的 `pinset.rb` 与 `pinset.json` 一起发布到 GitHub Release：

- `pinset-linux-x86_64.tar.gz`
- `pinset-macos-aarch64.tar.gz`
- `pinset-windows-x86_64.zip`

当前支持矩阵是 Linux x86_64、Windows x86_64 和 macOS arm64。Intel Mac、Linux arm64 尚未提供归档，包定义也会明确拒绝不支持的架构。

## 发布一个版本

先把 workspace 版本从 `0.0.0` 修改为准备发布的版本，运行一次 `cargo check --workspace` 让 `Cargo.lock` 同步版本，然后把两者正常合并：

```toml
[workspace.package]
version = "0.1.0"
```

然后从包含该版本的提交创建并推送完全一致的标签：

```shell
git tag -a v0.1.0 -m "Pinset v0.1.0"
git push origin v0.1.0
```

标签必须是 `v` 加上 `Cargo.toml` 中的版本，否则 Release 工作流会失败。带有后缀的版本（例如 `0.1.0-beta.1`）会被标为 GitHub prerelease。工作流不会执行 `pinset install`，不会在 runner 或本机安装 Node.js、Python 或 Flutter。

## 注册 Homebrew Tap

Homebrew 的第三方 Tap 本质上是一个 Git 仓库，不需要先向 Homebrew 中央仓库登记。建议创建公开仓库：

```text
Future-Element/homebrew-tap
```

首次创建可以在装有 Homebrew 的 macOS/Linux 上运行：

```shell
brew tap-new Future-Element/homebrew-tap
gh repo create Future-Element/homebrew-tap \
  --public \
  --source "$(brew --repository Future-Element/homebrew-tap)" \
  --push
```

每次 Pinset Release 完成后：

1. 下载 Release 附带的 `pinset.rb`。
2. 把它提交到 `Future-Element/homebrew-tap` 的 `Formula/pinset.rb`。
3. 在 macOS arm64 和 Linux x86_64 上分别执行 Tap 自带的检查。

用户可以直接安装（Homebrew 当前推荐这种方式）：

```shell
brew install Future-Element/homebrew-tap/pinset
```

也可以先添加 Tap：

```shell
brew tap Future-Element/tap
brew install pinset
```

发布前至少应验证：

```shell
brew audit --strict --online Future-Element/tap/pinset
brew install Future-Element/tap/pinset
pinset --version
```

当前模板安装上游预编译二进制，适合项目自有 Tap。以后申请进入 `homebrew/core` 时，需改为符合 Core 政策的源码构建 Formula，并满足公开源码、明确兼容许可证、稳定版本和支持平台测试等要求。

官方参考：

- [How to Create and Maintain a Tap](https://docs.brew.sh/How-to-Create-and-Maintain-a-Tap)
- [Formula Cookbook](https://docs.brew.sh/Formula-Cookbook)
- [Acceptable Formulae](https://docs.brew.sh/Acceptable-Formulae)

## 注册 Scoop Bucket

Scoop 的自定义 Bucket 同样是一个包含 JSON Manifest 的 Git 仓库，不需要中央登记。建议从官方 BucketTemplate 创建公开仓库：

```text
Future-Element/scoop-bucket
```

也可以手动创建仓库，并保留如下结构：

```text
scoop-bucket/
└── bucket/
    └── pinset.json
```

每次 Pinset Release 完成后，下载 Release 附带的 `pinset.json`，提交为 `bucket/pinset.json`。用户安装命令为：

```powershell
scoop bucket add future-element https://github.com/Future-Element/scoop-bucket
scoop install future-element/pinset
pinset --version
```

若想让 Bucket 出现在 Scoop Directory，可在 GitHub 仓库添加 `scoop-bucket` topic。将来若 Pinset 满足 Scoop 官方 Bucket 的接收条件，也可以再向对应官方 Bucket 提交 Manifest；自有 Bucket 更适合当前早期版本和快速迭代。

官方参考：

- [Buckets](https://github.com/ScoopInstaller/Scoop/wiki/Buckets)
- [App Manifests](https://github.com/ScoopInstaller/Scoop/wiki/App-Manifests)
- [BucketTemplate](https://github.com/ScoopInstaller/BucketTemplate)

## 当前发布前置条件

Pinset 主仓库目前是私有仓库，且开源许可证尚未决定。公开的 Homebrew/Scoop 用户无法匿名下载私有仓库的 Release 资产。因此正式启用包管理器分发前必须：

1. 决定并加入明确的开源许可证。
2. 将 Pinset Release 资产改为公众可访问（通常是公开主仓库）。
3. 创建公开的 `homebrew-tap` 与 `scoop-bucket` 仓库。
4. 在真实支持平台测试包管理器安装；这一步不应在日常开发机上冒险执行，可使用干净 VM/CI。
5. 后续补充 macOS 代码签名/公证和 Windows 代码签名；当前归档只有 SHA-256 完整性校验。

源模板位于 `packaging/homebrew/pinset.rb.template` 和 `packaging/scoop/pinset.json.template`。如需在本地只测试元数据生成（不安装任何运行时）：

```shell
python3 scripts/render_package_metadata.py \
  --version 0.1.0 \
  --checksums SHA256SUMS \
  --output-dir generated
```
