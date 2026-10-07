# Pinset 3

[English](README.md) · [简体中文](README.zh-CN.md) · [Command reference](docs/commands.md)

Pinset locks project toolchains and explains their execution paths. Select, upgrade or switch versions with `use`, then build and test using your project commands. The 3.0 source tree introduces a breaking protocol; a source version does not imply a published release.

Eight built-in Providers cover Node.js, pnpm, Bun, Go, Python, complete Temurin OpenJDK, Rust, and Flutter/Dart. npm comes with Node; pip comes with a project-root standard-library `.venv`; Dart comes with Flutter. Pinset does not run tasks or orchestrate workspaces.

```sh
pinset init
pinset use node@lts pnpm@10 python@3.13 java@21
pinset which javac --explain
pinset check
pinset check --probe
pinset exec -- ./mvnw verify
pinset use java@25
pinset exec -- ./mvnw verify
```

## Project files

```text
.pinset/config.toml       selections, policy, public profile metadata
.pinset/lock.toml         exact versions, builds, platform artifacts and digests
.pinset/env/<profile>.env age-encrypted values; safe to commit
.pinset/local/           ignored bindings and local state
.venv/                  ignored project Python packages
```

Commit configuration, lock and encrypted profiles. Ignore only `.pinset/local/` and `.venv/`. Global state lives under `PINSET_HOME/v3/` (default `~/.pinset/v3/`). Old files and data are neither imported nor removed.

Project discovery stops at the nearest Git repository, including worktrees. Non-Git discovery stops at the home directory; outside it, only the specified directory is checked. The nearest project is independent and never merges parent or global selections. Missing project tools fail explicitly. Global defaults apply only outside a project or through an explicit global command.

## Installation and execution

`use` resolves official artifacts and updates configuration and lock together. `install` uses the existing lock without resolving versions. Offline cache hits still verify digests. `remove` removes selections; `clean` removes unreferenced installations conservatively. `exec` and shims do not install tools or initiate downloads.

Java installs the complete Temurin **JDK**, including version-specific public tools, sources, headers, modules, native libraries, certificates and licenses. It is a first-class Provider for standalone applications, servers, wrappers, debugging and diagnostics. Flutter Android is a separate integration. Project wrappers and dependencies remain project-owned. IDE project JDK and language-server runtime JDK are separate settings.

Python projects use one owned `.venv` created with the locked interpreter's standard library, with pip and no system/user package inheritance. Changing Python retains the old environment until `pinset install --recreate-venv`; external environments cannot be adopted. Pinset does not integrate uv or lock application packages.

Shell integration is explicit: `pinset self shell bash` (also zsh, fish, powershell) prints the PATH fragment and never edits a profile. The installer verifies a platform ZIP and places the matching CLI and router together. Pinset 3 source builds can be installed locally with `pinset self repair` after compiling both binaries.

Provider command entries are always in `PINSET_HOME/v3/bin`, including when the CLI uses a custom install directory. Put this managed directory first on PATH using `pinset self shell`; earlier 2.x launchers in other directories can otherwise shadow the new entries. In PowerShell, apply the current-session fragment with `pinset self shell powershell | Out-String | Invoke-Expression`.

## Evidence and encrypted profiles

`check` is read-only by default. `--deep` inspects installed payloads; `--probe` explicitly runs bounded probes and records the host, executable, exact version and time. A configured binding is not an observation of an IDE, daemon, compiler override or user build.

Profiles use per-value age encryption. Private identities are held in the OS credential store or explicit `PINSET_IDENTITY`; no private-key file is created. Trust binds the project, directory identity and configuration/profile fingerprint. External changes invalidate trust. Variable collisions and overrides of managed toolchain/control variables are refused. See [environment security](docs/environment.md).

Upgrade and switch versions directly with `use`; select an earlier exact version to switch back. `self repair` recovers interrupted project/global transactions and CLI/router updates. Completed version changes, source, application packages, databases and IDE processes are outside interrupted transaction recovery.

## Development and release

The Rust workspace has five crates: pure `pinset-core`, mutating `pinset-engine`, encrypted `pinset-env`, thin `pinset-cli`, and read-only `pinset-shim`. Eight Providers share the installer, cache, extraction, locking and transaction services. XZ decoding uses pure Rust.

All checks run in **local Docker** through `verify.ps1 -Suite fast|acceptance|platform|integrations|all` or `./verify.sh <suite>`. The original checkout is mounted read-only; fresh identities, trust, projects and venvs are created per run. See [verification](scripts/docker/README.md) and [architecture](docs/architecture.md).

By user request, large Flutter/Android SDK download tests are exempt: actual Flutter execution and APK builds remain unverified. Metadata, locks, routing and Android evidence contracts are required; reports and the release gate record this boundary explicitly.

CI is manual publication only: validate a clean commit-bound local `all` report, compile four platform artifacts, package the extension, generate checksums/SBOM/provenance and publish. It does not run tests, lint, scans, independent typechecks or probes. Linux x64 is the native runtime acceptance platform; Linux ARM64 runs under QEMU. Windows x64 and macOS ARM64 native behavior remains explicitly outside Docker acceptance.

MIT license. Report security issues privately as described in [SECURITY.md](SECURITY.md).
