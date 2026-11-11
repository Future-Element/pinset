# Install Pinset and set up PATH

[简体中文](installation.zh-CN.md) · [Website](https://pinset.future-element.com) · [Main repository](https://github.com/Future-Element/pinset)

## Install a released CLI

Copy the commands for the current stable release from the [website](https://pinset.future-element.com/#install). The version authority is the main repository's [latest public release](https://github.com/Future-Element/pinset/releases/latest), not the website package version or an unreleased source version.

To inspect the scripts first and select an exact release yourself, replace `VERSION` below with that release number, without the leading `v`:

```sh
# macOS Apple Silicon, Linux x86_64 or Linux ARM64
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/install.sh -o install.sh
sh install.sh --version VERSION
```

```powershell
# Windows x86_64
Invoke-WebRequest https://raw.githubusercontent.com/Future-Element/pinset/main/install.ps1 -OutFile install.ps1
.\install.ps1 -Version VERSION
```

Without a version argument, these scripts use their embedded recommended release, which can differ from GitHub's latest release. The website supplies an explicit version once its latest-release lookup succeeds. macOS Intel binaries are not currently published. See [release assets](https://github.com/Future-Element/pinset/releases/latest) for availability.

The scripts verify the archive checksum and CLI version, install the CLI/router, and register Provider command entries. They do not download language runtimes. **By default, installation does not edit any shell profile or persist PATH changes: it only prints a PATH fragment.** There is no profile-writing installer wizard or `pinset self shell --write` command in this version.

For Homebrew or Scoop, follow [distribution channel setup](../packaging/README.md#安装目录path-和升级) to locate the package's CLI and refresh its managed entries with `self repair`. Channel versions follow their own manifests and may lag behind the latest GitHub release. The examples below use the official script's default CLI location.

## Preview and apply PATH manually

Provider entries live in `PINSET_HOME/v3/bin`, defaulting to `~/.pinset/v3/bin`. Put this directory first on PATH. A custom CLI `--install-dir` / `-InstallDir` does not change the Provider entry directory; use the installed CLI's absolute path to print the correct fragment. With `PINSET_HOME` set, substitute its actual `v3/bin` for the default paths below.

`self shell` requires a shell name and **only prints**; it neither applies the fragment nor writes a file. Preview with the default CLI path:

```sh
"$HOME/.pinset/v3/bin/pinset" self shell bash
"$HOME/.pinset/v3/bin/pinset" self shell zsh
"$HOME/.pinset/v3/bin/pinset" self shell fish
```

```powershell
& "$env:USERPROFILE\.pinset\v3\bin\pinset.exe" self shell powershell
```

After reviewing the output, apply its PATH command yourself. These default-location examples affect the current shell session:

```sh
# Bash or Zsh
export PATH="$HOME/.pinset/v3/bin:$PATH"
```

```fish
# Fish: --path uses PATH rather than persistent universal fish_user_paths
fish_add_path --path --move --prepend "$HOME/.pinset/v3/bin"
```

```powershell
$env:PATH = "$env:USERPROFILE\.pinset\v3\bin" + [IO.Path]::PathSeparator + $env:PATH
```

Close that session to discard its PATH changes. No startup file has been edited. See the [command reference](commands.md#self-shell) and [online reference](https://pinset.future-element.com/en/docs/commands/self-shell).

Fish's [`--path` option](https://fishshell.com/docs/current/cmds/fish_add_path.html) changes session PATH; `--move` brings an existing entry to the front without adding another copy.

## Optional persistence after review

Persistence is a separate, explicit choice. First confirm the exact file and PATH directory, inspect any existing Pinset block, and back up the file if it exists. Then edit it yourself. Pinset performs none of these writes.

| Shell | File to review and optionally edit |
| --- | --- |
| Bash interactive | `~/.bashrc`; login shells may instead read `~/.bash_profile`. Choose the file your shell loads; avoid loading the same block through both. |
| Zsh interactive | `${ZDOTDIR:-$HOME}/.zshrc` |
| Fish | `$XDG_CONFIG_HOME/fish/config.fish`, or `~/.config/fish/config.fish` when unset |
| PowerShell | The exact path printed by `$PROFILE.CurrentUserCurrentHost`; it varies by host and PowerShell edition. |

Add **one** marked block for your shell. If it already exists, replace that block instead of appending another. These default-location examples guard against repeatedly prepending the directory when the file is reloaded. For a custom home, replace the directory with the absolute `v3/bin` shown by `self shell`.

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

Rollback: remove only the marked Pinset block from the file you edited, or restore your backup after checking for later edits. If you created a new file solely for this block, remove it only if it still contains nothing else. Open a fresh shell afterward. Do not delete your other profile content or Pinset data.

## Verify locally

In the shell where you applied PATH, run `pinset --version` and compare with the exact release you chose. Use `command -v pinset` (Bash/Zsh), `type -p pinset` (Fish), or `Get-Command pinset` (PowerShell) to check the resolved path. After selecting project tools, `pinset which node --explain` explains routing.

To check optional persistence, open two fresh sessions and confirm that the managed directory is first, there is only one marked block in the chosen file, and reloading that file does not add another leading PATH entry. To check the default contract, save profile contents and note missing files, run the installer or `self shell`, and compare afterward: existing contents and missing-file status must stay unchanged.

Maintainers run `bash verify.sh fast` for installer and four-shell print-only contracts, and `bash verify.sh integrations` for website release lookup, typechecks, build and SEO. Both use the repository's isolated local Docker verifier; native Windows and Fish execution require separate manual verification.
