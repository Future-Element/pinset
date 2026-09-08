# Pinset command reference

[English](commands.md) | [简体中文](commands.zh-CN.md) · [README](../README.md)

This document describes the current Pinset development command-line contract. Run `pinset <command> --help` for the exact parser help shipped with your binary.

## Conventions

### Selections and scope

A selection has the form `<tool>@<selector>`, for example `node@22`, `pnpm@latest`, `java@lts`, or `rust@stable`. Project configuration keeps that requested selector and lock schema 4 records its exact resolved version, options, and platform artifacts.

Schema 5 projects may add structured options without changing the string selection:

```toml
[tools]
rust = "nightly"

[tool-options.rust]
profile = "minimal"
components = ["rustfmt", "clippy"]
targets = ["wasm32-unknown-unknown"]
date = "2026-07-16"
```

Rust supports `minimal`, `default`, and `complete` profiles. `components` adds verified components to the selected profile, while `targets` installs additional `rust-std` compilation targets. A fixed nightly may use `rust = "nightly"` with `date`, or `rust = "nightly-YYYY-MM-DD"`; a floating undated nightly is rejected. Different option sets use distinct installation identities, while projects without options keep the historical version-only path.

Java uses Eclipse Temurin and defaults to a JDK. Schema 5 can select the smaller JRE package while keeping the same version selector:

```toml
[tools]
java = "lts"

[tool-options.java]
distribution = "temurin"
package = "jre"
```

The distribution and package type are part of the lock and installation identity, so a JDK and JRE at the same exact release can coexist.

Python keeps the compatible default `.venv` and can declare additional isolated environments for task binding:

```toml
[tools]
python = "3.14"

[python.environments.docs]
path = ".venv-docs"

[tasks.docs]
command = ["mkdocs", "serve"]
python-environment = "docs"
```

Supported tools are Node.js, pnpm, Bun, Go, Python, Java, Rust, .NET, and Flutter. Dart is provided by the selected Flutter SDK. Project discovery stops at the nearest Git root by default; without a Git marker it inspects only the start directory. A project is strict by default: an undeclared tool neither inherits global state nor falls back to the system command unless `[policy]` explicitly enables `inherit-global` or `system-fallback`. Outside a project, global state then system `PATH` remain eligible.

The global `--lang <en|zh-CN>` option selects output language for one invocation. Running `pinset --lang <language>` without a subcommand saves the default language.

### State

- Project selection: `pinset.toml` and `pinset.lock`.
- Global selection: `PINSET_HOME/state/global.toml` and `global.lock`.
- Known-project protection records: `PINSET_HOME/state/projects`. Project `use`, `import`, and locked installation register the canonical config path so `uninstall` and `prune` can protect selections outside the current working tree.
- Local machine settings, sources, download cache, installations, and receipts: under `PINSET_HOME`.
- `--cwd <path>` starts project discovery at that path.
- `--dry-run` reports a planned destructive operation without applying it.

### JSON schema 1

Only commands marked **Yes** below accept `--json`. They write one JSON document to standard output:

```json
{"schema":1,"command":"current","ok":true,"data":{}}
```

```json
{"schema":1,"command":"current","ok":false,"error":{"code":"runtime_missing","message":"...","details":{}}}
```

`command` is stable and nested commands use names such as `cache.verify`. Error `code` values are stable snake_case identifiers; localized `message` text is for people, and `details` is sanitized automation context. JSON mode also applies to argument, configuration, metadata, installation, and integrity failures.

### Exit codes

- `0`: Pinset completed successfully.
- `1`: `pinset lock audit` completed and found one or more errors or warnings that require action. Informational findings alone still return `0`.
- `2`: Pinset usage, configuration, metadata, integrity, or installation failure.
- `pinset exec` and `pinset x`: return the exact child-process exit code after a successful launch; Pinset failures before launch return `2`.

The tables below repeat exceptional behavior where it matters. Otherwise the command follows these exit codes.

## Project and selection commands

### `init`

| Field | Description |
| --- | --- |
| Purpose | Create a minimal project configuration in the current directory. |
| Syntax and arguments | `pinset init`; no command-specific options. |
| Modifies state | **Yes.** Creates schema 5 `pinset.toml` with a unique `project-id` and strict project policy; it does not select or install a runtime. |
| Example | `mkdir app && cd app && pinset init` |
| JSON | No. |
| Exit | `0` success; `2` if the file cannot be safely created. |
| Key errors | Existing configuration, unsafe path, or filesystem permission failure. |

### `detect`

| Field | Description |
| --- | --- |
| Purpose | Read traditional project version files and report selections, constraints, ignored tools, unsupported values, and conflicts. |
| Syntax and arguments | `pinset detect [--cwd <path>] [--json]`. Discovery stops at the nearest `.git` file/directory; without one it scans only the start directory. |
| Modifies state | **No.** It does not use the network, create Pinset state, execute third-party tools, or modify source files. |
| Example | `pinset detect --cwd ./app --json` |
| JSON | **Yes.** Data contains `start`, `boundary`, `target_config`, `can_import`, and stable Provider-ordered `findings`. |
| Exit | `0` when the local scan completes, including reports with conflicts or no importable selection; `2` only when discovery itself cannot start. |
| Key errors | Missing/inaccessible start directory. Unsafe, malformed, or unrepresentable source files are report findings rather than command errors. |

Recognized selection sources include `.nvmrc`, `.node-version`, `.bun-version`, `.go-version`, `.python-version`, `.java-version`, `.sdkmanrc`, `rust-toolchain(.toml)`, `global.json`, `.fvmrc`, legacy FVM project JSON, `.tool-versions`, `mise.toml`, and unambiguous fields in `package.json`, `go.mod`, and `go.work`. Version ranges from package manifests are informational only. Symlinks, non-files, non-UTF-8 sources, and files larger than 1 MiB are rejected in the report.

### `import`

| Field | Description |
| --- | --- |
| Purpose | Re-scan and import every safe traditional selection into schema 5 `pinset.toml` and schema 4 `pinset.lock`. |
| Syntax and arguments | `pinset import [--cwd <path>] [--force] [--no-install]`. `--force` replaces only discovered tools whose existing requested selector differs. |
| Modifies state | **Yes.** Resolves metadata, atomically replaces the lock file and then the config file, and installs all project selections by default. `--no-install` skips runtime archives and Python `.venv`, but still resolves and locks metadata. |
| Example | `pinset import --no-install` |
| JSON | No. |
| Exit | `0` after a complete import/install; `2` for no selection, blockers, invalid existing Pinset state, resolution/write failure, or installation failure. |
| Key errors | Conflicting sources, unsupported values, missing/mismatched existing lock, version replacement without `--force`, or unavailable Provider metadata. |

Import never reads installed state from another runtime manager, executes manager tasks/hooks, or deletes legacy files. If installation fails after the state commit, the valid config and lock remain and `pinset install --locked` resumes installation.

### `global`

| Field | Description |
| --- | --- |
| Purpose | Show global selections or batch-set any combination of global runtime defaults. |
| Syntax and arguments | `pinset global [<tool>@<selector>...] [--no-install]`. With no selections it remains read-only; `--no-install` requires at least one selection. A Provider may appear only once per batch. |
| Modifies state | Without selections: **No**. With selections: **Yes.** Pinset resolves the complete batch before one config/lock update, preserves unmentioned selections, then performs one locked installation pass unless `--no-install` is used. |
| Example | `pinset global node@lts python@latest rust@stable` |
| JSON | No. |
| Exit | `0` success; `2` on Pinset failure. |
| Key errors | Duplicate/unsupported Provider, invalid selector, unavailable metadata, untrusted manifest, dependency or policy failure, download/integrity failure, or unsupported target. A pre-commit failure changes no selection; an installation failure retains the complete new lock and reports the global locked-install retry command. |

### `use`

| Field | Description |
| --- | --- |
| Purpose | Resolve and lock one or more runtimes for the nearest project, or for global scope. |
| Syntax and arguments | `pinset use <tool>@<selector>... [--no-install] [--global]`. At least one selection is required and a Provider may appear only once. |
| Modifies state | **Yes.** Resolves every selection before one scope config/lock update, preserves unmentioned selections, then performs one dependency-ordered locked installation pass unless `--no-install` is used. |
| Example | `pinset use java@lts dotnet@lts flutter@latest` |
| JSON | No. |
| Exit | `0` success; `2` on Pinset failure. |
| Key errors | Missing project config, duplicate Provider, invalid selector, metadata/signature, dependency or project-policy failure, unsupported platform, or installation failure. A pre-commit failure changes no selection; an installation failure retains the complete new lock and reports the locked-install retry command. |

