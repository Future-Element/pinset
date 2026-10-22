# Pinset command reference

Start with your task: new project, `init` → `use`; existing version files, `import` → `install`; cloned Pinset project, `install` → `check`.

Built-in tools: Node, pnpm, Bun, Go, Python, Temurin OpenJDK, Rust and Flutter/Dart. See [core concepts](concepts.md), [custom tools](generic-tools.md) and [CI](ci.md) for shared concepts and complete workflows.

Global options: `-C/--cwd <directory>`, `--lang auto|en|zh-CN`, `--json`, `--help`, `--version`. These are omitted below; use `<command> --help` for complete syntax. Success exits `0`, operational failure exits `1`, invalid arguments exit `2`; `exec` forwards child status. Example versions and paths are illustrative. “Result” describes effects, not literal terminal output.

## Tool management

### `init`

Create a Pinset project configuration in the current directory to start managing its tool versions.

#### When to use

Use for a new project without `.pinset/config.toml`. To keep existing `.nvmrc` or similar declarations, use `import` without running `init` first.

#### Examples

```sh
pinset init
```

Result: creates `.pinset/config.toml` with a new project ID. Next, run `pinset use node@24`.

#### Behavior

Initializes the specified directory without importing version files, network access or tool installation. Unselected tools can inherit global defaults; project selections win. `[policy] require_project_tools = true` requires project selections. Broken project state or an unavailable selected tool remains an error. See [core concepts](concepts.md).

#### Parameters, exit codes and common errors

No command-specific options. Success exits `0`. Existing configuration exits `1` with `PINSET_PROJECT_EXISTS`; use that project or choose another directory.

### `import`

Migrate existing project version declarations into Pinset after reviewing a plan of the changes.

#### When to use

Use with `.tool-versions`, `.nvmrc` or similar files. Existing Pinset choices are retained; only missing tools are added. Deliberately change choices with `use`.

#### Examples

For a project whose root `.nvmrc` declares Node 24:

```sh
pinset import --plan
pinset import
pinset install
```

The plan lists source declarations, exact versions, retained choices, conflicts and skipped tools. The second command writes configuration and lock after confirmation; the third installs tools.

#### Behavior

Checks `.tool-versions`, `package.json` (`engines`, `volta`, `packageManager`), `.nvmrc`, `.node-version`, `go.mod`, `rust-toolchain.toml` and `.python-version` at the project root. Existing projects use their discovered root; new ones use the specified directory. Original files remain.

Ranges and aliases resolve to marked exact versions through Provider metadata. Compatible declarations jointly constrain resolution; incompatible ones are skipped. `go.mod` prefers `toolchain` while checking the `go` minimum. Rust includes profile, components and targets, retaining pinned nightly dates, but cannot infer local paths or an unspecified default channel. npm, pip and standalone Dart have no separate Provider. pnpm needs a project Node selection. Declared generic tools require an explicit version or tag.

Existing choices and valid lock records remain. Missing locks are generated from existing selectors. Resolution can access the network; generic previews download temporary files to compute digests. `--plan` writes no project or Pinset state. Enter `y`, `yes` or “是” to apply; Enter cancels. Source, configuration or lock changes invalidate a reviewed plan.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `--plan` | Preview only. |
| `-y, --yes` | Display and confirm without prompting. |
| `--json` | Without `--yes`, return a plan only. When applying, plan goes to stderr and result to stdout. |

Success or cancellation exits `0`. No sources exits `1` with `PINSET_IMPORT_NO_SOURCES` and checked filenames. Invalid files use `PINSET_IMPORT_SOURCE_INVALID`; stale plans use `PINSET_IMPORT_PLAN_STALE` and need regeneration. Skipped entries need review but do not themselves fail the operation.

### `use`

Select or change project tool versions, update configuration and lock, and install the selected tools by default.

#### When to use

Use for first selection, upgrades or switching back. `use` decides versions; `install` restores versions already decided by the lock.

#### Examples

```sh
pinset use node@24 pnpm@10
pinset use python@3.14 --plan
```

Result: the first resolves exact versions, updates `.pinset/config.toml` and `.pinset/lock.toml`, installs and prepares command entries. The second previews only.

#### Behavior

