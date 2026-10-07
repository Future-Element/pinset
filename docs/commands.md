# Pinset 3.0 command reference

Pinset manages and locks project toolchains. Configuration is `.pinset/config.toml`; the exact artifact lock is `.pinset/lock.toml`. Eight built-in Providers: Node, pnpm, Bun, Go, Python, complete Temurin OpenJDK, Rust, Flutter/Dart. npm comes with Node, pip with the standard-library venv, and Dart with Flutter. There is no task runner or workspace orchestration.

Global options: `-C/--cwd`, `--lang auto|en|zh-CN`, `--json`, `--help`, `--version`. Preview uses `--plan`. `exec` rejects `--json` and preserves native output and exit codes.

## Toolchains

### `init`

Create a new project identity and configuration in the specified directory. No native version files are imported.

```text
pinset init
```

### `use`

Select, upgrade or switch tool versions by resolving official artifacts. Use an earlier exact version to switch back. Duplicate tools are rejected. Config and lock update together through a journaled transaction; `--no-install` commits only their selection and lock.

pnpm requires an explicit Node selection. Pinset supports the pnpm 10 CJS, 11 ESM and 12 native packages; native binaries and shared runtime files are locked and verified during installation. Conflicting project package-manager or runtime versions fail explicitly rather than downloading or switching toolchains automatically.

```text
pinset use <tool@selector>... [--global] [--no-install] [--plan]
pinset use node@24 pnpm@10 python@3.14 java@21
```

### `remove`

Remove selections and lock entries. Installed SDKs and the project venv are retained; installation cleanup belongs to `clean`.

```text
pinset remove <tool>... [--global] [--plan]
```

### `install`

Install strictly from the existing exact lock. This command never resolves newer versions. Offline cache hits still undergo digest verification. Repair cannot adopt external installations. Recreating `.venv` requires valid Pinset 3 ownership in this project directory.

Interactive `use` and `install` show resolution and installation stages on stderr. Downloads with a known size show percentage, bytes, speed and estimated time remaining; unknown sizes show bytes and speed. Verified cache hits and existing installations are reported explicitly. JSON and redirected stderr omit the progress UI.

```text
pinset install [tool...] [--global] [--offline] [--repair]
               [--recreate-venv] [--plan]
```

### `list`

Inspect selections and installed artifacts, or query official remote versions.

```text
pinset list [tool]
pinset list <tool> --remote
```

### `which`

Show selected command entries, precise versions, installation identities and SDK/JDK roots. Unsupported JDK commands fail instead of falling back to a system JDK.

```text
pinset which [command] [--global] [--explain]
pinset which javac --explain
```

### `check`

Default inspection is read-only: it does not decrypt profiles, start tools, use the network or write reports. `--deep` inspects installation payloads. `--probe` explicitly starts bounded entry probes and records host, path, version and observation time. Configured bindings and observed probe results are reported separately; project builds remain your own commands. Java checks work without Flutter; build targets require project context.

```text
pinset check [--global] [--deep] [--probe]
             [--target android|ios|windows|macos|linux|web]
```

### `exec`

Run an original command in the exact selected environment. It does not install or download tools. Pinset is not an execution sandbox; native tool networking and explicit external build toolchains remain possible and must be reported separately.

```text
pinset exec [--profile <profile>|--no-env] -- <command...>
pinset exec -- ./mvnw verify
pinset exec -- ./gradlew build
```

## Encrypted environment

### `env init`

Create a profile under `.pinset/env/` using per-value age encryption. Private identities live only in the system credential store or explicit `PINSET_IDENTITY`.

```text
pinset env init <profile>
```

### `env use`

Default selection is local. `--project` changes the shared default; reset removes that selection. Selection priority: explicit argument, process environment, local default, shared default. Explicit disable wins.

```text
pinset env use <profile> [--project]
pinset env use --reset [--project]
```

### `env remove`

Remove the profile and its selection; preview uses `--plan`.

```text
pinset env remove <profile> [--plan]
```

### `env list`

List profiles or variable names, never decrypted values.

```text
pinset env list [--profile <profile>]
```

### `env set`

Input uses a hidden prompt or stdin. Conflicts with process variables fail; toolchain and Pinset control variables cannot be encrypted overrides.

```text
pinset env set <name> --profile <profile> [--stdin]
```

### `env unset`

```text
pinset env unset <name> --profile <profile>
```

### `env access request`

Requests contain public metadata only. Normal identities use the credential store. `--ci` requires a TTY and displays a private identity once for manual transfer into a platform Secret; it never writes that identity to a file.

```text
pinset env access request [--new]
pinset env access request --ci
```

### `env access grant`

Re-encrypt the profile to grant access to the request recipient.

```text
pinset env access grant <request-file> --profile <profile>
```

### `env access revoke`

Re-encrypt the profile after revoking the recipient.

```text
pinset env access revoke <request-id> --profile <profile>
```

### `env access list`

```text
pinset env access list --profile <profile>
```

### `env trust add`

Trust binds project ID, directory identity and config/profile fingerprint. External changes invalidate it.

```text
pinset env trust add
```

### `env trust status`

```text
pinset env trust status
```

### `env trust revoke`

```text
pinset env trust revoke
```

## Maintenance

### `clean cache`

Cleanup protects registered projects, global selections, venvs and recovery journals. Unknown or uncertain objects are retained.

```text
pinset clean cache [--plan]
```

### `clean installs`

```text
pinset clean installs [tool@exact...] [--plan]
```

### `self info`

```text
pinset self info
```

### `self shell`

Output a shell integration fragment. User profiles are never modified.

```text
pinset self shell <bash|zsh|fish|powershell>
```

### `self completions`

```text
pinset self completions <bash|zsh|fish|powershell>
```

### `self repair`

Recover interrupted transactions for the current project and global selections, restore an interrupted CLI/router update, and repair Pinset-owned command entries. Prepared transactions restore prior configuration, lock, encrypted profiles and local bindings; completed transactions only clear their pending marker. This does not undo a completed version switch. `--plan` previews recovery without writing state.

```text
pinset self repair [--plan]
```

### `self update`

Update from an official Pinset 3 release after checking its platform checksum.

The target release metadata selects the actual platform ZIP and `SHA256SUMS`; asset names may be stable or contain that release's version. The checksum must name the selected ZIP exactly once. Missing or incomplete assets, mismatched release versions and invalid checksums abort the update. `--plan` checks release metadata and checksums without downloading binaries or writing local state.

```text
pinset self update [version] [--plan]
```

Selectors use `latest`, Java/Node `lts`, numeric versions and exact builds. Rust additionally uses `stable` and dated `nightly-YYYY-MM-DD`; it never chooses an unpinned nightly. The old `current` selector is rejected.