### `unset`

| Field | Description |
| --- | --- |
| Purpose | Remove one project or global selection without uninstalling its runtime. |
| Syntax and arguments | `pinset unset <tool> [--global | --cwd <path>]`. |
| Modifies state | **Yes.** Updates the chosen config and lock only. |
| Example | `pinset unset python --cwd ./app` |
| JSON | No. |
| Exit | `0` success; `2` on invalid scope or write failure. |
| Key errors | Unsupported tool, no matching project, missing selection, or config/lock write failure. |

### `install`

| Field | Description |
| --- | --- |
| Purpose | Install one explicit exact runtime, or install every target from a project/global lock. |
| Syntax and arguments | `pinset install [<tool>@<exact-version>] [--locked] [--offline] [--global | --cwd <path>]`. An explicit selection conflicts with lock-scope options; locked installation is the default project behavior. `--offline` is valid only with a project or global lock. |
| Modifies state | **Yes.** Writes cache entries, runtime files, receipts, and command routes; a locked Python project may create or validate `.venv`. It does not change a selection. |
| Example | `pinset install --locked --cwd ./app` |
| JSON | No. |
| Exit | `0` success; `2` on Pinset failure. |
| Key errors | Non-exact explicit version, config/lock mismatch, legacy Node lock requiring relock, missing signature, integrity failure, unsafe archive, or install transaction failure. |

## Query and lifecycle commands

### `which`

| Field | Description |
| --- | --- |
| Purpose | Print the exact executable Pinset would use for a command. |
| Syntax and arguments | `pinset which <command> [--cwd <path>] [--explain] [--json]`. |
| Modifies state | No. |
| Example | `pinset which node --json` |
| JSON | **Yes**; command name `which`. With `--explain`, `data.explanation` includes the boundary, candidate chain, policy result, and traditional migration-only sources. |
| Exit | `0` when resolved; `2` when no usable command can be resolved. |
| Key errors | Unknown managed command, missing selected runtime, invalid lock, or no eligible system fallback. |

### `current`

| Field | Description |
| --- | --- |
| Purpose | Show the effective project, global, or system runtime selection and executable. |
| Syntax and arguments | `pinset current [tool] [--cwd <path>] [--explain] [--json]`; the default tool is Node.js. |
| Modifies state | No. |
| Example | `pinset current python --cwd ./app` |
| JSON | **Yes**; command name `current`, including both `requested` and exact `version`; `--explain` adds the resolution trace. |
| Exit | `0` when resolved; `2` when selection or installation is unusable. |
| Key errors | Unsupported tool, invalid config/lock, missing runtime, or blocked system fallback. |

### `list`

| Field | Description |
| --- | --- |
| Purpose | List installed versions, or query official available versions for one Provider. |
| Syntax and arguments | `pinset list [tool] [--available] [--json]`. `--available` requires `tool`. |
| Modifies state | No. |
| Example | `pinset list java --available --json` |
| JSON | **Yes**; command name `list`, with versions under `data.versions`. |
| Exit | `0` success; `2` on argument or metadata failure. |
| Key errors | Unsupported Provider, network/metadata failure, invalid or untrusted signed metadata, or response limit exceeded. |

### `outdated`

| Field | Description |
| --- | --- |
| Purpose | Compare each exact locked version with the newest version compatible with its requested selector and with the latest stable release. |
| Syntax and arguments | `pinset outdated [tool] [--global | --cwd <path>] [--json]`. |
| Modifies state | No. |
| Example | `pinset outdated --cwd ./app --json` |
| JSON | **Yes**; command name `outdated`, with `requested`, `current`, `latest_compatible`, `latest`, `update_available`, and `upgrade_available` under `data.runtimes`. |
| Exit | `0` after a complete comparison; `2` if scope, lock, or metadata validation fails. |
| Key errors | Missing project, unsupported tool, invalid lock, or Provider metadata failure. |

### `update`

| Field | Description |
| --- | --- |
| Purpose | Re-resolve requested selectors and refresh exact lock records without changing selectors or installing runtimes. |
| Syntax and arguments | `pinset update [tool] [--global | --cwd <path>] [--dry-run] [--json]`. |
| Modifies state | **Yes**, unless `--dry-run`; updates only the selected lockfile. |
| Example | `pinset update node --cwd ./app --dry-run` |
| JSON | **Yes**; command name `update`, with previous/resolved exact versions and requested selector. |
| Exit | `0` after comparison/write; `2` on scope, lock, or Provider metadata failure. |
| Key errors | Missing project/selection, invalid lock, unsupported tool, or unavailable metadata. |

### `migrate`

| Field | Description |
| --- | --- |
| Purpose | Validate and rewrite schema 1–4 project configuration as schema 5 and runtime locks as schema 4. Schema-only changes preserve comments and use atomic replacement. It also repairs safely recognized pre-1.0 Provider records at their existing exact versions. Use `--global` to migrate an old global lock manually. |
| Syntax and arguments | `pinset migrate [--global | --cwd <path>] [--dry-run] [--json]`. |
| Modifies state | **Yes**, unless `--dry-run`; normalizes the config and lock with atomic per-file replacement only. |
| Example | `pinset migrate --cwd ./app --dry-run` |
| JSON | **Yes**; command name `migrate`, including source and target schemas. |
| Exit | `0` after validation/migration; `2` when config and lock cannot be proven consistent. |
| Key errors | Missing config/lock, unsupported schema, config-lock mismatch, or write failure. |

### `lock audit`

| Field | Description |
| --- | --- |
| Purpose | Audit one project or global configuration/lock pair, its current-platform artifacts, relevant content-addressed cache entries, install receipts, and receipt-backed ownership. Project Python selections also audit the `.venv` ownership marker. |
| Syntax and arguments | `pinset lock audit [--global | --cwd <path>] [--json]`. Project scope is the default and follows normal repository-bounded discovery. |
| Modifies state | **No.** The command is always read-only, never runs a repair plan, and never contacts Provider metadata or archive services. Cache checks hash only entries referenced by the selected current-platform artifacts. |
| Example | `pinset lock audit --cwd ./app --json` |
| JSON | **Yes**; command name `lock.audit`. A completed audit uses `ok: true` even when `data.passed` is false. Stable `reason_code`, `severity`, `category`, `subject`, optional `path`, and optional `repair` fields are returned under `data.findings`. |
| Exit | `0` when there are no errors or warnings; `1` when the audit completes with action-required errors/warnings; `2` only when command parsing or audit startup itself fails. An optional cache miss is informational and does not cause exit `1`. |
| Key findings | Missing/invalid/legacy configuration or lock state, selector drift, unsupported Providers, missing current-platform artifacts, missing/corrupt/unsafe cache entries, missing/unsafe installations, invalid or mismatched receipts, and invalid Python environment ownership. |

Stable reason codes are grouped as follows:

- Configuration and lock: `config_missing`, `config_invalid`, `config_schema_legacy`, `lock_missing`, `lock_invalid`, `lock_schema_legacy`, `lock_tool_missing`, `lock_tool_unconfigured`, `lock_selector_mismatch`.
- Provider and platform: `provider_unsupported`, `provider_audit_unsupported`, `platform_artifact_missing`, `platform_artifact_invalid`.
- Cache: `cache_entry_missing`, `cache_entry_corrupt`, `cache_entry_unsafe`, `cache_entry_unreadable`.
- Receipt and ownership: `install_missing`, `install_path_unsafe`, `receipt_missing`, `receipt_unreadable`, `receipt_invalid`, `receipt_schema_legacy`, `receipt_schema_unsupported`, `receipt_incomplete`, `receipt_identity_mismatch`, `receipt_integrity_missing`, `receipt_integrity_mismatch`, `receipt_overlay_mismatch`, `python_environment_missing`, `python_environment_ownership_invalid`.

### `uninstall`

