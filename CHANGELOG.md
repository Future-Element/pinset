# Changelog

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
