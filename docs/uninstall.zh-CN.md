# 完全卸载 Pinset

[English](uninstall.md)

仓库根目录的 `uninstall.sh` 支持 macOS/Linux，`uninstall.ps1` 支持 Windows PowerShell。脚本独立运行，不需要 Pinset 命令仍然可用。以下远程命令下载 `main` 分支中的脚本。

## macOS / Linux

预览清理计划：

```sh
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/uninstall.sh | sh -s -- --plan
```

执行完全卸载：

```sh
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/uninstall.sh | sh -s -- --yes
```

可以先下载，检查内容后执行。下载到文件再执行也能确保下载失败时不会执行不完整脚本：

```sh
script=$(mktemp) && curl -fL https://raw.githubusercontent.com/Future-Element/pinset/main/uninstall.sh -o "$script" && sh "$script" --yes
```

自定义位置或未登记的项目可以重复传入：

```sh
sh uninstall.sh --plan --pinset-home /opt/my-pinset --install-dir "$HOME/.local/bin" --project /path/to/project
sh uninstall.sh --yes --pinset-home /opt/my-pinset --install-dir "$HOME/.local/bin" --project /path/to/project
```

## Windows

如果清理计划包含系统级 `Machine/Path` 或其他系统环境变量，请以管理员身份打开 PowerShell。执行前关闭从 Pinset 启动的 Node、pnpm、Java 等开发任务和后台服务；Windows 的运行中程序会占用 SDK 和共享的命令入口。新版脚本会在修改数据前检查这两项条件，列出占用进程后停止，不会自动终止程序。`-Plan` 可在普通 PowerShell 中查看计划。

在 PowerShell 中下载并预览。使用 `curl.exe` 避免 Windows PowerShell 的 `curl` 别名：

```powershell
$script = Join-Path ([IO.Path]::GetTempPath()) 'pinset-uninstall.ps1'
curl.exe -fL https://raw.githubusercontent.com/Future-Element/pinset/main/uninstall.ps1 -o $script
if ($LASTEXITCODE -ne 0) { throw '下载卸载脚本失败' }
& $script -Plan
```

确认计划后执行：

```powershell
& $script -Yes
```

也可以下载到内存后直接执行，并检查 curl 是否成功：

```powershell
$pinsetUninstall = curl.exe -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/uninstall.ps1 | Out-String; if ($LASTEXITCODE -ne 0) { throw '下载失败' }; & ([scriptblock]::Create($pinsetUninstall)) -Yes
```

如果执行策略阻止运行，可仅为此次子进程绕过策略：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File $script -Yes
```

自定义位置或未登记的项目：

```powershell
& .\uninstall.ps1 -Plan -PinsetHome 'D:\Pinset' -InstallDir 'D:\Tools\bin' -Project 'D:\Code\my-project'
& .\uninstall.ps1 -Yes -PinsetHome 'D:\Pinset' -InstallDir 'D:\Tools\bin' -Project 'D:\Code\my-project'
```

执行后可删除下载到临时目录的卸载脚本。

遇到文件占用或权限错误时，处理报告中的条件后重新下载脚本并运行。删除过程中若有文件失败，脚本会保留该目录的所有权记录，便于安全重试；未涉及 Pinset 路径的环境变量（包括空值变量）保持原样。

## 清理范围

- Pinset 的 CLI、router、工具入口，以及其管理的 Node、pnpm、Bun、Go、Python、JDK、Rust、Flutter/Dart 和通用工具安装。
- 已识别的 Pinset 3.x / 2.x 数据、下载缓存、全局配置、状态、信任记录和迁移备份。
- Pinset 专用环境变量、PATH 中的托管路径，以及指向这些路径的语言环境变量；支持的 shell 启动文件和补全配置。
- 系统凭据库中 Pinset 3.x / 2.x 的加密身份。删除后，仍保留在其他地方的加密环境文件可能无法解密。
- 已登记或显式指定项目中的 Pinset 配置、加密环境文件、本地状态，以及带有 Pinset 所有权标记的虚拟环境。

`--plan` / `-Plan` 不删除文件、不修改环境变量，也不删除凭据。未指定 `--yes` / `-Yes` 时需要交互确认。脚本检查目录归属与路径；遇到未知目录、无法安全编辑的配置、权限或占用问题，会报告未完成项，不能把有错误的运行视为完整卸载。

默认只处理当前用户及已发现的安装位置和项目，不遍历整块磁盘或其他用户目录。无法发现的旧自定义 home、安装目录和项目需要通过参数补充。不要整体删除共享的 `~/.cargo`、`~/.npm`、`~/.pub-cache` 等目录，它们也可能被独立安装的工具使用。项目构建产物、应用依赖和独立安装的 SDK 不属于 Pinset 管理的环境。

脚本支持 Homebrew、WinGet、Scoop 的对应包卸载。aqua 的项目声明需要自行移除，遇到 aqua 托管文件会提示处理。Linux 原生凭据清理使用 `secret-tool`；有身份记录但缺少该工具时，脚本会先停止，保留数据供重试。复杂或混合的 shell 初始化命令无法安全拆分时会保留并报告，退出码为非零。

自行安装的编辑器扩展、IDE 设置和 CI 文件需要在各自配置中处理。例如 VS Code 可执行 `code --uninstall-extension FutureElement.pinset-vscode`，并检查项目 `.vscode/settings.json` 中仍指向 Pinset 的 Python、Flutter、Rust、Java 路径。任意命名的 shell 配置片段也可能需要手动检查。

卸载完成后关闭并重新打开终端。通过管道或子进程执行的脚本无法改写父进程或已经打开的其他终端的环境；这些终端关闭后，旧的临时环境变量才会消失。系统级安装或系统级环境变量需要相应权限，按脚本报告处理后重新运行。

隔离验证：Linux（WSL）脚本样例和 Windows PowerShell 5.1 / PowerShell 7 样例已通过。样例使用临时目录，并替换系统凭据与包管理器调用。macOS 原生运行、实际包管理器卸载和真实凭据删除尚未验证；没有在当前机器执行卸载。