| Field | Description |
| --- | --- |
| Purpose | Remove one exact Pinset-owned runtime installation. |
| Syntax and arguments | `pinset uninstall <tool>@<exact-version> [--force] [--cwd <path>] [--dry-run] [--json]`. |
| Modifies state | **Yes**, unless `--dry-run`. Deletes only an installation with valid Pinset ownership evidence. |
| Example | `pinset uninstall node@22.0.0 --dry-run --json` |
| JSON | **Yes**; command name `uninstall`. |
| Exit | `0` for a completed plan/removal; `2` when protection blocks it or validation fails. |
| Key errors | Non-exact version, selected runtime still referenced by the current, global, explicitly supplied, or locally registered project state, missing/invalid receipt, unsafe path, or non-owned installation. `--force` bypasses selection references, not ownership checks. Projects never used by this Pinset installation cannot be discovered automatically. |

### `prune`

| Field | Description |
| --- | --- |
| Purpose | Remove installed versions not protected by global, supplied, or locally registered project selections. |
| Syntax and arguments | `pinset prune [--cwd <path>] [--project <path>]... [--dry-run] [--json]`. |
| Modifies state | **Yes**, unless `--dry-run`. |
| Example | `pinset prune --project ./app --project ../service --dry-run` |
| JSON | **Yes**; command name `prune`. |
| Exit | `0` for a completed plan/removal; `2` if references or ownership cannot be validated. |
| Key errors | Invalid project/registry state, unsafe installation path, missing receipt, or filesystem failure. Projects never used by this Pinset installation must still be supplied with `--project`. |

### `exec`

| Field | Description |
| --- | --- |
| Purpose | Run a child command with Pinset's selected runtimes and environment without relying on direct shell routing. |
| Syntax and arguments | `pinset exec [--cwd <path>] -- <command> [args...]`. An optional exact tool selection may lead the child command, for example `pinset exec node@22.0.0 -- node -v`. |
| Modifies state | Pinset state: **No**. The launched program may modify its own files or external state. |
| Example | `pinset exec -- node ./scripts/build.js` |
| JSON | No; child stdout/stderr remains unwrapped. |
| Exit | Exact child exit code after launch; `2` if Pinset cannot resolve or launch it. |
| Key errors | Missing command, unresolved runtime, exact override not installed, shim recursion protection, or process launch failure. |

### `x`

| Field | Description |
| --- | --- |
| Purpose | Resolve, verify, install, and run one Provider command without changing project/global selection state. |
| Syntax and arguments | `pinset x <tool>@<selector> [--cwd <path>] -- <command> [args...]`. The command must belong to the selected Provider. |
| Modifies state | Selection state: **No**. Verified downloads, cache entries, installation receipts, and installed runtimes under `PINSET_HOME` may be created. The launched program may modify its own files or external state. |
| Example | `pinset x node@24 -- node ./scripts/build.js` |
| JSON | No; child stdout/stderr remains unwrapped. |
| Exit | Exact child exit code after launch; `2` if Pinset cannot resolve, verify, install, or launch it. |
| Key errors | Invalid selector, command/Provider mismatch, failed metadata or artifact verification, missing declared Provider dependency, unsupported platform, or process launch failure. pnpm requires a valid project/global Node.js selection. |

### `doctor`

| Field | Description |
| --- | --- |
| Purpose | Diagnose the project boundary and strict policy, lockfile, installation, command routing, environment, PATH state, and traditional migration-only sources. |
| Syntax and arguments | `pinset doctor [--cwd <path>] [--json]`. |
| Modifies state | No. |
| Example | `pinset doctor --json` |
| JSON | **Yes**; command name `doctor`. |
| Exit | `0` when the diagnostic completes; `2` if its inputs cannot be read or validated. Findings are reported in data and do not necessarily make the command fail. |
| Key errors | Unreadable config/lock, malformed state, unsafe path, or filesystem failure. |

### `status` and `check`

| Field | Description |
| --- | --- |
| Purpose | Produce portable diagnostic report schema 1. `status` always reports; `check` is suitable for CI policy gates. |
| Syntax and arguments | `pinset <status|check> [--cwd <path>] [--json] [--save <file>] [--compare <file>] [--repair-preview]`. |
| Modifies state | Only `--save` writes the requested report file atomically. Repair preview never executes a command. |
| Example | `pinset check --save .pinset-diagnostic.json --repair-preview` |
| JSON | **Yes**; command name `status` or `check`. The report has its own schema field, independent of the CLI envelope. |
| Exit | `status` returns `0` after collection. `check` returns `1` for errors, warnings, or comparison changes. Invalid input returns `2`. |
| Privacy | Reports omit filesystem paths, environment values, encrypted payloads, checksums, and secret digests. Comparisons return changed JSON Pointer paths only. |

## Download cache commands

The cache stores verified archives by integrity identity. Cache inspection never treats a filename alone as proof of integrity. The GitHub Action caches only `PINSET_HOME/downloads` and runs `pinset cache verify` after every restore before installing locked runtimes; set its `cache` input to `"false"` to disable this behavior.

### `cache`

| Field | Description |
| --- | --- |
| Purpose | Group download-cache inspection, verification, repair, cleanup, and offline import operations. |
| Syntax and arguments | `pinset cache <list|info|verify|repair|clean|import|prefetch> ...`; a subcommand is required. |
| Modifies state | Depends on the subcommand: `repair`, `clean`, and `import` modify cache state. |
| Example | `pinset cache info` |
| JSON | No group-level output; `list`, `info`, `verify`, `repair`, and `clean` support `--json`. |
| Exit | `0` for successful subcommand completion; `2` for missing/invalid subcommand or cache failure. |
| Key errors | Missing subcommand, unsafe cache path, corruption, invalid integrity, or filesystem failure. |

### `cache prefetch`

Downloads the current-platform artifacts from the project lock into the verified content-addressed cache without extracting or installing them. `--jobs <1..16>` limits concurrent downloads and defaults to four. Lock validation happens before workers start, and worker failures are reported together.

```sh
pinset cache prefetch --jobs 4
```

### `bundle export` and `bundle import`

`pinset bundle export --output project.pinset-bundle.tar.gz [--target <target>]` creates bundle schema 1 from the project lock and verified cache files. Export fails if a required artifact is absent or corrupt. `pinset bundle import <file>` validates entry paths, format, target, lock identity, artifact sizes, and cryptographic identities before committing artifacts to the local cache. A bundle transports bytes only and never grants project or Provider trust.

After import, `pinset install --locked --offline` makes no network requests. It reports every missing current-platform artifact before changing installation state.

### `cache list`

| Field | Description |
| --- | --- |
| Purpose | List complete content-addressed runtime archives. |
| Syntax and arguments | `pinset cache list [--json]`. |
| Modifies state | No. |
| Example | `pinset cache list --json` |
| JSON | **Yes**; command name `cache.list`, entries under `data.entries`. |
| Exit | `0` success; `2` if cache metadata cannot be inspected. |
| Key errors | Unsafe cache entry or filesystem read failure. |

### `cache info`

| Field | Description |
| --- | --- |
| Purpose | Summarize complete and partial download-cache usage. |
| Syntax and arguments | `pinset cache info [--json]`. |
| Modifies state | No. |
| Example | `pinset cache info` |
| JSON | **Yes**; command name `cache.info`. |
| Exit | `0` success; `2` on cache inspection failure. |
| Key errors | Unreadable cache directory or invalid entry metadata. |

### `cache verify`

| Field | Description |
| --- | --- |
| Purpose | Hash every complete archive and compare it with its content identity. |
| Syntax and arguments | `pinset cache verify [--json]`. |
| Modifies state | No. |
| Example | `pinset cache verify --json` |
| JSON | **Yes**; command name `cache.verify`. Corruption is returned as an `ok: false` document. |
| Exit | `0` only when all entries verify; `2` for corrupt entries or inspection failure. |
| Key errors | Digest mismatch, truncated file, unsafe entry, or read failure. |

### `cache repair`

| Field | Description |
| --- | --- |
| Purpose | Remove corrupt complete archives so a later install can fetch them again. |
| Syntax and arguments | `pinset cache repair [--dry-run] [--json]`. |
| Modifies state | **Yes**, unless `--dry-run`; only verified-corrupt complete archives are targeted. |
| Example | `pinset cache repair --dry-run --json` |
| JSON | **Yes**; command name `cache.repair`. |
| Exit | `0` after planning/removal; `2` if entries cannot be safely classified or removed. |
| Key errors | Unsafe path, permission failure, or cache changing during verification. |

