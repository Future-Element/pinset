# Pinset command reference

[English](commands.md) | [简体中文](commands.zh-CN.md) · [README](../README.md)

This document describes the Pinset v1.9 command-line contract. Run `pinset <command> --help` for the exact parser help shipped with your binary.

## Conventions

### Selections and scope

A selection has the form `<tool>@<selector>`, for example `node@22`, `pnpm@latest`, `java@lts`, or `rust@stable`. Schema 3 keeps that requested selector in configuration and records its exact resolved version in the lockfile.

Supported tools are Node.js, pnpm, Bun, Go, Python, Java, Rust, .NET, and Flutter. Dart is provided by the selected Flutter SDK. Project discovery stops at the nearest Git root by default; without a Git marker it inspects only the start directory. A project is strict by default: an undeclared tool neither inherits global state nor falls back to the system command unless `[policy]` explicitly enables `inherit-global` or `system-fallback`. Outside a project, global state then system `PATH` remain eligible.

The global `--lang <en|zh-CN>` option selects output language for one invocation. Running `pinset --lang <language>` without a subcommand saves the default language.

### State

- Project selection: `pinset.toml` and `pinset.lock`.
- Global selection: `PINSET_HOME/state/global.toml` and `global.lock`.
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
| Modifies state | **Yes.** Creates schema 3 `pinset.toml` with strict project policy; it does not select or install a runtime. |
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
| Purpose | Re-scan and import every safe traditional selection into schema 3 `pinset.toml` and `pinset.lock`. |
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
| Purpose | Show global selections or set one global default. |
| Syntax and arguments | `pinset global [<tool>@<selector>] [--no-install]`. `--no-install` requires a selection. |
| Modifies state | Without a selection: **No**. With a selection: **Yes**, writes global config and lock; installs unless `--no-install` is used. |
| Example | `pinset global node@lts` |
| JSON | No. |
| Exit | `0` success; `2` on Pinset failure. |
| Key errors | Unsupported Provider, invalid selector, unavailable metadata, untrusted manifest, download/integrity failure, or unsupported target. |

### `use`

| Field | Description |
| --- | --- |
| Purpose | Resolve and lock one runtime for the nearest project, or for global scope. |
| Syntax and arguments | `pinset use <tool>@<selector> [--no-install] [--global]`. |
| Modifies state | **Yes.** Writes the selected scope's config and lock; installs unless `--no-install` is used. |
| Example | `pinset use pnpm@10` |
| JSON | No. |
| Exit | `0` success; `2` on Pinset failure. |
| Key errors | Missing project config, invalid selector, metadata/signature failure, unsupported platform, or installation failure. |

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
| Syntax and arguments | `pinset install [<tool>@<exact-version>] [--locked] [--global | --cwd <path>]`. An explicit selection conflicts with lock-scope options; locked installation is the default project behavior. |
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
| Purpose | Validate and rewrite existing schema 1/2 project or global config and lock state as schema 3 without re-resolving versions. |
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
| Key errors | Non-exact version, selected runtime still referenced, missing/invalid receipt, unsafe path, or non-owned installation. `--force` bypasses selection references, not ownership checks. |

### `prune`

| Field | Description |
| --- | --- |
| Purpose | Remove installed versions not protected by global or supplied project selections. |
| Syntax and arguments | `pinset prune [--cwd <path>] [--project <path>]... [--dry-run] [--json]`. |
| Modifies state | **Yes**, unless `--dry-run`. |
| Example | `pinset prune --project ./app --project ../service --dry-run` |
| JSON | **Yes**; command name `prune`. |
| Exit | `0` for a completed plan/removal; `2` if references or ownership cannot be validated. |
| Key errors | Invalid project lock, unsafe installation path, missing receipt, or filesystem failure. |

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

## Download cache commands

The cache stores verified archives by integrity identity. Cache inspection never treats a filename alone as proof of integrity.

### `cache`