Usually fetches official version/artifact metadata. Installation downloads or reuses verified cache. Configuration and lock updates are transactional. Installs and bindings write Pinset home and local project state; Python may prepare `.venv`. `--no-install` commits choices and lock only. Preview can access the network without writing state.

pnpm needs a Node selection. After editing selectors or Rust options, explicitly run `use tool@selector` to regenerate matching locks; installation and execution do not do this automatically. Do not repeat tools in one invocation. See [custom tools](generic-tools.md) for other CLIs.

Notes: pnpm 10 CJS, 11 ESM and 12 native releases are supported. Native packages and shared files are locked and verified. A mismatch between project declarations and the locked pnpm/runtime fails instead of switching toolchains.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `<tool@selector>...` | One or more choices, such as `node@24`. |
| `--global` | Change global defaults. |
| `--no-install` | Commit choices and lock without installation. |
| `--plan` | Resolve and preview only. |

Success exits `0`; failure exits `1`. `PINSET_SELECTOR_REQUIRED` means a missing selector; `PINSET_SELECTION_DUPLICATE`, repeated tools; `PINSET_PLATFORM_UNAVAILABLE`, no target artifact. Adjust choices rather than bypassing lock verification.

### `install`

Install tools from the project lock so a new computer or CI can restore the agreed toolchain.

#### When to use

Use after cloning, to add missing installations or repair damaged ones. Change versions with `use`.

#### Examples

```sh
pinset install
pinset install node pnpm --offline
```

Result: the first installs locked tools and bindings. The second restores only Node, pnpm and their locked dependencies from local cache; missing cache fails.

#### Behavior

Configuration and lock must be valid and match. Uses locked versions, URLs and digests, verifying downloads and cache without resolving versions. Writes installations, command entries and local bindings, including managed Python environments or Flutter links when required. Choices remain unchanged. Named installation handles only those tools and runtime dependencies, leaving unrelated environments/links alone.

`--repair` does not take over external installations. `--recreate-venv` requires a valid managed environment and a selected set including Python. Interactive stderr shows stages, progress and cache reuse for `use` and `install`; known sizes include percentage, speed and time remaining. JSON or redirected stderr hides progress UI.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `[tool...]` | Named tools and dependencies; omit for all project tools. |
| `--global` | Use the global lock. |
| `--offline` | Use valid installs or verified cache without downloading. |
| `--repair` | Repair managed installations. |
| `--recreate-venv` | Explicitly rebuild managed project Python. |
| `--plan` | Plan only, without downloads or writes. |

Success exits `0`; failure exits `1`. `PINSET_LOCK_MISSING` needs a maintainer-generated lock; `PINSET_LOCK_MISMATCH` needs choices aligned through `use`; `PINSET_TOOL_NOT_SELECTED` names an unselected tool. External `.venv` directories are preserved; inspect ownership first.

### `remove`

Remove a project tool choice while keeping downloaded tools and Python environments.

#### When to use

Use when a tool is no longer needed. Reclaim disk space with `clean installs`; other projects' references still protect installations.

#### Examples

```sh
pinset remove bun --plan
pinset remove bun
```

Result: previews, then removes Bun from configuration and lock while retaining its installation.

#### Behavior

Transactionally updates configuration and lock and registers project state. No network access or SDK/venv deletion. Global defaults may become available where inheritance is allowed; strict projects cannot use the removed tool.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `<tool>...` | Names to deselect. |
| `--global` | Remove global defaults. |
| `--plan` | Preview only. |

Success exits `0`; state and other operational errors exit `1`. Repeated tools use `PINSET_SELECTION_DUPLICATE`. Retain runtimes needed by other selected tools.

### `list`

View project choices and local installations, or look up official available versions.

#### When to use

Use to see choices and disk installations. Find an entry with `which`; inspect health with `check`.

#### Examples

```sh
pinset list
pinset list node --remote
```

Illustrative first output:

```text
Selected tools (exact lock)
  node  24.0.0  (24)
Installed
  node  24.0.0  linux-x86_64  /example/pinset/v3/installs/...
```

#### Behavior

Local listing is read-only, without tool execution or network access. `--remote` queries built-in Provider metadata, not SDK downloads. Generic tools cannot discover versions remotely. Install records do not prove a version probe passed.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `[tool]` | Filter one tool. |
| `--remote` | Query remote versions; requires a tool name. |