### `cache clean`

| Field | Description |
| --- | --- |
| Purpose | Remove complete content-addressed archives from the download cache. |
| Syntax and arguments | `pinset cache clean [--dry-run] [--json]`. |
| Modifies state | **Yes**, unless `--dry-run`; installed runtimes are not removed. |
| Example | `pinset cache clean --dry-run` |
| JSON | **Yes**; command name `cache.clean`. |
| Exit | `0` after planning/removal; `2` on unsafe path or filesystem failure. |
| Key errors | Non-owned/unsafe entry or deletion failure. |

### `cache import`

| Field | Description |
| --- | --- |
| Purpose | Import a reviewed archive into the verified offline cache. |
| Syntax and arguments | `pinset cache import <archive> (--sha256 <hex> | --integrity <SRI>)`; the integrity options conflict. |
| Modifies state | **Yes.** Copies a matching archive under its content identity; does not install it. |
| Example | `pinset cache import ./node.tar.xz --sha256 <reviewed-digest>` |
| JSON | No. |
| Exit | `0` after a verified import; `2` on argument, digest, or write failure. |
| Key errors | Missing expected integrity, digest mismatch, invalid SRI/SHA-256, unsafe source, or cache write failure. |

## Python environment commands

Pinset owns a project Python environment only when its ownership marker matches its declared name, path, selected CPython distribution, and target. The reserved `default` environment remains `.venv`; other names must be declared under `[python.environments.<name>]`. Destructive operations fail closed if ownership cannot be proven.

### `venv`

| Field | Description |
| --- | --- |
| Purpose | Group project-owned Python environment operations. |
| Syntax and arguments | `pinset venv <create|status|recreate> [name] ...`; `name` defaults to `default`. |
| Modifies state | Depends on the subcommand; `create` and `recreate` modify state. |
| Example | `pinset venv status` |
| JSON | No. |
| Exit | `0` for successful subcommand completion; `2` for missing/invalid subcommand or environment failure. |
| Key errors | Missing subcommand, missing project Python selection, invalid ownership marker, or environment creation failure. |

### `venv create`

| Field | Description |
| --- | --- |
| Purpose | Install the selected CPython runtime if needed, then create or validate one project environment. |
| Syntax and arguments | `pinset venv create [name] [--cwd <path>]`. |
| Modifies state | **Yes.** May install Python and create the selected environment plus its ownership marker. |
| Example | `pinset venv create docs --cwd ./app` |
| JSON | No. |
| Exit | `0` when the environment is ready; `2` on Pinset failure. |
| Key errors | No project Python selection, lock mismatch, unsupported target, install failure, existing foreign `.venv`, or marker mismatch. |

### `venv status`

| Field | Description |
| --- | --- |
| Purpose | Show the selected CPython distribution and managed project-environment path. |
| Syntax and arguments | `pinset venv status [name] [--cwd <path>]`. |
| Modifies state | No. |
| Example | `pinset venv status` |
| JSON | No. |
| Exit | `0` when status can be determined; `2` for invalid project or ownership state. |
| Key errors | Missing Python selection, invalid lock, missing/mismatched ownership marker, or unreadable environment. |

### `venv recreate`

| Field | Description |
| --- | --- |
| Purpose | Delete and recreate one project environment after proving Pinset ownership. |
| Syntax and arguments | `pinset venv recreate [name] [--cwd <path>]`. |
| Modifies state | **Yes.** Replaces only the correctly marked Pinset-owned environment. |
| Example | `pinset venv recreate docs --cwd ./app` |
| JSON | No. |
| Exit | `0` when recreated; `2` when validation or recreation fails. |
| Key errors | Missing/invalid ownership marker, path escape, selected Python mismatch, removal failure, or venv creation failure. |

## Command-routing commands

### `shim`

| Field | Description |
| --- | --- |
| Purpose | Group inspection and repair operations for Provider command routes. |
| Syntax and arguments | `pinset shim <path|install|migrate> ...`; a subcommand is required. |
| Modifies state | Depends on the subcommand; `install` and `migrate` modify routing entries. |
| Example | `pinset shim path` |
| JSON | No. |
| Exit | `0` for successful subcommand completion; `2` for missing/invalid subcommand or routing failure. |
| Key errors | Missing subcommand, unsafe routing path, ownership conflict, or missing shim binary. |

### `shim path`

| Field | Description |
| --- | --- |
| Purpose | Print the user-owned directory containing Pinset command shims. |
| Syntax and arguments | `pinset shim path`. |
| Modifies state | No. |
| Example | `pinset shim path` |
| JSON | No. |
| Exit | `0` success; `2` if the Pinset home/routing path is invalid. |
| Key errors | Missing home-directory context or unsafe configured path. |

### `shim install`

| Field | Description |
| --- | --- |
| Purpose | Repair command shims without overwriting files Pinset does not own. |
| Syntax and arguments | `pinset shim install [--binary <pinset-shim>] [--dir <path>] [--provider <tool> | <COMMAND>...]`. |
| Modifies state | **Yes.** Creates or repairs owned shim entries in the destination. |
| Example | `pinset shim install --provider node` |
| JSON | No. |
| Exit | `0` when requested routes are ready; `2` on validation/write failure. |
| Key errors | Unsupported Provider, invalid command name, missing shim binary, existing non-owned file, or permission failure. |

### `shim migrate`

| Field | Description |
| --- | --- |
| Purpose | Register configured Provider commands in the current routing directory while preserving existing entries. |
| Syntax and arguments | `pinset shim migrate [--provider <tool>] [--dir <path>]`. |
| Modifies state | **Yes.** Repairs routing entries only; this is not a config/lock migration command. |
| Example | `pinset shim migrate --provider python` |
| JSON | No. |
| Exit | `0` success; `2` if ownership or routing validation fails. |
| Key errors | Unsupported Provider, missing shim binary, non-owned conflicting entry, or filesystem failure. |

### `activate`

| Field | Description |
| --- | --- |
| Purpose | Print shell code that prepends Pinset's command-routing directory to `PATH`. |
| Syntax and arguments | `pinset activate <bash|zsh|fish|powershell>`. |
| Modifies state | No. The caller chooses whether to evaluate or save the printed code. |
| Example | `eval "$(pinset activate zsh)"` |
| JSON | No. |
| Exit | `0` success; `2` for invalid shell or path configuration. |
| Key errors | Unsupported shell value or invalid routing directory. |

### `completions`

| Field | Description |
| --- | --- |
| Purpose | Generate Pinset completion code for a supported shell. |
| Syntax and arguments | `pinset completions <bash|zsh|fish|powershell>`. |
| Modifies state | No; shell redirection may create a file. |
| Example | `pinset completions fish > ~/.config/fish/completions/pinset.fish` |
| JSON | No. |
| Exit | `0` success; `2` for an invalid shell value. |
| Key errors | Unsupported shell value or output write failure. |

## Source commands

Custom source configuration currently applies to Node.js, Go, Python, and Flutter. Archive mirrors and trusted metadata mirrors have different security authority: `--trust-metadata` is required before a custom HTTPS source may determine versions or integrity metadata. For Node.js, a trusted metadata source must also serve the signed manifest.

### `source`

| Field | Description |
| --- | --- |
| Purpose | Group local Provider source inspection, selection, policy, and validation operations. |
| Syntax and arguments | `pinset source <list|add|use|fallback|remove|test> ...`; a subcommand is required. |
| Modifies state | Depends on the subcommand; `add`, `use`, `fallback`, and `remove` modify local source configuration. |
| Example | `pinset source list` |
| JSON | No. |
| Exit | `0` for successful subcommand completion; `2` for missing/invalid subcommand or source failure. |
| Key errors | Missing subcommand, unsupported Provider, invalid URL/trust policy, unknown alias, or metadata validation failure. |

### `source list`

| Field | Description |
| --- | --- |
| Purpose | List built-in and custom sources, optionally for one Provider. |
| Syntax and arguments | `pinset source list [node|go|python|flutter]`. |
| Modifies state | No. |
| Example | `pinset source list node` |
| JSON | No. |
| Exit | `0` success; `2` on config/provider validation failure. |
| Key errors | Unsupported source Provider or malformed source configuration. |

### `source add`

