# Changelog

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
