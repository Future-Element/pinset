# Pinset 3 architecture and protocol

| Crate | Responsibility | Dependencies |
| --- | --- | --- |
| pinset-core | Strict models, read-only project discovery, lock/receipt matching, route plans and public reports | No HTTP, crypto, credentials or writes |
| pinset-engine | Official metadata, install/cache/extraction, local bindings, transactions and maintenance | core, env |
| pinset-env | Per-value age, identities, system credentials, authorization and project trust | core |
| pinset-cli | Eleven command families, input, output and service dispatch | engine, core |
| pinset-shim | Read-only route and native process launch; version-matched private broker | core |

The eight Providers are compiled into engine. They describe official metadata, artifacts and required entries; one installer implements download verification, safe extraction, content-addressed cache, locks, staging and commit. XZ uses `lzma-rust2`; no liblzma build dependency is used.

Public models are `ProjectContext`, `LockedTool`, `InstallReceipt`, `CommandPlan`, `EnvironmentReport`, `ExecutionEvidence` and `TransactionJournal`. TOML files require `protocol = "pinset/3"` and `schema = 3`. Unknown fields and previous protocols fail. JSON reports use the same protocol and stable `PINSET_*` errors.

Configuration holds only tool selectors, platforms, Rust options, artifact verification policy and public profile metadata. It never stores task definitions. Locks include exact builds, official artifact URLs, digests, verification methods and identity-affecting options. Install identity excludes the floating selector and includes exact payload identity.

Every operation carries an explicit canonical project root and filesystem directory identity. Config-file parents are never used as an implicit root. Project discovery obeys Git/worktree and home boundaries, chooses the nearest independent configuration and never merges selections.

## Transactions and concurrency

Writers hold a project lock and a shared maintenance lock. Cleanup uses the exclusive maintenance lock. Install staging has a per-install lock; registry and shim updates have separate locks. Config and lock writes are individually atomic and protected by a journal plus `.pinset/local/transaction.json`. Readers reject an unfinished pair. Local bindings complete before the journal is committed and the marker is removed.

`self repair` recovers interrupted transactions for the current project and global selections. Prepared journals restore prior config, lock, encrypted profiles and local bindings. Committed or already recovered journals with a surviving marker need marker cleanup only. Recovery validates prior state before writes and runs under the same project and maintenance locks. `--plan` previews recovery without creating state. Completed version changes are switched back using `use`.

## Execution and evidence

Shims and `exec` use core's route plan and never install or download. Managed entries missing from a project fail. Absolute or explicit external commands are native executions, not sandboxed processes. Child PATH includes managed shims to enforce subsequent managed entry routing. Java uses one JDK root; Python/pip share one owned environment; pnpm uses selected Node; Go sets `GOTOOLCHAIN=local`; Rust uses its exact SDK.

Default `check` performs no network, subprocess, decryption or writes. Deep checks inspect payloads; probes are explicit, bounded subprocesses. Reports distinguish configuration, installation, binding and entry observation. Static Maven/Gradle/IDE declarations never imply observed compiler or daemon state.

The private environment broker negotiates protocol and exact CLI version with the adjacent shim, returns a bounded binary payload, and is absent from public help/completions. Identity keys are removed from child environments. Internal profile markers avoid decrypting again in an already resolved child scope; the project fingerprint is still checked.

Rust locks the official compiler artifact plus each selected profile/component/target artifact individually. Minimal profiles omit documentation, formatting and lint components unless explicitly selected. Platform-independent sources use the same transaction and checksum path; unavailable components produce an explicit platform error. Nightly probes compare the compiler release, commit and commit date from the verified manifest, independently of its distribution date.