Success exits `0`; state or network errors exit `1`; a missing required tool argument exits `2`. Omit `--remote` to inspect local records during network failures.

### `which`

Find the program a command will run and, when requested, explain its version and selection source.

#### When to use

Use to diagnose project Node, Python, Java or other routes. This explains Pinset routing; use shell command lookup separately to inspect PATH.

#### Examples

```sh
pinset which node
pinset which javac --explain
```

The first prints only a path such as `/example/pinset/v3/installs/node/.../bin/node`. The second includes version, entry, SDK root, Provider and selection source.

#### Behavior

Read-only; no network, installation or tool execution. Omit the command for all managed routes. Requires valid locks and installations. Unavailable project selections do not fall back to system tools. Commands absent from a JDK do not fall back to the system JDK either.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `[command]` | Command, such as `node`, `python` or `javac`. |
| `--global` | Inspect global selections. |
| `--explain` | Include selection details. |

Success exits `0`; missing selections/installations or invalid routes exit `1`. Check `list`, then `install`. If bare commands still differ, inspect `self shell` integration.

### `check`

Check that tool configuration, installations and command routes agree, with optional payload and actual-version verification.

#### When to use

Use after installation, for diagnosis or CI reports. Default checks are read-only. `--deep` inspects payloads; `--probe` starts tools and records evidence.

#### Examples

```sh
pinset check
pinset check --deep --probe --format json > pinset-check.json
```

Illustrative plain-check output:

```text
Tool  Configured  Installed  Bound  Observed
node  yes  yes  yes  no
Check passed: yes
```

`Observed: no` means no probe ran, not a failed tool. The second writes overall and per-tool `passed` results.

#### Behavior

By default, does not decrypt, start tools, access the network or write files. `--deep` compares payload fingerprints with receipts. `--probe` runs bounded version probes and writes executable, version, host and timestamp evidence under Pinset home. Java/Android observations do not certify a project build.

