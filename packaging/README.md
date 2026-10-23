# 分发渠道维护

本目录保存可审核的渠道文件，当前版本为已公开的 **Pinset 3.1.0**。生成时已实际下载四个平台 ZIP，重算 SHA-256，并确认每包只含位于同一目录的 `pinset` 和 `pinset-shim` 两个程序。Homebrew 已在 Linux x64 Docker 中、Scoop 已在隔离的 Windows x64 目录中完成真实安装、从 3.0.9 升级、托管入口刷新和工具路由验证；两者均用公开资产通过了真实 GitHub attestation 验证。WinGet 清单已通过本机 `winget validate`，实际 WinGet 安装仍以上游验证为准。

| 渠道 | 文件与发布位置 | 当前状态 |
| --- | --- | --- |
| Homebrew | `homebrew/pinset.rb`；[官方 tap](https://github.com/Future-Element/homebrew-tap) 的 `Formula/pinset.rb` | 3.1.0 已公开；`brew install Future-Element/tap/pinset` |
| Scoop | `scoop/pinset.json`；[官方 bucket](https://github.com/Future-Element/scoop-bucket) 的 `bucket/pinset.json` | 3.1.0 已公开；支持直接使用公开 manifest URL 一条命令安装 |
| WinGet | `winget/manifests/f/FutureElement/Pinset/3.1.0/` 下三个文件 | [收录 PR #460148](https://github.com/microsoft/winget-pkgs/pull/460148) 已提交，CLA 已通过，等待上游验证和审核合并 |
| aqua | `aqua/registry.yaml` 和 `aqua/pkg.yaml` | [标准注册表收录 PR #63088](https://github.com/aquaproj/aqua-registry/pull/63088) 已提交，等待审核合并 |
| GitHub Actions | 仓库根目录 `action.yml` | [setup-pinset 已在 Marketplace 上架](https://github.com/marketplace/actions/setup-pinset)，最新为 v3.1.0；使用 `Future-Element/pinset@v3.1.0` |

当前资产支持 Linux x64/ARM64、macOS ARM64、Windows x64；没有 macOS Intel 或 Windows ARM64 包。Formula、Scoop 和 WinGet 限制不支持的架构，aqua 列出四种明确环境并关闭模拟运行回退。每个渠道均保留真正相邻的 CLI/router；不能只把 CLI 复制到包外的单独目录。

## 生成和更新

在仓库根目录运行：

```sh
python scripts/generate_channels.py --verify-archives
```

脚本读取 `release.json` 的 `published`，只接受匹配的公开稳定 Pinset 3 release，下载其 SHA256SUMS，并核对校验清单和四个 ZIP 的 GitHub asset digest。`--verify-archives` 还会下载并重算四个 ZIP 的摘要，检查两个根目录程序。结果默认写入 `output/channels/`；它不会覆盖本目录、提交 Git、更新远端仓库、提交 PR、发布包或安装任何工具。

将 `output/channels/packaging/` 中审阅后的文件手动更新到本目录。发布新版本时必须先有真实公开资产，再生成新的版本文件。WinGet 新版本应保留独立版本目录。生成证据在 `output/channels/evidence/`，不会充当外部渠道已发布的证明。

可提供 `--release-metadata PATH --checksums PATH` 使用固定快照离线生成。这时信任的是调用者提供的元数据快照；摘要一致不等同于签名验证。离线回归使用固定的最小 3.0.9 fixture：

```sh
python scripts/tests/channels.py
```

它验证重复生成一致、篡改 SHA256SUMS 拒绝、缺平台资产拒绝，且失败不写半套 manifest。测试明确禁止网络。发行前还应单独完成对应系统的真实安装、一次升级、`self repair`、工具路由以及卸载保留数据验收。

## 安装目录、PATH 和升级

CLI 安装在包管理器目录，工具与状态位于 `PINSET_HOME/v3/`，默认 `~/.pinset/v3/`。`self repair` 和工具安装会把 CLI/router 及命令入口复制到 `PINSET_HOME/v3/bin`；`self shell` 将这个目录放在 PATH 前面。因此包管理器升级后，只运行未限定路径的 `pinset self repair` 可能仍然调用旧副本。需要从渠道实际安装的新版 CLI 刷新托管入口，再检查 PATH。

### Homebrew

Formula 把两个程序装进同一个 keg 的 `bin`，通过 caveats 给出绝对路径，不在 post-install 中自动写用户 home。安装、升级后使用实际 Formula 前缀：

```sh
PINSET_CHANNEL_CLI="$(brew --prefix pinset)/bin/pinset"
"$PINSET_CHANNEL_CLI" self repair
"$PINSET_CHANNEL_CLI" self shell bash
```

最后一行仅打印当前 shell 的 PATH 配置；按需在当前会话应用，或显式放入自己的配置。使用 zsh/fish 时传对应名称。通过 Homebrew 升级此包，随后再次运行上述绝对路径的 `self repair`。

### Scoop

ZIP 的两个 EXE 留在同一个 app 目录，只给 `pinset.exe` 建立 Scoop 命令入口。`post_install` 会通过真实 `$dir\pinset.exe self repair` 刷新托管副本，包括 Scoop 更新时；失败即停止。需要手动恢复时：

```powershell
$pinsetChannelCli = Join-Path (scoop prefix pinset) 'pinset.exe'
& $pinsetChannelCli self repair
& $pinsetChannelCli self shell powershell
```

按输出显式应用 PATH 配置。Scoop `checkver.github` 使用仓库 URL 和官方默认捕获组，检测结果会去掉 tag 的 `v`；autoupdate 中 `v$version` 因而只生成一个 `v`。不要改成依赖新版 helper 才支持的完整 API endpoint。

### WinGet

使用现有 ZIP 的 nested portable 模式，两个 EXE 保持原名。官方 [PortableFlow 实现](https://github.com/microsoft/winget-cli/blob/bc1e9ff8b982798ab346b61416b76ae738038e57/src/AppInstallerCLICore/Workflows/PortableFlow.cpp#L204-L256) 将 ZIP 顶层条目放入同一个 `TargetInstallLocation`，nested entries 只建立命令别名，不会将两个程序拆到不同目录。无需另造脚本 installer。

升级后，通过 WinGet 的安装记录或命令链接找到包管理器目录中的真实 `pinset.exe`，确认旁边有 `pinset-shim.exe`，再使用该绝对路径执行 `self repair` 和 `self shell powershell`。默认用户安装且链接创建成功时，可以检查 `%LOCALAPPDATA%\Microsoft\WinGet\Links\pinset.exe` 的 `Target`；如果用了自定义 portable 根目录或链接创建回退，按实际安装记录定位。不要直接以 PATH 中的第一个 `pinset` 代替这个真实路径。

本地 manifest 检查：

```powershell
winget validate --manifest packaging/winget/manifests/f/FutureElement/Pinset/3.1.0 --disable-interactivity
```

### aqua

registry 候选把同一 ZIP 的两个程序声明为命令。aqua 安装或版本切换后，用 aqua 自己的路径查询找到包内 CLI，再刷新托管副本，例如 Unix shell：

```sh
PINSET_CHANNEL_CLI="$(aqua which pinset)"
"$PINSET_CHANNEL_CLI" self repair
"$PINSET_CHANNEL_CLI" self shell bash
```

在 PowerShell 中同样先保存 `aqua which pinset` 的结果，再以该路径执行。应在包含选定 Pinset 版本的 aqua 配置目录下操作；不要用普通 `which pinset`，因为它可能找到 Pinset home 的旧副本。

向 aqua 上游提交时，按[贡献指南](https://github.com/aquaproj/aqua-registry/blob/main/docs/add_package.md)在个人 fork 运行 `argd s Future-Element/pinset`，把候选配置与脚手架输出核对，再运行 `argd t`、签名提交并提 PR。PR #63088 已执行这些步骤，且通过 Conftest、格式检查及 Linux/Windows x64 原生安装和托管入口升级验证；其他平台仅验证下载解包。PR 中还按上游格式维护历史版本布局，本目录候选不替代上游配置，也不表示已获收录。

## 卸载和保留数据

使用对应包管理器升级和卸载其包文件。`pinset self update` 会替换解析到的 CLI/router，可能绕过包管理器清单，因此渠道安装应继续由所属包管理器升级。

包管理器卸载只删除它管理的包文件。`PINSET_HOME` 中的工具、状态、复制的 CLI/router 和命令入口会保留；如果该目录仍在 PATH 前面，卸载后 `pinset` 命令仍可能可用。没有渠道 uninstaller 会递归删除整个 Pinset home。

如果用户明确希望移除托管 home，使用仓库既有 [uninstall.sh](../uninstall.sh) 或 [uninstall.ps1](../uninstall.ps1)。先用 `--plan` / `-Plan` 查看目标，再按脚本交互确认；脚本验证 Pinset 3 所有权后删除 `PINSET_HOME/v3`，包括其中的 SDK、状态和命令入口。包管理器目录和用户自行添加的 shell 配置需分别处理。维护流程不自动执行这一步。
