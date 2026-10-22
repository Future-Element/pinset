# Pinset 3

[English](README.md) · [简体中文](README.zh-CN.md) · [Command reference](docs/commands.md)

Pinset locks project toolchains and explains their execution paths. Select, upgrade or switch versions with `use`, then build and test using your project commands. The 3.0 source tree introduces a breaking protocol; a source version does not imply a published release.

Eight built-in Providers cover Node.js, pnpm, Bun, Go, Python, complete Temurin OpenJDK, Rust, and Flutter/Dart. npm comes with Node; pip comes with a project-root standard-library `.venv`; Dart comes with Flutter. Pinset does not run tasks or orchestrate workspaces.

Additional precompiled CLI tools can use the [generic binary Provider](docs/commands.md#generic-binary-provider): declare an HTTPS URL template, archive format and executable path under `[generic.<name>]`, then select an exact version with `use`. Installation follows locked URLs and SHA-256 digests; managed entries route these tools like built-ins. Source builds and script installers are unsupported.

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

Commit configuration, lock and encrypted profiles. Ignore only `.pinset/local/` and `.venv/`. Global state lives under `PINSET_HOME/v3/` (default `~/.pinset/v3/`). Use `pinset import --plan` to review native version files such as `.tool-versions`, `.nvmrc` and `package.json`, then `pinset import` to confirm the migration. It adds missing selections and their exact locks while retaining existing selections and source files; install tools separately with `pinset install`. For a 2.x upgrade, use the [standalone upgrade script](docs/migration-v2-to-v3.md): it installs 3.x, migrates exact versions, verifies environments, switches command entries and cleans up legacy files with recovery backups. It is not a Pinset command.

Project discovery stops at the nearest Git repository, including worktrees. Non-Git discovery stops at the home directory; outside it, only the specified directory is checked. The nearest project is independent of parent configurations. Commands prefer explicit project selections and use the global default for an omitted tool, so `init` alone does not hide an existing global pnpm. Required runtimes prefer the project's own selection; otherwise they use the global default. Project policy still applies to inherited tools. Set `require_project_tools = true` in `[policy]` to reject omitted tools. Invalid project configuration, locks or explicit selections fail instead of falling back.

## Installation and execution

`use` resolves official artifacts and updates configuration and lock together. `install` uses the existing lock without resolving versions. Offline cache hits still verify digests. `remove` removes selections; `clean` removes unreferenced installations conservatively. `exec` and shims do not install tools or initiate downloads.

Java installs the complete Temurin **JDK**, including version-specific public tools, sources, headers, modules, native libraries, certificates and licenses. It is a first-class Provider for standalone applications, servers, wrappers, debugging and diagnostics. Flutter Android is a separate integration. Project wrappers and dependencies remain project-owned. IDE project JDK and language-server runtime JDK are separate settings.

Python projects use one owned `.venv` created with the locked interpreter's standard library, with pip and no system/user package inheritance. Changing Python retains the old environment until `pinset install --recreate-venv`; external environments cannot be adopted. Pinset does not integrate uv or lock application packages.

Partial installs bind only the requested tools; installing pnpm also installs its locked Node dependency. An unavailable unrelated Python or Flutter selection does not prevent an installed Node entry from running. Python's versioned `python3.<minor>` and `pip3.<minor>` entries use the same project environment and reject a different selected minor. Routing rechecks the current verification policy as well as the exact lock.

Shell integration is explicit: `pinset self shell bash` (also zsh, fish, powershell) prints the PATH fragment and never edits a profile. The installer verifies a platform ZIP and places the matching CLI and router together. Pinset 3 source builds can be installed locally with `pinset self repair` after compiling both binaries.

Provider command entries are always in `PINSET_HOME/v3/bin`, including when the CLI uses a custom install directory. Put this managed directory first on PATH using `pinset self shell`; earlier 2.x launchers in other directories can otherwise shadow the new entries. In PowerShell, apply the current-session fragment with `pinset self shell powershell | Out-String | Invoke-Expression`.

## Evidence and encrypted profiles

`check` is read-only by default. `--deep` inspects installed payloads; `--probe` explicitly runs bounded probes and records the host, executable, exact version and time. A configured binding is not an observation of an IDE, daemon, compiler override or user build.

Use `pinset check --deep --probe --format json` for a structured CI report. It includes expected and observed versions, receipt/lock digest agreement, installed payload verification and an overall `passed` flag. A failed check exits with status 1 after printing the report. Without `--probe`, the actual version is `null`; without `--deep`, installed payload verification is `null`. The JSON contract is versioned and evolves through additive fields.

Profiles use per-value age encryption. Private identities are held in the OS credential store or explicit `PINSET_IDENTITY`; no private-key file is created. Trust binds the project, directory identity and configuration/profile fingerprint. External changes invalidate trust. Variable collisions and overrides of managed toolchain/control variables are refused. See [environment security](docs/environment.md).

Upgrade and switch versions directly with `use`; select an earlier exact version to switch back. `self repair` recovers interrupted project/global transactions and CLI/router updates. Completed version changes, source, application packages, databases and IDE processes are outside interrupted transaction recovery.

Interrupted venv replacement and managed command publication are also recoverable. Repair retains a live venv writer's files and asks you to retry after it exits. Managed binaries are atomically replaced; unknown external entries are never adopted. VS Code bindings require the actual project root to be an opened folder, including independent folders in a multi-root workspace.

## GitHub Actions

The official `setup-pinset` Action installs the requested Pinset version (latest by default), caches tool installations, restores the committed lock and checks the resulting toolchain on Linux, macOS and Windows. Managed tool entries are available to subsequent steps. See [CI setup and JSON reports](docs/ci.md) for inputs, cache behavior and report archiving.

## Development and release

The Rust workspace has five crates: pure `pinset-core`, mutating `pinset-engine`, encrypted `pinset-env`, thin `pinset-cli`, and read-only `pinset-shim`. Built-in and generic Providers share the installer, cache, extraction, locking and transaction services. XZ decoding uses pure Rust.

Local checks run in **local Docker** through `verify.ps1 -Suite fast|acceptance|platform|integrations|x64|all|hotfix` or `./verify.sh <suite>`. The original checkout is mounted read-only; fresh identities, trust, projects and venvs are created per run. Native verification on other platforms uses a separate manually triggered CI workflow. See [verification](scripts/docker/README.md) and [architecture](docs/architecture.md).

The user-selected local 3.0.9 scope is `-Suite x64`: fast checks, real Linux x64 SDK acceptance, Windows x64 target compilation and integrations inside Linux x64 Docker. It excludes ARM64 compilation, QEMU setup/execution and native ARM64 tests, plus macOS compilation and native tests. Windows x64 compilation does not establish native Windows runtime behavior. This scope does not invoke `all`; `all` retains its full-platform meaning. To reuse an explicit SDK baseline, pass `-SdkSelections output/verify-v3-<UTC>/frozen-selectors.json`; the report records its path, digest and exact selections without skipping the selected suite's install, checksum or runtime checks.

By user request, large Flutter/Android SDK download tests are exempt: actual Flutter execution and APK builds remain unverified. Metadata, locks, routing and Android evidence contracts are required; reports and the release gate record this boundary explicitly.

The manual release workflow validates a clean commit-bound local `all`, explicitly authorized `x64` or `hotfix` report, compiles four platform artifacts, packages the extension, generates checksums/SBOM/provenance and publishes. That workflow does not run tests, lint, scans, independent typechecks or probes. Release compilation does not extend local runtime evidence.

The independent [native-platform verification workflow](.github/workflows/verify-platforms.yml) is available only by manual dispatch. `mode=source` builds and checks candidate source; `mode=release` checks published packages and the `setup-pinset` Action, with `version=latest` by default or an explicit version. It covers native Linux x64/ARM64, Windows x64 and macOS ARM64 runners using `scripts/tests/ci_platform.py`. It is not triggered by every push, pull request or publication. The user has authorized this separate full-platform verification for 3.0.9; results belong to its own reports. Local work remains limited to the current system's scope, without QEMU setup or unrelated-platform tests.

MIT license. Report security issues privately as described in [SECURITY.md](SECURITY.md).