| Field | Description |
| --- | --- |
| Purpose | Add a named custom archive source, optionally granting trusted metadata authority. |
| Syntax and arguments | `pinset source add <provider> <alias> --base-url <url> [--allow-insecure | --trust-metadata]`. HTTP requires `--allow-insecure`, which conflicts with metadata trust. |
| Modifies state | **Yes.** Writes local `sources.toml`; project lockfiles are unchanged. |
| Example | `pinset source add node mirror --base-url https://mirror.example/node` |
| JSON | No. |
| Exit | `0` success; `2` on URL, trust, or write failure. |
| Key errors | Unsupported Provider, reserved/duplicate alias, invalid URL, insecure URL without opt-in, or invalid trust combination. |

### `source use`

| Field | Description |
| --- | --- |
| Purpose | Select the active source for one supported Provider. |
| Syntax and arguments | `pinset source use <provider> <alias>`. |
| Modifies state | **Yes.** Updates local source configuration; existing lockfiles remain unchanged. |
| Example | `pinset source use go mirror` |
| JSON | No. |
| Exit | `0` success; `2` on lookup or write failure. |
| Key errors | Unknown alias, unsupported Provider, or invalid configuration. |

### `source fallback`

| Field | Description |
| --- | --- |
| Purpose | Replace the ordered fallback source list for one Provider. |
| Syntax and arguments | `pinset source fallback <provider> [alias]...`; pass no aliases to clear the list. |
| Modifies state | **Yes.** Replaces the local fallback order. |
| Example | `pinset source fallback python mirror-a mirror-b official` |
| JSON | No. |
| Exit | `0` success; `2` on validation/write failure. |
| Key errors | Unknown or duplicate alias, active-source conflict, unsupported Provider, or malformed configuration. |

### `source remove`

| Field | Description |
| --- | --- |
| Purpose | Remove an inactive custom source. |
| Syntax and arguments | `pinset source remove <provider> <alias>`. |
| Modifies state | **Yes.** Removes the local source entry. |
| Example | `pinset source remove flutter old-mirror` |
| JSON | No. |
| Exit | `0` success; `2` if removal is not allowed or cannot be saved. |
| Key errors | Built-in source, active source, referenced fallback, unknown alias, or unsupported Provider. |

### `source test`

| Field | Description |
| --- | --- |
| Purpose | Perform read-only connectivity and Provider metadata validation for one source. |
| Syntax and arguments | `pinset source test <provider> [alias]`; omitted alias means the active source. |
| Modifies state | No. |
| Example | `pinset source test node mirror` |
| JSON | No. |
| Exit | `0` only when connectivity and metadata validation succeed; `2` otherwise. |
| Key errors | Network failure, response limit, invalid metadata, missing/invalid Node signature, unknown signer, insecure policy, or unknown alias. |

## Provider Registry commands

Registry schema 2 supports a constrained `github-release-binary` backend. It fixes the upstream repository, release tag prefix, checksum asset, platform asset names, commands, revision, and disable state. A manifest cannot contain scripts, hooks, shell fragments, arbitrary download hosts, or environment code. `jq` is the first Provider resolved and installed by this generic backend. Active Registry files must be bounded regular files containing exactly one valid cleartext OpenPGP signature from Pinset's pinned Registry key.

### `provider list`

| Field | Description |
| --- | --- |
| Purpose | Verify and list the active declarative Provider Registry. |
| Syntax and arguments | `pinset provider list [--json]`. |
| Modifies state | No. It does not use the network, install runtimes, or execute manifest content. |
| Example | `pinset provider list --json` |
| JSON | **Yes**; command name `provider.list`, including whether a trusted file is active, the signed document, and signer fingerprint. |
| Exit | `0` when signature, schema, capabilities, dependency graph, and built-in declarations all verify; `2` otherwise. |
| Key errors | Invalid embedded key/signature, unknown capability, duplicate command, missing dependency, cycle, or declaration drift. |

### `provider verify`

| Field | Description |
| --- | --- |
| Purpose | Verify the embedded Registry or one local clear-signed Registry file without activating it. |
| Syntax and arguments | `pinset provider verify [REGISTRY] [--json]`; omitted path verifies the embedded Registry. |
| Modifies state | No. Local files are read only; no Provider is installed, activated, or executed. |
| Example | `pinset provider verify registry/providers.json.asc --json` |
| JSON | **Yes**; command name `provider.verify`, including the verified document and signer fingerprint. |
| Exit | `0` only after cryptographic, schema, capability, and dependency validation; `2` otherwise. |
| Key errors | Symlink/non-file input, input over 256 KiB, unsigned or multiply-signed data, signer mismatch, tampering, unknown field/capability, missing dependency, or cycle. |

### `provider status`, `trust`, and `untrust`

| Field | Description |
| --- | --- |
| Purpose | Inspect the active snapshot, activate a verified official snapshot, or return to the Registry embedded in the binary. |
| Syntax and arguments | `pinset provider status [--json]`; `pinset provider trust <REGISTRY> [--json]`; `pinset provider untrust [--json]`. |
| Modifies state | `status` does not. `trust` atomically writes `PINSET_HOME/config/provider-registry.asc` only after signature, schema, capability, and runtime-declaration validation. `untrust` removes that local selection. |
| Example | `pinset provider trust registry/providers.json.asc` |
| JSON | **Yes** for all three commands, using `provider.status`, `provider.trust`, and `provider.untrust`. |
| Exit | `0` after the resulting active Registry has been verified; `2` otherwise. |
| Key errors | Untrusted signer, tampering, declaration drift, disabled or missing Provider revision, unsafe input, or atomic write failure. |

### `provider validate` and `provider scaffold`

| Field | Description |
| --- | --- |
| Purpose | Validate unsigned JSON during contribution, or generate a constrained GitHub release binary manifest template. Neither command grants trust. |
| Syntax and arguments | `pinset provider validate <REGISTRY.json> [--json]`; `pinset provider scaffold <tool> --repository <owner/repository> [--command <name>]`. |
| Modifies state | No. |
| Example | `pinset provider scaffold jq --repository jqlang/jq --command jq` |
| JSON | `validate` uses command name `provider.validate`; `scaffold` prints the manifest JSON directly. |
| Exit | `0` for a valid bounded document or generated template; `2` otherwise. |
| Key errors | Unknown fields or capabilities, scripts/hooks represented as unknown fields, unsafe names, incomplete target mapping, duplicate commands, missing dependencies, or cycles. |

Locks for declarative Providers record the Provider id, revision, Registry signer fingerprint, repository, tag, five target assets, and exact SHA-256 identities. CLI and shim routing compare the lock to the active signed manifest every time. A signed revision change or disable flag therefore fails closed until the project explicitly resolves a new lock. `pinset uninstall jq@1.8.2` resolves a unique revision-bound installation identity; if multiple identities exist, Pinset asks for the exact identity shown by `pinset list jq`.

## Short execution (2.2)

```sh
pinset env init
pinset env use dev
pinset env set DATABASE_URL
pinset -- pnpm dev
pinset -e test -- pnpm test
pinset -C ./another-project -- pnpm dev
pinset --no-env -- node app.js
```

Put Pinset options before the top-level `--`; everything after it belongs to the child, including `--json`, `--lang`, and further `--` separators. `-C` / `--cwd` changes the invocation directory before project discovery. `-e` / `--profile` overrides the profile for execution or ordinary environment operations; it cannot be combined with `--no-env`. Legacy subcommand `--cwd` and `--profile` remain valid, with subcommand options taking precedence over root options.

The short entry resolves managed commands, project Python environment commands, explicit executable paths, then other commands on the system PATH. A configured but broken runtime is an error; it cannot silently fall back to a system version. Arbitrary commands require the explicit `--` boundary: `pinset typo` remains an error. Commands are argument arrays, not shell expressions; invoke a shell explicitly when shell syntax is needed. The existing `exec` and `x` syntax, runtime-only resolution, and child exit behavior remain available.

### `run`

`pinset run <task> [-- <arguments...>]` executes a task declared under `[tasks.<name>]`. A task contains a nonempty `command` string array and may add a project-relative `cwd`, a `profile`, and a human-readable `description`. Arguments after `--` are appended without shell parsing. The task's child exit status is preserved.

Task environment selection uses root `-e`, then `PINSET_ENV_PROFILE`, then the task profile, then machine-local and project defaults. `--no-env` disables injection. A missing task is always an error and never falls back to a system program. Task working directories must already exist within the project after canonical path resolution.