| Field | Description |
| --- | --- |
| Purpose | Group download-cache inspection, verification, repair, cleanup, and offline import operations. |
| Syntax and arguments | `pinset cache <list|info|verify|repair|clean|import> ...`; a subcommand is required. |
| Modifies state | Depends on the subcommand: `repair`, `clean`, and `import` modify cache state. |
| Example | `pinset cache info` |
| JSON | No group-level output; `list`, `info`, `verify`, `repair`, and `clean` support `--json`. |
| Exit | `0` for successful subcommand completion; `2` for missing/invalid subcommand or cache failure. |
| Key errors | Missing subcommand, unsafe cache path, corruption, invalid integrity, or filesystem failure. |

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

Pinset owns a project `.venv` only when its ownership marker matches the current project and selected CPython distribution. Destructive operations fail closed if ownership cannot be proven.

### `venv`

| Field | Description |
| --- | --- |
| Purpose | Group project-owned Python environment operations. |
| Syntax and arguments | `pinset venv <create|status|recreate> ...`; a subcommand is required. |
| Modifies state | Depends on the subcommand; `create` and `recreate` modify state. |
| Example | `pinset venv status` |
| JSON | No. |
| Exit | `0` for successful subcommand completion; `2` for missing/invalid subcommand or environment failure. |
| Key errors | Missing subcommand, missing project Python selection, invalid ownership marker, or environment creation failure. |

### `venv create`

| Field | Description |
| --- | --- |
| Purpose | Install the selected CPython runtime if needed, then create or validate the project `.venv`. |
| Syntax and arguments | `pinset venv create [--cwd <path>]`. |
| Modifies state | **Yes.** May install Python and create `.venv` plus its ownership marker. |
| Example | `pinset venv create --cwd ./app` |
| JSON | No. |
| Exit | `0` when the environment is ready; `2` on Pinset failure. |
| Key errors | No project Python selection, lock mismatch, unsupported target, install failure, existing foreign `.venv`, or marker mismatch. |

### `venv status`

| Field | Description |
| --- | --- |
| Purpose | Show the selected CPython distribution and managed project-environment path. |
| Syntax and arguments | `pinset venv status [--cwd <path>]`. |
| Modifies state | No. |
| Example | `pinset venv status` |
| JSON | No. |
| Exit | `0` when status can be determined; `2` for invalid project or ownership state. |
| Key errors | Missing Python selection, invalid lock, missing/mismatched ownership marker, or unreadable environment. |

### `venv recreate`

| Field | Description |
| --- | --- |
| Purpose | Delete and recreate the project `.venv` after proving Pinset ownership. |
| Syntax and arguments | `pinset venv recreate [--cwd <path>]`. |
| Modifies state | **Yes.** Replaces only a correctly marked Pinset-owned `.venv`. |
| Example | `pinset venv recreate --cwd ./app` |
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

The v1.8 Registry is a read-only preview. A verified manifest describes commands, dependencies, shared capabilities, and provenance methods, but it cannot install, activate, or execute a Provider. Only Providers compiled into the current Pinset binary are active. Registry files must be bounded regular files containing exactly one valid cleartext OpenPGP signature from Pinset's pinned registry key.

### `provider list`

| Field | Description |
| --- | --- |
| Purpose | Verify and list the embedded declarative Provider Registry. |
| Syntax and arguments | `pinset provider list [--json]`. |
| Modifies state | No. It does not use the network, install runtimes, activate Providers, or execute manifest content. |
| Example | `pinset provider list --json` |
| JSON | **Yes**; command name `provider.list`, including the signed document and signer fingerprint. |
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

## Stable protocol boundary

Pinset v1.8 writes schema 3 for project/global configuration and lockfiles. Schema 1/2 remains readable. Schema 3 project `[policy]` accepts optional `verification-strength = "checksum" | "signed-checksum" | "provenance"` and `minimum-release-age = "<positive integer><d|h|m|s>"`. New locks may record the optional upstream `released-at` timestamp. A configured policy is enforced during state writes, project installation, updates including dry runs, and lock audits; unavailable release time fails closed, and replacing an existing tool lock with weaker verification is rejected. Installation receipts retain their independent schema.

The JSON schema 1 envelope remains unchanged in v1.8. `provider.list` and `provider.verify` add command-specific data without changing that envelope. `lock.audit` exposes provenance policy failures through stable `verification_below_policy`, `release_age_unavailable`, and `release_too_new` reason codes. Automation should branch on reason codes and report state, not human-facing messages.