JSON uses `schema_version: 1`; field names, types and meanings are stable, with additive changes only. Tools include `tool`, `expected_version`, `actual_version`, `artifact_digest`, `passed`. Without observed probes, `actual_version` is `null`; without payload inspection, `payload_verified` is `null`. Receipt/lock digest agreement differs from payload/receipt agreement; neither claims to rehash the original archive. See [JSON check contract](ci.md#json-check-contract).

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `--global` | Check global selections. |
| `--deep` | Inspect installed payloads. |
| `--probe` | Run version probes and record evidence. |
| `--format text\|json` | Default `text`; global `--json` produces the same report. |
| `--target android\|ios\|windows\|macos\|linux\|web` | Add target checks; needs a project, cannot combine with `--global`. |

All selected tools passing requested checks exits `0`. Failed tools still produce a full report and exit `1`. Operational errors exit `1` with JSON `passed: false` and `error`; invalid arguments exit `2`. Read `detail` to distinguish missing installs, payload changes and version mismatches before repairing.

### `exec`

Run a command with the project toolchain and environment, preserving native output and exit status.

#### When to use

Use in scripts, CI or terminals to explicitly use project tools without depending on shell integration. Prefer Maven/Gradle project wrappers.

#### Examples

```sh
pinset exec -- node --version
pinset exec -- ./mvnw verify
pinset exec -- ./gradlew build
```

The first prints the project's Node version, such as `v24.0.0`. The others return the build's own output and status.

#### Behavior

No automatic installation or version changes. Prepares locked tools then runs the raw command. Encrypted profiles need trust and decryption access; conflicting process variables fail. Children can access the network, write files and use external toolchains. This entry is not a sandbox.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `--profile <profile>` | Select a profile for this invocation. |
| `--no-env` | Disable encrypted variables; conflicts with `--profile`. |
| `-- <command...>` | Raw command and arguments. |

Forwards child exit status. Preparation failures exit `1`; invalid arguments exit `2`. Rejects `--json` with `PINSET_NATIVE_OUTPUT`. For `PINSET_ENV_COLLISION`, inspect existing process variables.

## Environment and keys

Profiles hold individually encrypted variables. Common flow: `env init dev` → `env set ... --profile dev` → `env use dev` → `exec`. Commit ciphertext and public authorization; keep private identities in the system credential store or supply `PINSET_IDENTITY`. Review external changes before `env trust add`.

### `env init`

Create an encrypted profile for development, testing or another environment.

#### When to use

Use for the first environment or another set of variables. Add values with `env set`, switch with `env use`.

#### Examples

```sh
pinset env init dev
```

Result: creates `.pinset/env/dev.env` and registers public recipients. Next, run `pinset env set API_TOKEN --profile dev`.

#### Behavior

Writes ciphertext, configuration and transaction state, and refreshes local trust. No network access. Identities normally use the system credential store; no private-key file is generated. Existing profiles require valid trust before editing.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `<profile>` | Name such as `dev`. |

Success exits `0`; failure exits `1`. `PINSET_PROFILE_EXISTS` means the name is taken. Review stale trust before `env trust add`.

### `env use`

Choose a profile for later commands, changing only this machine's project choice by default.

#### When to use

Use to switch environments. Use `exec --profile` for one command or `env remove` to delete a profile.

#### Examples

```sh
pinset env use dev
pinset exec -- node app.js
pinset env use --reset
```

Result: chooses `dev` locally and runs the application. Reset clears the local choice; a shared default may still apply.

#### Behavior

Writes `.pinset/local/profile.json` by default. `--project` changes the shared default and transaction state. No network, decryption or application execution. Precedence: explicit argument, `PINSET_PROFILE`, local choice, shared default. `--no-env` or `PINSET_NO_ENV` disables injection first.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `<profile>` | Existing profile name. |
| `--reset` | Clear the scoped choice; do not supply a name. |
| `--project` | Change the shared default. |

Success exits `0`; failure exits `1`. For `PINSET_PROFILE_MISSING`, check `env list`. Name plus `--reset` exits `2`.

### `env remove`

Delete an unused encrypted profile and clear defaults pointing to it.

#### When to use

Use to retire an environment. Remove one variable with `env unset`; switch with `env use`.

#### Examples

```sh
pinset env remove staging --plan
pinset env remove staging
```

Result: removes `.pinset/env/staging.env`, its configuration and matching shared/local defaults.

#### Behavior

Requires valid trust. Transactionally updates profile, configuration, lock and local choice without network access. Preview deletes nothing. Historical ciphertext in Git is not removed.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `<profile>` | Profile to delete. |
| `--plan` | Preview only. |

Success exits `0`; failure exits `1`. Check `env list` for `PINSET_PROFILE_MISSING`.

### `env list`

List project profiles or the variable names in one profile.

#### When to use

Use to check available environments, defaults and variable names. Access records use `env access list`.

#### Examples

```sh
pinset env list
pinset env list --profile dev
```

Illustrative second output:

```text
profile: dev
variables: 1
  API_TOKEN
```

#### Behavior

Read-only, without network access, value decryption or secret output. Without a profile, lists profiles and shared/local defaults. With one, lists public variable names.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `--profile <profile>` | Inspect this profile's variables. |

Success exits `0`; read failures exit `1`. `PINSET_PROFILE_MISSING` means an absent name.

### `env set`

Add or update a profile variable while saving only ciphertext in project files.

#### When to use

Use to add API tokens, database addresses or rotate values. Create a profile with `env init` first.

#### Examples

```sh
pinset env set API_TOKEN --profile dev
```

The `API_TOKEN:` prompt hides input. Results confirm only `profile`, `variable` and `set`, never the value. Inspect names with `env list --profile dev`.

#### Behavior

Reads and re-encrypts the profile, writing ciphertext and state transactionally without network access. Commit ciphertext/public authorization, never private identities. Toolchain and Pinset control variables are reserved. Process-variable conflicts also fail during execution.

Editing requires explicit `owner_recipients`. Older profiles remain readable. Maintainers must confirm public owners and grants so recipients equal their union, then renew trust. Private keys do not belong in configuration.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `<name>` | Variable name. |
| `--profile <profile>` | Required target. |
| `--stdin` | Read stdin, trimming trailing newlines; maximum 1 MiB. |

Success exits `0`; failure exits `1`. `PINSET_ENV_INPUT_LIMIT` means oversized input; `PINSET_PROFILE_MISSING`, no profile. Do not put secrets directly in command arguments or logs.

### `env unset`

Remove one variable from an encrypted profile while retaining other values.

#### When to use

Use for retired variables. Change values with `env set`; delete environments with `env remove`.

#### Examples

```sh
pinset env unset API_TOKEN --profile dev
```

Illustrative output: `removed: yes`; an absent variable produces `removed: no`.

#### Behavior

Requires trust, decryption access and explicit owner authorization. Updates ciphertext and state transactionally; no other plaintext output or network access. Historical ciphertext may remain in Git.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `<name>` | Variable to remove. |
| `--profile <profile>` | Required target. |

Success, including an absent variable, exits `0`; failure exits `1`. Check profile names for `PINSET_PROFILE_MISSING` and identity access for decryption failures.

### `env access request`

Generate a public request so a maintainer can authorize this machine or CI to read encrypted profiles.

#### When to use

Use for a collaborator's or CI's first access. Requests alone grant nothing; maintainers must `env access grant`.

#### Examples

```sh
pinset env access request
```

Result: prints a public request and JSON path. Share the file, retain the identity locally. For CI, run interactive `pinset env access request --ci` and store the once-displayed identity in a platform Secret.

#### Behavior

No network access. Public requests go under Pinset home; normal identities use the system credential store or an explicit identity. `--ci` requires stdin/stdout TTYs, rejects `--json`, and writes no private-key file. CI can later supply its Secret through `PINSET_IDENTITY`.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `--new` | Create a new normal identity request. |
| `--ci` | Create a separate CI identity, showing its secret once interactively; conflicts with `--new`. |

Success exits `0`; failure exits `1`. `PINSET_CI_IDENTITY_TTY` requires an interactive terminal without private-output redirection. Conflicting arguments exit `2`.

### `env access grant`

Authorize a collaborator or CI to read a profile using its public request.

#### When to use

A maintainer uses this after reviewing the ID and public key. Inspect/revoke with `env access list` and `env access revoke`.

#### Examples

```sh
pinset env access grant ./alice-request.json --profile dev
```

Result: records authorization and re-encrypts `dev` for effective recipients. Commit configuration and ciphertext for the collaborator.

#### Behavior

Requires decryption access, valid trust and confirmed owners. Updates configuration and ciphertext transactionally without network access. Repeated grants write nothing; changing a key for an existing ID fails. Effective recipients combine owners and valid grants.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `<request-file>` | Public JSON from the request command. |
| `--profile <profile>` | Required target. |

Success, including unchanged grants, exits `0`; failure exits `1`. `PINSET_PROTOCOL_UNSUPPORTED` means a non-Pinset-3 request. Verify request source and owner configuration for invalid IDs, keys or authorization.

### `env access revoke`

Remove a profile grant and re-encrypt for the remaining recipients.

#### When to use

Use when a collaborator leaves or a CI identity rotates. Find the ID with `env access list` first.

#### Examples

```sh
pinset env access revoke <request-id> --profile dev
```

Replace the placeholder with the actual ID. Result: removes authorization and updates ciphertext; commit the changed configuration and profile.

#### Behavior

Requires trust and decryption access. Updates configuration, ciphertext and state transactionally without network access. A key retained by an owner or another grant keeps access. Previously copied secrets cannot be recalled; also rotate the service secret when needed.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `<request-id>` | ID to revoke. |
| `--profile <profile>` | Required target. |

Success exits `0`; profile, authorization and decryption errors exit `1`. Confirm the ID and profile in the grants list.

### `env access list`

View public access grants registered for an environment profile.

#### When to use

Use to inspect authorization or find an ID to revoke. Variable names use `env list --profile`.

#### Examples

```sh
pinset env access list --profile dev --json
```

Illustrative JSON with no extra grants:

```json
{"protocol":"pinset/3","grants":{}}
```

#### Behavior

Reads public grants only, without network, decryption or private-key output. Owners are separate from grants; empty grants do not mean owners lack access.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `--profile <profile>` | Required target. |

Success exits `0`; read failures exit `1`. `PINSET_PROFILE_MISSING` means no such profile.

### `env trust add`

Trust reviewed project configuration and encrypted profiles locally so they can be decrypted and edited.

#### When to use

Use before first using variables or after pulling profile changes. Review configuration, public recipients and profile changes first.

#### Examples

```sh
pinset env trust add
pinset exec --profile dev -- node app.js
```

Illustrative first output: `trusted: yes`. The second still needs an authorized identity; trust does not grant decryption access.

#### Behavior

Binds trust to project ID, directory identity, configuration/profile fingerprints. Writes local trust, not shared configuration. No network access. External changes invalidate old trust.

#### Parameters, exit codes and common errors

No command-specific options. Success exits `0`; project/profile read errors exit `1`. Enter the project directory first and do not renew trust for unreviewed changes.

### `env trust status`

Check whether this machine's trust in the current project is still valid.

#### When to use

Use before decryption or after pulling changes. Establish trust with `env trust add`.

#### Examples

```sh
pinset env trust status
```

Illustrative valid output: `trusted: yes`. Invalid trust produces `trusted: no` and a `detail` reason.

#### Behavior

Reads trust and current fingerprints only; no network, decryption or writes. Successful queries do not imply trust. Scripts must inspect JSON `trusted`.

#### Parameters, exit codes and common errors

No command-specific options. Query success exits `0`, including `trusted: false`. File errors exit `1`. Review changes before renewing trust.

### `env trust revoke`

Revoke this machine's trust in the current project to stop subsequent encrypted environment use.

#### When to use

Use to pause trust or diagnose injection. Revoke others' access with `env access revoke` instead.

#### Examples

```sh
pinset env trust revoke
```

Illustrative output: `revoked: yes`; an absent record produces `revoked: no`.

#### Behavior

Deletes local trust without changing configuration, ciphertext or identities. No network access. Review and run `env trust add` before using the environment again.

#### Parameters, exit codes and common errors

No command-specific options. Success, including no prior record, exits `0`; project/local-record errors exit `1`.

## Cleanup

### `clean cache`

Delete downloads no longer referenced by projects or recovery records to free disk space.

#### When to use

Use when download cache grows too large. Installed tools remain; remove unused installs with `clean installs`.

#### Examples

```sh
pinset clean cache --plan
pinset clean cache
```

Result: lists candidates/retained objects, then deletes unreferenced cache. In previews, `removed` means planned paths, not completed deletion.

#### Behavior

Protects registered projects, global locks, venvs and unfinished transactions. Uncertain objects remain. No network or choice changes. Only safe managed cache objects are removed; referenced cache remains useful offline.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `--plan` | Preview candidates and retained items. |

Success exits `0`; failure exits `1`. Inspect paths for `PINSET_CLEAN_UNSAFE`. `uncertain_references: true` means conservative retention, not failure.

### `clean installs`

Delete tool installations no longer used by projects, global choices or managed Python environments.

#### When to use

Use after upgrades to reclaim old-version space. Deselect unwanted tools with `remove`; other projects' references still protect installations.

#### Examples

```sh
pinset clean installs node@22.0.0 --plan
pinset clean installs node@22.0.0
```

Result: only matching, owned and unreferenced installs are deleted. No removable objects can still mean success.

#### Behavior

No network or configuration/lock changes. Protects registered projects, globals, managed venvs and recovery. External objects, missing ownership proof and uncertain references remain. Omit filters to inspect all managed installs.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `[tool@exact...]` | Exact versions only; floating selectors such as `latest` and `lts` are rejected. |
| `--plan` | Preview only. |

Success exits `0`; failure exits `1`. Use exact versions for `PINSET_EXACT_REQUIRED`, inspect paths for `PINSET_CLEAN_UNSAFE`. Do not bypass protection by deleting referenced directories manually.

## Pinset maintenance

### `self info`

View Pinset's current version, platform and paths for setup checks or issue reports.

#### When to use

Use to identify the running Pinset or locate home. Project tools use `list`.

#### Examples

```sh
pinset self info
```

Output includes `version: 3.0.9`, `platform`, `home`, `cli` and eight built-in Providers. Paths/platform vary by machine.

#### Behavior

Read-only; no network, tool execution or configuration changes. Custom tools do not alter the built-in Provider list.

#### Parameters, exit codes and common errors

No command-specific options. Success exits `0`; basic environment errors exit `1`. For CLI/entry version mismatches, inspect `self repair` or reinstall a matched pair.

### `self shell`

Generate a PATH snippet so your shell can run Pinset-managed tool commands directly.

#### When to use

Use when bare commands such as `node` still bypass Pinset. Execute the snippet or add it to startup configuration yourself.

#### Examples

```sh
pinset self shell bash
eval "$(pinset self shell bash)"
```

Illustrative first output:

```sh
export PATH='/example/pinset/v3/bin':"$PATH"
```

For the current PowerShell session: `pinset self shell powershell | Invoke-Expression`.

#### Behavior

Generates text only, without profile writes, network or installation. Prepends the managed command directory to PATH. CLI paths still require installer setup. Save the snippet in shell startup configuration for persistent use.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `<bash\|zsh\|fish\|powershell>` | Required target shell. |

Success exits `0`; basic environment errors exit `1`; invalid shell exits `2`. Inspect PATH order and command caches if routes remain wrong.

### `self completions`

Generate shell completion scripts to make Pinset commands and options easier to enter.

#### When to use

Use for command/option completion. PATH integration is handled separately by `self shell`.

#### Examples

```sh
pinset self completions bash > pinset.bash
source ./pinset.bash
```

Result: the shell saves a script, then enables completion in the current Bash session. Pinset does not edit startup configuration.

#### Behavior

Outputs script text only, without network, automatic completion installation or profile writes. Save/load according to the target shell's conventions.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `<bash\|zsh\|fish\|powershell>` | Required target shell. |

Success exits `0`; basic environment errors exit `1`; invalid shell exits `2`. Do not execute scripts in a different shell.

### `self repair`

Recover interrupted Pinset operations and repair Pinset-managed command entries.

#### When to use

Use after interrupted version changes, self-updates or managed Python rebuilds. Repair a damaged tool with `install --repair`; this handles transactions and entries.

#### Examples

```sh
pinset self repair --plan
pinset self repair
```

Result: previews, then recovers current-project, global and CLI/shim interrupted state. Completed version changes are not undone.

#### Behavior

No network. Unfinished transactions restore configuration, lock, encrypted profiles and local bindings; completed ones only lose leftover markers. Also recovers managed venv rebuilds, entry publication and CLI/shim updates. Symlink launchers resolve to real installations.

Running venv creators prevent directory deletion. Windows replacement failures preserve running entries and report errors. External entries and those without ownership proof also remain.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `--plan` | Read-only recovery preview. |

Success exits `0`; unsafe recovery exits `1`. Wait for creators on `PINSET_VENV_BUSY`. `PINSET_BROKER_VERSION` needs a matched CLI/shim pair. Inspect external-file ownership rather than forcing replacement.

### `self update`

Update Pinset from official releases or install an explicitly selected Pinset 3 version.

#### When to use

Use to upgrade Pinset itself. Project tools use `use`; downgrades need explicit versions.

#### Examples

```sh
pinset self update --plan
pinset self update 3.0.9
```

Result: the first shows release, archive and digest. The second verifies downloads, replaces CLI/shim together and reports restart requirements.

#### Behavior

Accesses the network. No version follows official `/releases/latest` redirects and accepts stable Pinset 3 tags only; explicit versions may select supported prereleases. Neither uses GitHub REST API or needs a token. Downloads stay in one resolved release directory.

Requires a unique valid SHA-256 in `SHA256SUMS`. Prefers `pinset-v<version>-<platform>.zip`, recognizing fixed platform names too. Verifies CLI version before paired replacement; symlink launchers update the real installation. Current versions are not downloaded again; automatic updates reject older latest. Preview reads redirects/checksums without program downloads or writes.

Metadata has timeouts and bounded retries. Long or HTTP-date `Retry-After` responses report waiting instead of retrying early. Network/GitHub traffic limits still apply.

#### Parameters, exit codes and common errors

| Parameter | Meaning |
| --- | --- |
| `[version]` | Exact Pinset 3 release; omit for latest stable. |
| `--plan` | Preview release information only. |

Success, including already-current, exits `0`; failure exits `1`. Access denial, rate limits, missing assets/platforms and invalid checksums have distinct messages. Follow reported network/wait/version guidance. For `PINSET_UPDATE_REPAIR`, inspect `self repair --plan` first.