### `workspace`

Schema 5 workspaces declare explicit member paths in the root `pinset.toml`. Every member has its own `pinset.toml` and `pinset.lock`. Commands may run from the workspace root or any declared member:

| Command | Behavior |
| --- | --- |
| `pinset workspace members [--json]` | List members, effective tool selectors, and whether each selector comes from the root or member. |
| `pinset workspace install [--member <path>]... [--changed-since <git-ref>] [--offline]` | Install each selected member's independently locked effective tool set. |
| `pinset workspace check [--member <path>]... [--changed-since <git-ref>] [--json]` | Run strict diagnostics for selected members and return `1` when any member fails. |
| `pinset workspace run <task> [--member <path>]... [--changed-since <git-ref>] [-- <arguments...>]` | Run the named task sequentially and stop at the first nonzero child exit. |
| `pinset workspace update [--member <path>]... [--changed-since <git-ref>] [--json]` | Resolve and report candidate updates without changing member locks. |
| `pinset workspace references <tool> [--json]` | Show members that select a tool and the source of each selector. |

Root tools, tasks, Python environments, and environment settings provide member defaults. A member tool selector replaces the root selector and its complete structured options table; option arrays are never merged. A same-name member task replaces the root task. A member `[python]` or `[environment]` section replaces the corresponding root section. `--changed-since` selects members with Git changes below their declared paths; changing the root `pinset.toml` selects every member.

### `candidate`

`pinset candidate` prepares and verifies an exact replacement lock before it becomes the active project selection:

| Command | Behavior |
| --- | --- |
| `candidate prepare [tool] [--workspace] [--no-install] [--json]` | Re-resolve all selectors or one tool, save a candidate record under `PINSET_HOME`, and prepare every exact runtime unless `--no-install` is set. The active lock is unchanged. |
| `candidate test <task> [--workspace] [-- <arguments...>]` | Run a declared task with the candidate runtime directories, variables, and an isolated candidate Python environment; preserve and record the child exit code. |
| `candidate status [--workspace] [--json]` | Show candidate identity, exact-lock digest, and test records. |
| `candidate apply [--workspace] [--json]` | Apply the exact candidate from the latest passing test after rechecking project, lock, and Git baselines. |
| `candidate history [--json]` | List local application and restoration records for the current project. |
| `candidate restore [history-id] [--json]` | Restore the previous lock from the selected or latest history entry when the current state still matches it. |
| `candidate recover [--json]` | Complete history for fully applied interrupted transactions or roll a partially applied workspace transaction back to every previous member lock. |

Candidate apply writes a recovery journal before changing any lock. It rejects concurrent configuration or lock changes and uses one deterministic history entry per member. Candidate history covers Pinset-managed lock state only; task side effects in source files, databases, or external services are outside recovery.

The initialization wizard chooses a profile, recovery setup, and a new or existing device identity. After creating the profile it saves a local preference and asks separately whether to trust the project. A fully explicit `env init` keeps the existing behavior: use `--auto` for a shared default or `env use` for a local one. No new project or lock schema is introduced.

### `env`

`pinset env` shows the selected profile, its source, declared profiles, and trust status without decrypting values. Use root `-C` to inspect another project. It does not change state.

### `env use`

`pinset env use <profile> [--cwd <path>]` remembers a declared profile in `PINSET_HOME/state/environments/`. The record is bound to the canonical project path and `project-id`, so worktrees are independent. It never changes shared project configuration, ciphertext, or trust. Invalid, removed, or stale selections fail with a reset instruction. Newer selection via `-e` or `PINSET_ENV_PROFILE` takes precedence.

### `env reset`

`pinset env reset [--cwd <path>]` clears only the local preference. `pinset env use --reset` is equivalent. Process and shared defaults still apply afterward.

### `env share`

`pinset env share <age-recipient> [--profile <name>] [--cwd <path>]` adds a public recipient to the selected profile, using the existing re-encryption operation. A matching private identity is required; changed recipient policy invalidates existing project trust. The original `env recipient add` remains available.

### `env unshare`

`pinset env unshare <age-recipient> [--profile <name>] [--cwd <path>]` removes a recipient using the existing re-encryption operation. It cannot remove the last recipient. This cannot revoke plaintext or ciphertext already copied by that recipient. The original `env recipient remove` remains available.

### `env members`

`pinset env members [--profile <name>] [--cwd <path>]` lists the selected profile's public recipients without decrypting values or changing state. The original `env recipient list` remains available.

## Encrypted project environment commands

