# Pinset

[English](README.md) | [简体中文](README.zh-CN.md)

[![CI](https://github.com/Future-Element/pinset/actions/workflows/ci.yml/badge.svg)](https://github.com/Future-Element/pinset/actions/workflows/ci.yml)
[![GitHub Release](https://img.shields.io/github/v/release/Future-Element/pinset?include_prereleases)](https://github.com/Future-Element/pinset/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Pinset is a predictable, project-aware runtime version manager for polyglot development.

It manages Node.js, pnpm, Bun, Go, Python, Java, Rust, .NET, and Flutter/Dart through one configuration and lockfile model. Traditional version files are read only by the explicit `detect` and `import` migration commands, never as an implicit runtime fallback.

## Highlights

- One CLI for global defaults and reproducible project selections.
- Requested selectors stay in `pinset.toml`; exact versions and per-platform artifacts are recorded in `pinset.lock`.
- Strict project routing by default, with explicit global inheritance and system fallback policies.
- Explainable resolution through `current --explain`, `which --explain`, and `doctor`.
- Direct command routing through one small, runtime-independent shim.
- Node.js release manifests verified with embedded OpenPGP trust roots before checksums are parsed.
- Provider integrity checks, safe extraction, atomic installs, ownership-aware uninstall, and a content-addressed download cache.
- First-class English and Simplified Chinese output, JSON schema 1 for automation, and shell completion.
- Project-owned Python `.venv` support without requiring shell activation.
- Read-only detection and explicit import of repository-local traditional version files.

## Install and upgrade

### Linux and macOS

The installer downloads the matching GitHub Release archive, verifies its entry in `SHA256SUMS`, and installs `pinset` and `pinset-shim` into `~/.local/bin` by default.

```sh
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/install.sh | sh
```

Add the directory before system runtime directories in the current shell:

```sh
export PATH="$HOME/.local/bin:$PATH"
```

Run the same installer again to upgrade. To install an exact release or another absolute directory:

```sh
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/install.sh | sh -s -- --version 1.5.1
PINSET_INSTALL_DIR=/opt/pinset/bin sh install.sh
```

### Windows

Download `pinset-windows-x86_64.zip` from [GitHub Releases](https://github.com/Future-Element/pinset/releases), extract `pinset.exe` and `pinset-shim.exe` into a permanent directory, and place that directory near the beginning of your user `PATH`.

```powershell
$pinsetBin = 'C:\Tools\pinset'
$env:PATH = "$pinsetBin;$env:PATH"
pinset --version
```

Upgrade by replacing both binaries with the files from a newer Release archive. Windows and WSL installations are independent.

### Manual download

All supported archives, `SHA256SUMS`, SBOMs, and build attestations are published on the [Releases page](https://github.com/Future-Element/pinset/releases). Verify the archive checksum before extracting it.

## Shell setup

Pinset does not edit shell profiles. Add the appropriate initialization yourself so its routing directory takes precedence.

### Bash

```sh
eval "$(pinset activate bash)"
```

### Zsh

```sh
eval "$(pinset activate zsh)"
```

### Fish

```fish
pinset activate fish | source
```

### PowerShell

```powershell
pinset activate powershell | Out-String | Invoke-Expression
```

Add the matching line to your shell profile for future sessions. Run `pinset doctor` if another runtime manager or a system command appears earlier in `PATH`.

## Shell completion

Completion scripts are generated on demand:

```sh
pinset completions bash > ~/.local/share/bash-completion/completions/pinset
pinset completions zsh > "${fpath[1]}/_pinset"
pinset completions fish > ~/.config/fish/completions/pinset.fish
```

```powershell
pinset completions powershell | Out-String | Invoke-Expression
```

## Quick start

Set global defaults used outside a Pinset project:

```sh
pinset global node@lts
pinset global pnpm@latest
pinset global go@1.25
pinset current node
```

Pin a project independently:

```sh
mkdir example && cd example
pinset init
pinset use node@22
pinset use pnpm@10
pinset use python@3.14
pinset install --locked
pinset exec -- node --version
```

Commit `pinset.toml` and `pinset.lock`. Selectors such as `latest`, `lts`, `stable`, or version prefixes remain visible in `pinset.toml`; their exact resolved versions are recorded in the lockfile. `pinset outdated` distinguishes a compatible lock refresh from a selector-changing upgrade, and `pinset update --dry-run` previews compatible lock changes.

Schema 3 projects are strict by default: once `pinset.toml` exists, an undeclared tool does not inherit a global selection or use the system `PATH`. Opt in deliberately when needed:

```toml
[policy]
inherit-global = true
system-fallback = false
boundary = "git"
```

Resolution stops at the nearest Git root by default; it crosses that boundary only when the parent configuration itself explicitly sets `boundary = "filesystem"`. Use `pinset current node --explain` or `pinset which node --explain` to inspect every candidate and any traditional files that remain explicit migration inputs only. Existing schema 1/2 state remains readable; preview and perform its schema-only upgrade with `pinset migrate --dry-run` and `pinset migrate`.

To migrate an existing repository, inspect traditional files locally first, then import and install their unambiguous selections:

```sh
pinset detect --json
pinset import
```

`detect` never uses the network or writes files. `import --no-install` writes the Pinset config and exact lock without downloading runtimes. Pinset scans from the working directory to the nearest Git repository root and does not modify or delete the source files.

Discover available and installed versions:

```sh
pinset list node --available
pinset list pnpm --available
pinset list
```

## Providers and platforms

| Provider | Commands | Windows x64 | Linux x64 | Linux ARM64 | macOS ARM64 |
| --- | --- | :---: | :---: | :---: | :---: |
| Node.js | `node`, `npm`, `npx`, `corepack` | ✓ | ✓ | ✓ | ✓ |
| pnpm | `pnpm` | ✓ | ✓ | ✓ | ✓ |
| Bun | `bun`, `bunx` | ✓ | ✓ | ✓ | ✓ |
| Go | `go`, `gofmt` | ✓ | ✓ | ✓ | ✓ |
| Python | `python`, `python3`, `pip`, `pip3` | ✓ | ✓ | ✓ | ✓ |
| Java (Temurin) | `java`, `javac`, `jar`, and JDK tools | ✓ | ✓ | ✓ | ✓ |
| Rust stable | `rustc`, `cargo`, `rustdoc`, `rustfmt`, `clippy-driver` | ✓ | ✓ | ✓ | ✓ |
| .NET SDK | `dotnet` | ✓ | ✓ | ✓ | ✓ |
| Flutter / bundled Dart | `flutter`, `dart` | ✓ | ✓ | — | ✓ |

Flutter does not publish an official Linux ARM64 SDK archive that matches Pinset's install model, so Pinset returns an explicit unsupported-target error instead of falling back to x64. macOS Intel is not a Pinset v1.0 release target.

## Command reference

See the complete [English command reference](docs/commands.md) or [Chinese command reference](docs/commands.zh-CN.md). It documents every command and subcommand, state changes, JSON support, exit codes, and common failures.

For product positioning and trade-offs, see the sourced [Pinset v1.5 comparison (Chinese)](docs/comparison.zh-CN.md).

## Future Roadmap

The roadmap describes direction, not promised versions or dates.

| Status | Direction |
| --- | --- |
| Planned | Expand lock auditing, stable failure reason codes, and compatibility diagnostics. |
| Exploring | Official distribution through Homebrew, Winget, Scoop, and similar channels. |
| Exploring | Additional platforms and architectures when upstream runtimes provide suitable artifacts. |
| Exploring | New Providers or a Provider extension mechanism, guided by real-world demand. |
| Ongoing | Stronger supply-chain verification, cache behavior, and diagnostics. |

## Uninstall

Remove runtimes through Pinset first when you want ownership and reference checks:

```sh
pinset uninstall node@22.0.0 --dry-run
pinset uninstall node@22.0.0
pinset prune --dry-run
```

To remove Pinset itself, delete `pinset` and `pinset-shim` from the directory where you installed them, then remove the shell-profile line you added. Delete `PINSET_HOME` (normally `~/.local/share/pinset` on Unix) only if you also intend to remove Pinset-owned runtimes, caches, and global state. Project `pinset.toml`, `pinset.lock`, and `.venv` files are not removed automatically.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a change. Security reports should follow [SECURITY.md](SECURITY.md).

## License

Pinset is available under the [MIT License](LICENSE).
