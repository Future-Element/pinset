# Pinset core concepts

Pinset separates the versions a project chooses from the tools installed on a machine. Maintainers commit configuration and lock; everyone else installs from that lock instead of deciding versions again.

[简体中文](concepts.zh-CN.md) · [Command reference](commands.md)

## Where to start

| Situation | Commands |
| --- | --- |
| New project without version declarations | `pinset init`, then `pinset use node@24`. |
| Existing `.nvmrc`, `.tool-versions` or similar | Review `pinset import --plan`, then `pinset import` and `pinset install`. |
| Cloned project already using Pinset | `pinset install`, then `pinset check`. |
| Upgrade or switch back | `pinset use tool@selector`. |
| Damaged managed tool installation | `pinset install tool --repair`. |
| Interrupted Pinset operation | Preview recovery with `pinset self repair --plan`. |

`use` installs by default; `import` does not. `install` does not resolve newer versions; `exec` does not install automatically.

## Configuration and lock

`.pinset/config.toml` records project ID, choices, platforms, policy and public environment authorization. This is its tool-selection section, not a complete file:

```toml
[tools]
node = "24"
pnpm = "10"
python = "3.14"
```

`.pinset/lock.toml` records exact versions, platform URLs, digests and installation information. `node = "24"` selects a series; the lock determines the exact patch version and artifact.

Commit both files. `use` updates them; confirmed `import` creates or extends them. Other users' `install` follows the lock rather than that day's latest release. After manually changing selectors or Rust options, explicitly run `use tool@selector` for matching lock records. Do not copy another project's lock: its project ID must match.

Local bindings live in `.pinset/local`; Python projects may have a managed `.venv`. Neither is a shared version choice. Pinset home holds tools, cache, transactions and local identity state. Its default root is `.pinset` in your user directory, with v3 state under `v3`. Set an absolute `PINSET_HOME` to change the root. Installs live at `PINSET_HOME/v3/installs`, command entries at `PINSET_HOME/v3/bin`. `self info` reports actual paths.

## Version selectors

| Selector | Purpose |
| --- | --- |
| Numeric prefix: `node@24`, `python@3.14` | Resolve an available stable version in the series. |
| Exact version: `node@24.0.0` | Choose one version; Java also supports exact builds. |
| `latest` | Latest version supported by the Provider. |
| `lts` | Node or Java long-term-support release. |
| `stable` | Stable Rust. |
| `nightly-YYYY-MM-DD` | Date-pinned Rust nightly. |

Providers have different version formats. Inspect official candidates with `list <tool> --remote`. Legacy `current` is rejected; Rust rejects undated nightly. Generic tools require an explicit version/tag and do not discover `latest` or resolve ranges.

`import` also understands source declarations such as npm ranges, comparisons, wildcards and `lts/*`, resolving and marking exact versions in the plan. Supported import ranges do not imply that every Provider's `use` selector accepts the same syntax.

For multiple team platforms, declare a top-level `platforms` array before running `use` to generate artifacts. Example excerpt:

```toml
platforms = ["linux-x86_64", "macos-aarch64", "windows-x86_64"]
```

The lock must include artifacts needed by every machine or CI runner. Missing tool artifacts fail resolution. Project target support and published Pinset CLI platforms are separate; see installation instructions and [CI](ci.md) for CLI availability.

## Project and global scope

Run commands in a project or child directory. Pinset finds `.pinset/config.toml` within project boundaries. `init` is different: it creates configuration only in the specified directory. `-C/--cwd` supplies that working directory without changing your shell:

```sh
pinset -C ./packages/api install
pinset use node@24 --global
```

`--global` is available only on commands that expose it, and operates on global configuration/lock under Pinset home. Explicit project choices win; unselected tools can inherit global defaults. Inherited tools and runtimes still obey project policy. Broken explicit choices, mismatched locks and missing installations fail rather than silently falling back to global or system tools.

To require project choices, set:

```toml
[policy]
require_project_tools = true
```

Verification-strength and minimum-release-age policies are also enforced during installation and routing. Global inheritance does not bypass project policy.

## Preview, inspection and execution

`--plan` is the common preview flag on mutation commands that support it. It writes neither project nor managed Pinset state, but does not mean offline: `use` and `import` may resolve metadata; generic tools may download temporarily to compute a real digest; `self update` reads release checksums.

Local `list`, `which` and plain `check` are read-only. `list --remote` accesses the network. `check --deep` reads and verifies payloads. `check --probe` explicitly starts tools and records evidence. `--format json` and global `--json` change output, not which deep checks or probes run.

`exec` keeps native output and exit status. After `self shell` integration, bare managed commands also work. Without integration, use `pinset exec -- node --version` explicitly. Neither entry is a sandbox; applications can still access networks and files.

## Profiles and collaboration

A profile holds individually encrypted environment values. A development flow:

```sh
pinset env init dev
pinset env set API_TOKEN --profile dev
pinset env use dev
pinset exec -- node app.js
```

Input is hidden; project files contain ciphertext. New collaborators run `env access request` and share the public request. Maintainers run `env access grant` and commit updated ciphertext/authorization. Collaborators retrieve and review changes, then `env trust add`; their identities must also have decryption access.

Profile precedence: command argument, `PINSET_PROFILE`, local choice, shared default. `--no-env` or `PINSET_NO_ENV` disables injection first. `env use --reset` clears one scope's choice, not all possible defaults. `env list` shows profiles or public variable names only.

Private identities use the system credential store or explicit `PINSET_IDENTITY`, never repository commits. Interactive `env access request --ci` displays a CI identity once for storage in a platform Secret. Local trust permits reviewed project use; it does not replace authorization. Revocation cannot recall copied secrets.

## Output and errors

`--lang auto|en|zh-CN` selects terminal language. Most commands support global `--json`; `exec` rejects it to preserve native output. `check --format json` has a stable [CI report contract](ci.md#json-check-contract), including explicit `null` fields for checks not performed.

Normally success exits `0`, operational failure `1`, invalid arguments `2`; `exec` forwards child status. Failed `check` reports exit `1`. A successful `env trust status` query may exit `0` with `trusted: false`, so scripts must read the field. A successful preview or status query does not prove an application build passed.

