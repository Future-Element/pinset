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

Artifact locks use the checksum algorithm and digest, independently of the installation identity. SDKs with different Rust profiles or components cannot race over a shared partial download. Managed installation repair quarantines and restores payloads under the same per-install lock.

Project and internal resource locks have separate namespaces. A project ID equal to `shims`, `registry`, `self-update` or `maintenance` cannot collide with an internal lock or turn a project operation into exclusive cleanup.

`self repair` recovers interrupted transactions for the current project and global selections. Prepared journals restore prior config, lock, encrypted profiles and local bindings. Committed or already recovered journals with a surviving marker need marker cleanup only. Recovery validates prior state before writes and runs under the same project and maintenance locks. `--plan` previews recovery without creating state. Completed version changes are switched back using `use`.

Venv journals are registered globally and bound to the project directory, both interpreter identities and an owned creation token. Python creates the environment at its final path; a failed or interrupted replacement restores the previous owned environment. Recovery precedes configuration recovery and refuses to remove directories while the recorded writer process is still alive. Cleanup protects both interpreter identities until recovery completes.

Managed CLI/router and alias publication stages files and preserves backups on the destination volume. A refresh journal covers all entries and the command registry, including CLI/router digests. Replacement never unlinks an old entry first. Interrupted publication can resume or roll back, and unchanged entries are reused. A legacy registry without a CLI digest cannot authorize replacement of a different CLI found in the managed bin directory; confirm and move that unproven entry before running repair. Repair does not execute unknown binaries to establish ownership.

## Execution and evidence

Shims and `exec` use core's route plan and never install or download. Managed entries missing from a project fail. Absolute or explicit external commands are native executions, not sandboxed processes. Child PATH includes managed shims to enforce subsequent managed entry routing. Java uses one JDK root; Python/pip share one owned environment; pnpm uses selected Node; Go sets `GOTOOLCHAIN=local`; Rust uses its exact SDK.

Every route validates the current configuration policy against the lock. Availability checks cover the requested tool and required runtimes, so an unrelated missing SDK does not block a usable entry. Python routing checks `pyvenv.cfg`, package isolation and the interpreter's actual base SDK; versioned Python/pip aliases and child PATH use that same environment. Explicit Python probes report executable, environment prefix, base prefix and base executable as well as version. Windows bare managed commands are matched without case sensitivity, and native exit codes remain full-width through both the CLI and its private process worker.

Default `check` performs no network, subprocess, decryption or writes. Deep checks inspect payloads; probes are explicit, bounded subprocesses. Reports distinguish configuration, installation, binding and entry observation. Static Maven/Gradle/IDE declarations never imply observed compiler or daemon state.

The private environment broker negotiates protocol and exact CLI version with the adjacent shim, returns a bounded binary payload, and is absent from public help/completions. Identity keys are removed from child environments. Internal profile markers avoid decrypting again in an already resolved child scope; the project fingerprint is still checked.

Rust locks the official compiler artifact plus each selected profile/component/target artifact individually. Minimal profiles omit documentation, formatting and lint components unless explicitly selected. Platform-independent sources use the same transaction and checksum path; unavailable components produce an explicit platform error. Nightly probes compare the compiler release, commit and commit date from the verified manifest, independently of its distribution date.
