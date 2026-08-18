# Pinset command reference

[English](commands.md) | [简体中文](commands.zh-CN.md) · [README](../README.md)

This document describes the Pinset v1.1 command-line contract. Run `pinset <command> --help` for the exact parser help shipped with your binary.

## Conventions

### Selections and scope

A selection has the form `<tool>@<selector>`, for example `node@22`, `pnpm@latest`, `java@lts`, or `rust@stable`. Pinset resolves selectors to exact versions before writing a lockfile.

Supported tools are Node.js, pnpm, Bun, Go, Python, Java, Rust, .NET, and Flutter. Dart is provided by the selected Flutter SDK. A nearest project `pinset.toml` takes precedence over global state; Pinset can then fall back to an eligible system command.

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
- `2`: Pinset usage, configuration, metadata, integrity, or installation failure.
- `pinset exec`: returns the exact child-process exit code after a successful launch; Pinset failures before launch return `2`.

The tables below repeat exceptional behavior where it matters. Otherwise the command follows these exit codes.

## Project and selection commands

### `init`

| Field | Description |
| --- | --- |
| Purpose | Create a minimal project configuration in the current directory. |
| Syntax and arguments | `pinset init`; no command-specific options. |
| Modifies state | **Yes.** Creates `pinset.toml`; it does not select or install a runtime. |
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
| Purpose | Re-scan and import every safe traditional selection into schema-2 `pinset.toml` and `pinset.lock`. |
| Syntax and arguments | `pinset import [--cwd <path>] [--force] [--no-install]`. `--force` replaces only discovered tools already selected at another exact version. |
| Modifies state | **Yes.** Resolves metadata, writes lock then config atomically, and installs all project selections by default. `--no-install` skips runtime archives and Python `.venv`, but still resolves and locks metadata. |
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
| Syntax and arguments | `pinset which <command> [--cwd <path>] [--json]`. |
| Modifies state | No. |
| Example | `pinset which node --json` |
| JSON | **Yes**; command name `which`. |
| Exit | `0` when resolved; `2` when no usable command can be resolved. |
| Key errors | Unknown managed command, missing selected runtime, invalid lock, or no eligible system fallback. |

### `current`

| Field | Description |
| --- | --- |
| Purpose | Show the effective project, global, or system runtime selection and executable. |
| Syntax and arguments | `pinset current [tool] [--cwd <path>] [--json]`; the default tool is Node.js. |
| Modifies state | No. |
| Example | `pinset current python --cwd ./app` |
| JSON | **Yes**; command name `current`. |
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
| Purpose | Compare selected project and global runtimes with current stable releases. |
| Syntax and arguments | `pinset outdated [tool] [--global | --cwd <path>] [--json]`. |
| Modifies state | No. |
| Example | `pinset outdated --cwd ./app --json` |
| JSON | **Yes**; command name `outdated`, with results under `data.runtimes`. |
| Exit | `0` after a complete comparison; `2` if scope, lock, or metadata validation fails. |
| Key errors | Missing project, unsupported tool, invalid lock, or Provider metadata failure. |

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

### `doctor`

| Field | Description |
| --- | --- |
| Purpose | Diagnose project, lockfile, installation, command-routing, environment, and PATH state. |
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

## Stable protocol boundary

Pinset v1.0 writes schema 2 for project/global configuration, lockfiles, global state, and installation receipts. Compatible readers may add fields within a major version. Removing a field or changing its type or meaning requires a new major version; a future disk-format change must remain readable or provide an explicit migration. A pre-v1 Node lock that records only an HTTPS checksum is rejected with instructions to run `pinset use` again, because it lacks the v1 OpenPGP verification evidence.
