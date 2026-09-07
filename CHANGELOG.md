# Changelog

## 2.3.0 - 2026-09-07

- Add schema 5 project tasks with argument-array commands, optional project-relative working directories, environment profiles, descriptions, and appended arguments through `pinset run <task> -- <arguments...>`.
- Add typed environment-variable contracts for strings, integers, booleans, URLs, and enums, with required fields, profile filters, non-secret defaults, and secret-default rejection. Validate contracts before command injection.
- Add `pinset env check` and `pinset env diff` reports that expose readiness and variable structure without printing secret values or secret-derived hashes.
- Preserve schema 4 projects until explicit migration, retain TOML comments during schema-only migration, and keep migration writes atomic under the existing project-state lock.

Project configuration migrates to schema 5; the runtime lock remains schema 3. Schema 1-4 projects remain readable, and existing schema 4 encrypted environments continue to work before migration.

## 2.2.0 - 2026-09-07

- Add `pinset [-C <directory>] [-e <profile> | --no-env] -- <command> [arguments...]` for managed tools, project Python commands, and explicit external programs. Preserve the existing `exec` and `x` contracts.
- Share runtime PATH, runtime variables, and profile selection between CLI execution and shims. Remember machine-local project profiles with `env use`; clear them with `env reset` or `env use --reset`. Bind preferences to canonical project paths and project IDs, and ignore local preferences in CI.
- Add an interactive `env init` workflow, stored identity reuse, current environment display, and `env share`, `env unshare`, and `env members` shortcuts.
- Update bilingual help, command references, completions, and native CI coverage. Accept any positive `rc.N` release candidate whose base matches the workspace version.

No project configuration or lock schema change is required. Tasks and variable contracts remain planned for 2.3.

## 2.1.6 - 2026-08-31

- Check and migrate a safely recognized incompatible global lock before `pinset self update` downloads a release.
- Classify the historical Node.js HTTPS checksum as checksum-strength so its OpenPGP upgrade passes anti-downgrade validation.
- Document `pinset migrate --global` as the explicit repair command and keep unknown or corrupted lock shapes fail-closed.

No configuration or lock schema migration is required. The compatibility migration re-resolves selected pre-1.0 Node.js, pnpm, Bun, Go, Python, Java, Rust, and .NET SDK records at their existing exact versions. Flutter is unchanged.

## 2.1.5 - 2026-08-31

- Repair every exact pre-1.0 Provider target matrix expanded for Linux ARM64: Node.js, pnpm, Bun, Go, Python, Java, Rust, and .NET SDK.
- Re-resolve each affected Provider at its existing exact version, preserve its requested selector, and upgrade legacy Node.js locks to the current OpenPGP-authenticated metadata contract.
- Apply the shared compatibility migration to project/global selection, update, migrate, unset, and project-import state changes while keeping install, execution, and other ordinary lock reads strict.
- Reject unknown or corrupted target gaps, validate all existing artifacts before migration, and leave configuration and lock bytes unchanged when any Provider refresh fails.

No configuration or lock schema migration is required. Flutter is unchanged because its supported target matrix did not add Linux ARM64. Existing unselected installations and their receipts are preserved; only selected lock records are rebuilt during an explicit state-changing command or `pinset migrate`.

## 2.1.4 - 2026-08-31

- Repair pre-1.0 pnpm and Bun locks that predate Linux ARM64 target coverage when a project or global selection is next changed.
- Preserve existing selectors and exact resolved versions while rebuilding only the missing npm Provider target records from verified official metadata.
- Keep ordinary lock reads strict, reject corrupted existing artifacts during repair, and preserve the previous lock bytes exactly if a paired configuration write must roll back.
- Refresh the yanked `chacha20` transitive dependency to its maintained compatible release.

No configuration or lock schema migration is required. The compatibility repair is limited to the historical missing `linux-aarch64` pnpm/Bun artifact and runs only during an explicit `pinset use` or `pinset global` state change.

## 2.1.3 - 2026-08-27

- Strengthen the documentation site's Google site-name signals by keeping the Pinset brand identity and canonical subdomain consistent across exported pages.
- Validate site-name, Open Graph, and canonical metadata during website CI and release preflight.

## 2.1.2 - 2026-08-27