Pinset manages project-scoped string environment variables in independent [age](https://age-encryption.org/) ciphertext profiles. Public recipients and ciphertext files belong in the repository; private identities and recovery passphrases do not. Pinset does not automatically read `.env`, write temporary plaintext files, interpolate values, or provide a general-purpose Secrets Vault.

The normal first-computer workflow is:

```sh
# Existing schema 1–4 projects only; a new `pinset init` project is already schema 5.
pinset migrate

# Creates pinset.env/development.age, a device identity, and an encrypted recovery identity.
pinset env init --profile development --auto \
  --recovery ~/pinset-development-recovery.age
pinset env set DATABASE_URL --profile development
pinset env list --profile development
pinset trust add

# The direct shim selects the runtime and injects the trusted auto-profile.
node app.js
```

Commit `pinset.toml` and `pinset.env/*.age`. Keep recovery and identity files outside the repository. Back up the recovery file and its passphrase separately; losing every matching identity makes the ciphertext unrecoverable.

Profile names are 1–64 ASCII letters, digits, dots, underscores, or hyphens. A configured ciphertext path must remain inside the canonical project boundary and resolve to a regular non-symlink file. Decrypted profile data is limited to 1 MiB, and Pinset also refuses to launch when the inherited plus injected environment would exceed the platform environment-block limit.

### Profile selection and injection

Commands with an optional `--profile` select a profile in this order: explicit `--profile` (or root `-e`), `PINSET_ENV_PROFILE`, machine-local `env use`, then `[environment].auto-profile`. CI ignores machine-local preferences (`CI`, `GITHUB_ACTIONS`, `GITLAB_CI`, or `TF_BUILD` set to a nonempty value other than `0` or `false`). If none is available, Pinset asks for `--profile`. Commands whose syntax requires `--profile` do not use this fallback.

Direct Provider commands such as `node`, `python`, `cargo`, and `flutter` inject only when the project is trusted and `PINSET_ENV_PROFILE`, a machine-local preference, or `auto-profile` selects a profile. `pinset exec --profile <name> -- <command>` makes the selection explicit. `PINSET_ENV_DISABLE=1` and `pinset exec --no-env -- <command>` disable injection for one launch.

The `[environment].collision` policy is case-insensitive and defaults to `error`:

- `error`: stop before launch when an encrypted name already exists in the process or runtime environment.
- `process-wins`: keep the existing value and discard the encrypted value for that name.
- `encrypted-wins`: replace the existing value with the encrypted value.

`PINSET_IDENTITY`, `PINSET_IDENTITY_FILE`, `PINSET_ENV_PROFILE`, and `PINSET_ENV_DISABLE` are removed before the business process starts. This prevents accidental forwarding, but it is not process isolation: a process that receives a secret may pass it to other programs.

### `env init`

| Field | Description |
| --- | --- |
| Purpose | Create one empty encrypted profile, a device age X25519 identity, and normally a separate recovery identity. |
| Syntax and arguments | `pinset env init [<name> \| --profile <name>] [--auto] [--recovery <path> \| --no-recovery] [--identity-file <path> \| --identity <id>] [--cwd <path>]`. Missing profile or recovery choice opens an interactive wizard. Noninteractive setup must supply both choices; `--identity` reuses a stored device identity. `--identity-file` stores the device identity in a passphrase-protected file instead of the system keyring. |
| Modifies state | **Yes.** Creates `pinset.env/<profile>.age`, updates a schema 4 or 5 `pinset.toml`, stores the device identity, and may create a recovery file. `--auto` sets this profile as `auto-profile`. |
| Example | `pinset env init --profile ci --recovery ~/pinset-ci-recovery.age` |
| JSON | No. |
| Key errors | Project is older than schema 4, profile/file already exists, invalid profile name, unavailable keyring, unsafe path, existing recovery output, or encryption/write failure. A final configuration-write failure removes the new ciphertext; an identity or recovery file created earlier in the operation may remain and should be reviewed. |

Use `--no-recovery` only when another tested identity-backup procedure exists. On Linux/SSH systems without a usable keyring, use `--identity-file <path>` and set `PINSET_IDENTITY_FILE` to that protected file for later interactive commands.

### `env set`

| Field | Description |
| --- | --- |
| Purpose | Add or replace one encrypted variable in a profile. |
| Syntax and arguments | `pinset env set <name> [--profile <name>] [--stdin] [--cwd <path>]`. Without `--stdin`, Pinset reads the value with hidden terminal input; the value is never a positional argument. |
| Modifies state | **Yes.** Decrypts, modifies, re-encrypts with fresh age file-key material, and atomically replaces the selected ciphertext under a file lock. |
| Example | `pinset env set DATABASE_URL --profile development` |
| JSON | No. |
| Key errors | No selected/matching identity, invalid variable name, process input failure, malformed/oversized profile, unsafe profile path, or encryption/write failure. |

Portable names match `[A-Za-z_][A-Za-z0-9_]*`. Names are unique ignoring ASCII case; `PATH` and every `PINSET_*` name are reserved. Values may be empty or multiline but may not contain NUL. `--stdin` reads the entire standard input and removes one trailing line ending; ensure the producer does not expose the value in its own arguments, logs, or files.

### `env unset`

| Field | Description |
| --- | --- |
| Purpose | Remove one variable, matching its name case-insensitively. |
| Syntax and arguments | `pinset env unset <name> [--profile <name>] [--cwd <path>]`. |
| Modifies state | **Yes.** Atomically re-encrypts the profile even when reporting that the name was not set. |
| Example | `pinset env unset LEGACY_TOKEN --profile development` |
| JSON | No. |
| Key errors | Invalid name, missing profile or identity, unsafe/damaged ciphertext, or write failure. |

### `env list`

| Field | Description |
| --- | --- |
| Purpose | List variable names in one profile without writing values to output. |
| Syntax and arguments | `pinset env list [--profile <name>] [--json] [--cwd <path>]`. |
| Modifies state | No. The profile is decrypted in memory only. |
| Example | `pinset env list --profile ci --json` |
| JSON | **Yes**; command name `env.list`, with `profile` and `names`. Values are never included. |
| Key errors | No selected profile, missing matching identity, unsafe/damaged ciphertext, or unsupported profile schema. |

### Environment variable contracts

Schema 5 can declare expected structure without storing secret values in `pinset.toml`:

```toml
[environment.variables.DATABASE_URL]
type = "url"
required = true
secret = true
profiles = ["development", "test"]

[environment.variables.PORT]
type = "integer"
default = "3000"

[environment.variables.MODE]
type = "enum"
required = true
default = "development"
values = ["development", "production"]
```

Types are `string`, `integer`, `boolean`, `url`, and `enum`. Boolean values are exactly `true` or `false`; URLs require a host; enums require at least one allowed value. Secret contracts cannot declare defaults. Contracts are checked before injection, and applicable non-secret defaults are added only when the encrypted profile does not contain that name.

### `env check`

`pinset env check [--profile <name>] [--json] [--cwd <path>]` decrypts one profile in memory and reports missing required variables or invalid declared types. It exits `0` when ready, `1` after a complete check with contract issues, and `2` when checking cannot start. JSON uses command `env.check`, returns `data.ok` and issue names/reasons, and never includes values or value hashes.

### `env diff`

`pinset env diff <left> <right> [--json] [--cwd <path>]` compares case-insensitive variable names and applicable contract names. It reports only names present on one side. JSON uses command `env.diff`; values and value-derived hashes are never emitted.

### `env reveal`

| Field | Description |
| --- | --- |
| Purpose | Print exactly one decrypted value for deliberate human inspection. |
| Syntax and arguments | `pinset env reveal <name> --profile <name> [--cwd <path>]`. Both standard input and output must be interactive terminals. |
| Modifies state | No. |
| Example | `pinset env reveal DATABASE_URL --profile development` |
| JSON | No. |
| Key errors | Redirected/non-interactive terminal, unset name, no matching identity, or invalid ciphertext. |

The value is written to the terminal and may remain visible in scrollback. Prefer `env list` for routine inspection.

### `env import`

| Field | Description |
| --- | --- |
| Purpose | Import an explicitly named plaintext dotenv file into one encrypted profile. |
| Syntax and arguments | `pinset env import --from <path> --profile <name> [--cwd <path>]`. |
| Modifies state | **Yes.** Matching names replace profile values; other existing names remain. The source file is not modified or deleted. |
| Example | `pinset env import --from .env --profile development` |
| JSON | No. |
| Key errors | Invalid UTF-8/assignment/name, case-insensitive duplicate, `export`, interpolation, command substitution, shell expression, unsupported escape, unmatched quote, missing identity, or encryption failure. |

The portable subset accepts blank lines, `#` comments, empty values, unquoted values, single or double quotes, quoted multiline values, and double-quoted `\n`, `\r`, `\t`, `\\`, and `\"` escapes. It never executes input. After verifying the import, remove or protect the plaintext source yourself.

### `env export`

| Field | Description |
| --- | --- |
| Purpose | Deliberately export one profile as a plaintext dotenv file for a system that cannot consume Pinset injection. |
| Syntax and arguments | `pinset env export --profile <name> --format dotenv --output <path> --allow-plaintext [--cwd <path>]`. `dotenv` is the only format. |
| Modifies state | **Yes.** Creates a new plaintext file; it never overwrites an existing path. |
| Example | `pinset env export --profile development --format dotenv --output ./local.env --allow-plaintext` |
| JSON | No. |
| Key errors | Missing consent flag, existing/unsafe output, permission-hardening failure, missing identity, or invalid ciphertext. |

The output is restricted to the current user (`0600` on Unix and a user-only ACL on Windows). It still contains plaintext; do not commit it, and delete it as soon as its explicit use is complete.

### `env recipient add`

| Field | Description |
| --- | --- |
| Purpose | Allow another age X25519 identity to decrypt one profile. |
| Syntax and arguments | `pinset env recipient add <age1...> --profile <name> [--cwd <path>]`. |
| Modifies state | **Yes.** Decrypts with a current identity, re-encrypts atomically to the new deduplicated recipient set, then updates `pinset.toml`. |
| Example | `pinset env recipient add age1example... --profile production` |
| JSON | No. |
| Key errors | Invalid recipient, no current matching identity, undeclared profile, unsafe/damaged ciphertext, or transactional write failure. |

### `env recipient remove`

| Field | Description |
| --- | --- |
| Purpose | Remove one recipient from a profile after proving current decryption access. |
| Syntax and arguments | `pinset env recipient remove <age1...> --profile <name> [--cwd <path>]`. |
| Modifies state | **Yes.** Re-encrypts and updates configuration transactionally; the original ciphertext is restored if the configuration update fails. |
| Example | `pinset env recipient remove age1example... --profile production` |
| JSON | No. |
| Key errors | Invalid recipient, attempt to remove the final recipient, no matching identity, or re-encryption/configuration failure. |

Adding or removing a recipient changes the trusted environment policy. Commit both `pinset.toml` and the new ciphertext, then run `pinset trust add` again on each machine and in CI. Removing a recipient prevents future decryption with that identity but cannot revoke plaintext already obtained earlier.

### `env recipient list`

| Field | Description |
| --- | --- |
| Purpose | Print the public recipients configured for one profile. |
| Syntax and arguments | `pinset env recipient list --profile <name> [--cwd <path>]`. |
| Modifies state | No. It reads committed configuration and does not decrypt the profile. |
| Example | `pinset env recipient list --profile production` |
| JSON | No. |
| Key errors | Missing project/configuration or undeclared profile. |

### `env identity create`

| Field | Description |
| --- | --- |
| Purpose | Generate an additional age X25519 identity and print its ID plus public recipient. |
| Syntax and arguments | `pinset env identity create [--output <path>]`. Without `--output`, the private identity is stored in the system keyring; with it, a new passphrase-protected identity file is created. |
| Modifies state | **Yes.** Writes the keyring and local identity metadata, or creates the protected output file. |
| Example | `pinset env identity create` |
| JSON | No. The printed `age1...` recipient is public; the private identity is never printed. |
| Key errors | Keyring unavailable, output exists, passphrase confirmation mismatch, permission failure, or cryptographic failure. |

Use the printed recipient with `env recipient add`. Creating an identity alone does not grant it access to an existing profile.

### `env identity import`

| Field | Description |
| --- | --- |
| Purpose | Restore a passphrase-protected recovery/backup identity into this machine's system keyring. |
| Syntax and arguments | `pinset env identity import --from <path>`. The passphrase is read with hidden input. |
| Modifies state | **Yes.** Adds the decrypted identity to the keyring and local identity metadata; the source backup remains unchanged. |
| Example | `pinset env identity import --from ~/pinset-development-recovery.age` |
| JSON | No. |
| Key errors | Wrong passphrase, damaged/non-identity input, unavailable keyring, or metadata write failure. |

After cloning on a new computer, run `pinset install --locked`, import a matching recovery identity, run `pinset trust add`, and then use direct shims normally.

### `env identity list`

| Field | Description |
| --- | --- |
| Purpose | List registered identity IDs, public recipients, and storage backends without private keys. |
| Syntax and arguments | `pinset env identity list [--json]`. |
| Modifies state | No. |
| Example | `pinset env identity list --json` |
| JSON | **Yes**; command name `env.identity.list`. |
| Key errors | Invalid or unreadable local identity metadata. |

### `env identity backup`

| Field | Description |
| --- | --- |
| Purpose | Back up one keyring identity to a new passphrase-protected age file. |
| Syntax and arguments | `pinset env identity backup <id> --output <path>`. A new backup passphrase is requested and confirmed. |
| Modifies state | **Yes.** Creates the protected output without overwriting an existing file. |
| Example | `pinset env identity backup 4c5652e4-... --output ~/pinset-device-backup.age` |
| JSON | No. |
| Key errors | Unknown ID, keyring access failure, output exists, passphrase mismatch, or encryption/write failure. |

### `env identity export`

| Field | Description |
| --- | --- |
| Purpose | Export a keyring identity as plaintext, primarily for an explicitly protected CI secret. |
| Syntax and arguments | `pinset env identity export <id> --output <path> --allow-plaintext`. |
| Modifies state | **Yes.** Creates a new current-user-only plaintext file and never overwrites. |
| Example | `pinset env identity export 4c5652e4-... --output ./ci-identity.txt --allow-plaintext` |
| JSON | No. |
| Key errors | Missing consent flag, unknown ID, keyring failure, existing output, or permission-hardening failure. |

Copy the file contents into the CI secret, then securely remove the file. Never commit it or pass the private identity as a command-line argument.

### `trust add`

| Field | Description |
| --- | --- |
| Purpose | Approve automatic environment injection for the current canonical project and its exact environment policy. |
| Syntax and arguments | `pinset trust add [--project-id <id>] [--cwd <path>]`. The optional ID makes automation fail if the checked-out project has a different `project-id`. |
| Modifies state | **Yes.** Writes a local trust record under `PINSET_HOME/state/trust`; nothing is added to the repository. |
| Example | `pinset trust add --project-id 4c5652e4-0000-4000-8000-000000000000` |
| JSON | No. |
| Key errors | No environment configuration/project ID, expected ID mismatch, unsafe project root, or trust-store write failure. |

Trust binds the canonical root, `project-id`, `auto-profile`, collision policy, every profile path, and every recipient. Changing any of these requires renewed trust; changing only encrypted variable values does not. Trust does not make project code safe.

### `trust status`

| Field | Description |
| --- | --- |
| Purpose | Check whether the current project and environment policy match a local trust record. |
| Syntax and arguments | `pinset trust status [--cwd <path>] [--json]`. |
| Modifies state | No. |
| Example | `pinset trust status --json` |
| JSON | **Yes**; command name `trust.status`, with `trusted`, reason `trusted`/`trust_missing`/`trust_changed`, and canonical `root`. |
| Exit | `0` when the status check completes, including untrusted states; use the JSON `trusted` field for automation. |
| Key errors | Missing/invalid project environment configuration or unreadable trust storage. |

### `trust revoke`

| Field | Description |
| --- | --- |
| Purpose | Remove this machine's automatic-injection approval for one project. |
| Syntax and arguments | `pinset trust revoke [--cwd <path>]`. |
| Modifies state | **Yes.** Deletes only the local trust record; ciphertext, configuration, identities, and already disclosed values remain unchanged. |
| Example | `pinset trust revoke` |
| JSON | No. |
| Key errors | Project discovery or trust-store access failure. Revoking an already-untrusted project is successful and reports that no record existed. |

### GitHub Actions identity flow

Create a dedicated identity, add its public recipient only to the `ci` profile, and store the private identity text as the GitHub Actions secret `PINSET_IDENTITY`. The secret may contain multiple identities separated by newlines. Select the profile explicitly and pin the expected public project ID:

```yaml
jobs:
  test:
    runs-on: ubuntu-latest
    env:
      PINSET_IDENTITY: ${{ secrets.PINSET_IDENTITY }}
      PINSET_ENV_PROFILE: ci
    steps:
      - uses: actions/checkout@v4
      - uses: Future-Element/pinset@v2.3.0
        with:
          version: 2.3.0
          install: "true"
          trust-project-id: "4c5652e4-0000-4000-8000-000000000000"
      - run: pinset exec -- node app.js
```

The Action input is not a secret and does not persist the identity. Pinset removes `PINSET_IDENTITY` before launching the child. Do not inject server secrets into Flutter, Web, mobile, or other builds that compile values into client artifacts.

## Pinset 2.0 maintenance commands

`pinset paths [tool] [--json]` reports the CLI, adjacent shim, Pinset home, shim directory, installation root, and an optional tool's installed versions. `pinset list [tool] --long` adds receipt schema, installation root, file count, total size, critical entries, and integrity status. `pinset doctor --deep` rescans these statistics; it does not claim per-file cryptographic verification. `pinset install <tool@exact-version> --repair` repairs only an installation whose ownership receipt matches the requested tool, version, platform, and target directory. `pinset shim install --all` registers every built-in Provider command without downloading a runtime.

`pinset self outdated [--channel stable|prerelease] [--json]` performs an explicit check against the fixed official repository. Before downloading an update, `pinset self update [--version <version>]` checks the global `global.lock` and automatically migrates safely recognized pre-1.0 Provider records at their existing exact versions. Compatibility migration covers the old Linux ARM64 target gaps in Node.js, pnpm, Bun, Go, Python, Java, Rust, and .NET SDK, and upgrades the historical Node.js HTTPS-checksum record to the current OpenPGP-authenticated record. Flutter's target matrix did not change. It then verifies platform, semantic version, archive structure, and `SHA256SUMS`, validates the new CLI, and replaces the CLI and shim as a pair with backup and rollback. If automatic migration cannot be completed, repair it explicitly with `pinset migrate --global`. Ordinary commands and `doctor` never check for updates in the background.

Self updates use a cross-process lock and a 60-second HTTP timeout. Windows replacement remains asynchronous because a running executable cannot replace itself; the helper records success or rollback under `PINSET_HOME/state`, and the next `self outdated` or `self update` reports that result. Windows `.cmd`/`.bat` runtime fallbacks reject arguments containing `cmd.exe` metacharacters rather than risk shell reinterpretation; managed `.exe` runtimes are unaffected.

## Stable protocol boundary

The current development line writes schema 5 project configuration and schema 4 global configuration/runtime locks. Schema 1–4 projects and schema 1–3 locks remain readable and are migrated explicitly. Existing schema 4 encrypted environments continue to operate before migration. Installation receipts use independent schema 4 while schema 1–3 receipts remain readable. Project `[policy]` accepts optional `verification-strength = "checksum" | "signed-checksum" | "provenance"` and `minimum-release-age = "<positive integer><d|h|m|s>"`. New locks may record the optional upstream `released-at` timestamp. A configured policy is enforced during state writes, project installation, updates including dry runs, and lock audits; unavailable release time fails closed, and replacing an existing tool lock with weaker verification is rejected.

The JSON schema 1 envelope remains unchanged in v2.0. New JSON commands include `paths`, `env.list`, `env.identity.list`, `trust.status`, and `self.outdated`. Automation should branch on stable command and reason/code fields, not human-facing messages. JSON output and errors never include environment values, identities, or passphrases.
