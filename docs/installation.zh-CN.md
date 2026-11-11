# 安装 Pinset 与配置 PATH

[English](installation.md) · [官网](https://pinset.future-element.com) · [主仓](https://github.com/Future-Element/pinset)

## 安装已发布的 CLI

从[官网](https://pinset.future-element.com/#install)复制当前稳定 release 的命令。版本以主仓的[最新公开 release](https://github.com/Future-Element/pinset/releases/latest)为准，网站 package 版本或未发布的源码版本不是发布依据。

若想先检查安装脚本并自行选择精确版本，将以下 `VERSION` 替换成 release 版本号，不带前缀 `v`：

```sh
# macOS Apple Silicon、Linux x86_64 或 Linux ARM64
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/install.sh -o install.sh
sh install.sh --version VERSION
```

```powershell
# Windows x86_64
Invoke-WebRequest https://raw.githubusercontent.com/Future-Element/pinset/main/install.ps1 -OutFile install.ps1
.\install.ps1 -Version VERSION
```

不传版本参数时，脚本使用其内置推荐版本，可能与 GitHub 最新 release 不同。官网成功查询最新 release 后会在命令中显式传入版本。当前不发布 macOS Intel 二进制；平台支持请核对 [release 制品](https://github.com/Future-Element/pinset/releases/latest)。

安装脚本校验压缩包摘要与 CLI 版本，放置 CLI、路由器并注册 Provider 命令入口，不下载语言运行时。**默认安装不修改任何 shell profile，也不持久化 PATH，只打印 PATH 片段。** 当前版本没有写入 profile 的安装向导，也没有 `pinset self shell --write` 命令。

若使用 Homebrew 或 Scoop，按[渠道接入说明](../packaging/README.md#安装目录path-和升级)定位包内 CLI，再用 `self repair` 刷新受管入口。渠道版本以各自 manifest 为准，可能滞后于 GitHub 最新 release。以下示例使用官方脚本的默认 CLI 位置。

## 预览并手动应用 PATH

Provider 入口位于 `PINSET_HOME/v3/bin`，默认 `~/.pinset/v3/bin`，应放在 PATH 最前面。自定义 CLI 的 `--install-dir` / `-InstallDir` 不改变 Provider 入口目录；请使用安装后 CLI 的绝对路径打印正确片段。若设置了 `PINSET_HOME`，将下面默认路径换成其实际 `v3/bin`。

`self shell` 需要指定 shell 名称，**只打印，不执行、不写文件**。默认 CLI 路径的预览命令：

```sh
"$HOME/.pinset/v3/bin/pinset" self shell bash
"$HOME/.pinset/v3/bin/pinset" self shell zsh
"$HOME/.pinset/v3/bin/pinset" self shell fish
```

```powershell
& "$env:USERPROFILE\.pinset\v3\bin\pinset.exe" self shell powershell
```

检查输出后，自行将 PATH 命令应用到当前 shell。以下默认目录示例只影响当前会话：

```sh
# Bash 或 Zsh
export PATH="$HOME/.pinset/v3/bin:$PATH"
```

```fish
# Fish：--path 使用 PATH，不写持久化的 universal fish_user_paths
fish_add_path --path --move --prepend "$HOME/.pinset/v3/bin"
```

```powershell
$env:PATH = "$env:USERPROFILE\.pinset\v3\bin" + [IO.Path]::PathSeparator + $env:PATH
```

关闭会话即可丢弃该会话的 PATH 改动，启动文件未被编辑。另见[命令参考](commands.zh-CN.md#self-shell)和[在线参考](https://pinset.future-element.com/docs/commands/self-shell)。

Fish 的 [`--path` 参数](https://fishshell.com/docs/current/cmds/fish_add_path.html)只改变会话 PATH；`--move` 将已有入口移到最前面，不再添加一份。

## 确认后可选持久化

持久化是单独的明确选择。先确认目标文件与 PATH 目录，检查已有 Pinset 区块，文件已存在时先备份，再自行编辑。Pinset 不会代为执行这些写入。

| Shell | 需要检查并可自行编辑的文件 |
| --- | --- |
| Bash 交互会话 | `~/.bashrc`；登录 shell 可能读取 `~/.bash_profile`。按实际加载方式选择，避免通过两个文件重复加载同一区块。 |
| Zsh 交互会话 | `${ZDOTDIR:-$HOME}/.zshrc` |
| Fish | `$XDG_CONFIG_HOME/fish/config.fish`，未设置时为 `~/.config/fish/config.fish` |
| PowerShell | `$PROFILE.CurrentUserCurrentHost` 打印的实际路径；不同宿主和 PowerShell 版本可能不同。 |

只为所用 shell 添加**一个**标记区块；已有时替换该区块，不重复追加。以下默认目录片段在重新加载时不会再次向 PATH 头部堆叠相同目录。自定义 home 时，将目录替换成 `self shell` 打印的绝对 `v3/bin` 路径。

```sh
# Bash / Zsh
# >>> pinset PATH >>>
case "$PATH" in
  "$HOME/.pinset/v3/bin"|"$HOME/.pinset/v3/bin":*) ;;
  *) export PATH="$HOME/.pinset/v3/bin:$PATH" ;;
esac
# <<< pinset PATH <<<
```

```fish
# >>> pinset PATH >>>
fish_add_path --path --move --prepend "$HOME/.pinset/v3/bin"
# <<< pinset PATH <<<
```

```powershell
# >>> pinset PATH >>>
$pinsetBin = Join-Path $env:USERPROFILE '.pinset\v3\bin'
if (($env:PATH -split [regex]::Escape([string][IO.Path]::PathSeparator), 2)[0] -ne $pinsetBin) {
    $env:PATH = $pinsetBin + [IO.Path]::PathSeparator + $env:PATH
}
# <<< pinset PATH <<<
```

回滚：从实际编辑的文件中只删除上述 Pinset 标记区块，或检查后续编辑后恢复备份。如果文件是仅为此区块新建的，确认无其他内容后再删除。之后新开 shell。不要删除其他 profile 内容或 Pinset 数据。

## 本地验证

在应用 PATH 的 shell 中执行 `pinset --version`，与所选精确 release 比较。用 `command -v pinset`（Bash/Zsh）、`type -p pinset`（Fish）或 `Get-Command pinset`（PowerShell）检查实际入口。项目选好工具后，`pinset which node --explain` 可解释路由。

验证可选持久化：新开两次会话，检查受管目录在 PATH 最前面、目标文件仅有一个标记区块，且重载文件不再新增头部目录。验证默认行为：先保存 profile 内容并记录未存在文件，再执行安装器或 `self shell`；前后现有文件内容与未存在状态应完全相同。

维护者用 `bash verify.sh fast` 验证安装器与四种 shell 的只打印契约，用 `bash verify.sh integrations` 验证官网版本查询、typecheck、构建与 SEO。两者均使用仓库隔离的本地 Docker 入口；Windows 和 Fish 的原生执行仍需单独手工验收。