- Make inherited global Providers and peer runtime environment roots available to managed child processes.
- Preserve ordinary system-PATH passthrough for unselected Providers even when their managed Provider metadata declares dependencies.
- Serialize project/global state mutations across processes, preserve concurrent batch updates, and register known projects for safer uninstall/prune reference checks.
- Harden Windows batch argument forwarding and self-update locking, timeout, result reporting, and release-asset immutability.
- Validate the documentation website and its release version in CI and release preflight.

## 2.1.1 - 2026-08-26

- Include every installed Provider selected by the active project configuration—or by global configuration when no project applies—in routed runtime `PATH`, so package-manager scripts can invoke locked peer runtimes such as `pnpm -> bun` without falling back to a Pinset shim.
- Replace the blanket shim-depth guard with a bounded command-chain guard that permits legitimate cross-Provider calls while still rejecting real command cycles.

No configuration or lock schema migration is required.

## 2.1.0 - 2026-08-21

- Allow `pinset global` and `pinset use` to accept a variable-length batch of selections across any supported, non-duplicate Provider set whose dependencies are satisfied.
- Parse and resolve the complete batch before one configuration/lock update, preserving unmentioned selections and leaving state unchanged on pre-commit failure.
- Run installation once after the batch state commit in deterministic Provider dependency order; installation failure preserves the complete lock and reports the appropriate locked-install retry command.
- Keep no-argument `global`, single-selection compatibility, `--no-install`, completions, and English/Chinese help and command documentation aligned with the batch interface.

No configuration or lock schema migration is required. Explicit `install <tool@exact-version>` remains single-selection; `install --locked` continues to install the complete project or global scope.

## 2.0.0 - 2026-08-21

- Add schema 4 project identities and profile-scoped age X25519 encrypted environments with explicit recipients, recovery files, system credential-store identities, portable dotenv import/export, and collision-safe direct shim injection.
- Add local project trust bound to canonical root, `project-id`, the complete environment policy, profile paths, and recipient lists. CI may provide `PINSET_IDENTITY` and an explicit non-secret `trust-project-id`.
- Add `paths`, `list --long`, `doctor --deep`, all-Provider shim registration, schema 3 installation receipts, owned-install repair, and a checksum-verifying Windows installer.
- Add stable/prerelease self-update checks and paired CLI/shim replacement with checksum verification, version handshake, backups, rollback, and a Windows replacement helper.
- Keep `pinset-shim` free of age, keyring, download, and archive dependencies; it invokes only the adjacent matching CLI when a trusted environment profile is selected.
- Raise the minimum supported Rust version to 1.97 so development, CI, and Pinset's current Rust Provider use one toolchain baseline.

Project configuration migrates to schema 4 and gains a generated `project-id`; `pinset.lock` remains schema 3. Run `pinset migrate --dry-run` before migration. KMS/OIDC, arbitrary age plugins, daemons, hooks, remote secret synchronization, and general-purpose vault behavior remain out of scope.

## 1.9.0 - 2026-08-20

- Add `pinset x <tool>@<selector> -- <command>` for verified one-shot execution. It resolves and installs the requested runtime in Pinset-owned storage, preserves the child exit code, and never creates or modifies project/global selection or lock state.
- Resolve declared Provider dependencies before one-shot installation. A pnpm one-shot uses the project/global Node.js selection and fails explicitly when that dependency is unavailable.
- Add a checksum-verifying composite GitHub Action, a Renovate custom-manager preset, JSON Schemas for `pinset.toml` and `pinset.lock`, and a verified Dev Container example.
- Generate ready-to-consume Winget, Scoop, and Homebrew manifests from the exact release archive hashes. The manifests are checksummed, attested, and published alongside every release.
- Add static integration-contract tests to CI and release preflight so action, schema, container, Renovate, and package-manifest drift blocks publication.

No configuration or lock schema migration is required. v1.9 continues to write schema 3 and read schema 1/2. One-shot installation may populate Pinset-owned download/cache/install directories, but selection state remains unchanged.

## 1.8.0 - 2026-08-20

- Add a constrained, clear-signed Provider Registry preview with a pinned OpenPGP signer. `pinset provider list` and `pinset provider verify [REGISTRY]` validate declarative manifests without installing, activating, or executing third-party code.
- Reject unknown manifest fields, unsupported capabilities, duplicate identifiers or commands, missing dependencies, dependency cycles, non-regular registry files, oversized inputs, unsigned data, and signature tampering.
- Add declarative Provider dependency graphs and topological install/state validation. pnpm now explicitly depends on Node.js instead of inheriting unrelated selected runtimes.
- Build each routed command's composite `PATH` and environment from its selected Provider plus declared transitive dependencies, preserving deterministic order and reporting missing dependencies explicitly.
- Keep registry verification out of the runtime-independent shim dependency graph; the embedded registry is also checked against built-in commands, dependencies, and provenance declarations to detect drift.

No configuration or lock schema migration is required. v1.8 continues to write schema 3 and read schema 1/2. The Registry is a read-only preview: only Providers compiled into Pinset may install or activate in this release.

## 1.7.0 - 2026-08-20

- Add a common provenance verifier contract and one verification vocabulary for HTTPS checksums, OpenPGP/Minisign signed checksums, npm registry signatures, Sigstore bundles, GitHub Attestations, and SLSA provenance. Node.js OpenPGP validation now runs behind that contract.
- Add optional project-wide `verification-strength = "checksum" | "signed-checksum" | "provenance"` and `minimum-release-age = "<n>d|h|m|s"` policy fields. Selection, import, update, project install, and lock audit fail closed when the lock cannot satisfy them.
- Record upstream release timestamps in new locks when the consumed Provider metadata supplies one. Go deliberately reports release age as unavailable because its official downloads JSON has no release timestamp.
- Reject silent verification downgrades when replacing an existing tool lock, while keeping schema 3 and legacy schema 1/2 reads compatible.
- Extend Provider capabilities and lock-audit findings with provenance methods, release-time availability, and stable `verification_below_policy`, `release_age_unavailable`, and `release_too_new` reason codes.

No schema migration is required. The optional `released-at` lock field and project policy keys are schema-3-compatible; existing locks remain valid until a project explicitly enables a policy they cannot satisfy.

## 1.6.0 - 2026-08-20

- Add `pinset lock audit` for project or global scope. It is always read-only and offline, checks config/lock consistency, current-platform artifacts, referenced cache bytes, install receipts, receipt-backed ownership, and project Python environment ownership.
- Add a stable audit finding contract with snake_case reason codes, severity/category/subject/path context, explicit repair plans, and JSON schema 1 command identity `lock.audit`.
- Reserve exit code `1` for a completed audit that found action-required errors or warnings; clean audits and informational-only optional cache misses return `0`, while command failures remain `2`.
- Consolidate command layout, metadata, installation, environment, traditional discovery, and lock-audit behavior into one capability model shared by all nine built-in Providers.
- Add cross-platform CLI and core regression coverage for read-only behavior, matching/missing receipts, optional cache state, JSON output, exit semantics, Provider capability coverage, completions, and parser options.

No configuration or lock schema migration is required. v1.6 continues to write schema 3 and read schema 1/2; `lock audit` reports legacy state and repair plans without changing it.

## 1.5.1 - 2026-08-19

- Upgrade `pgp` to 0.19.0 to resolve three runtime dependency advisories, including two high-severity parser denial-of-service issues.
- Raise the minimum supported Rust version to 1.88, required by the patched OpenPGP implementation.
- Bound Node.js clear-signed manifest input and cover valid, malformed, oversized, and deeply repeated-signature inputs without panics.
- Block Pull Requests and Releases when the pinned RustSec audit reports unapproved vulnerabilities or warnings; document the single non-reachable, unfixed RSA private-key timing exception.

## 1.5.0 - 2026-08-19

This release deliberately changes the project-resolution contract before broad adoption.

- Write schema 3 project/global configuration and lockfiles while retaining schema 1/2 readers.
- Keep requested selectors in configuration and exact resolved versions in lockfiles.
- Make projects strict by default, with explicit global inheritance, system fallback, and Git/filesystem boundary policy.
- Add `pinset current --explain`, `pinset which --explain`, and traditional-source diagnostics in `pinset doctor`.
- Add constraint-aware `pinset outdated`, lock-only `pinset update`, and explicit `pinset migrate`.
- Move provider-specific traditional-file declarations into Provider manifests; ordinary runtime routing still does not inspect those files.

Direct downgrade from schema 3 is not supported. Run `pinset migrate --dry-run` before migrating existing state and commit `pinset.toml` together with `pinset.lock`.
