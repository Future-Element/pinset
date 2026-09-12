# Development environment delivery

Implementation authorized on 2026-09-12. Development versions merge after validation; no intermediate tags, releases, Marketplace publication, or website deployment.

| Development version | Scope | State |
| --- | --- | --- |
| 2.13.0 | M0–M2: shared environment report, setup, execution evidence, VS Code integration | Merged as 4f7e2d7; four-platform native acceptance and CI Gate passed |
| 2.14.0 | M3–M4: compatibility and team delivery | Implemented; local Docker validation passed, native merge checks pending |
| 2.15.0 | M5–M6: worktree isolation and candidate evidence | Pending |

`release.json` records the published distribution version. Installers, the Action default and downloadable devcontainer examples keep using that version during development. Release preparation updates it and distribution defaults together with the final version, before the final preflight. No intermediate development version is advertised as downloadable.

Validation uses local Docker first, as authorized on 2026-09-12. Merge-required CI and platform-specific native acceptance are consolidated after local checks pass. Linux containers do not establish Windows/macOS native behavior. Preserve the existing icon/package changes in the original checkout; implementation runs in a separate worktree.

2.13 passed four-platform native acceptance in run 34682688713 before PR #62 merged. Source changes and static checks alone are not used as native execution evidence. SSH, WSL and untested extension entry points remain outside that acceptance.

2.13 Docker acceptance passed on Linux x64: workspace formatting/Clippy/tests, integration contracts, extension typing/tests/VSIX packaging, website typing/build/SEO, setup/resume/explicit-task handling, managed Node/Python probes, actual VS Code Node/Python/Flutter debug launches and Java home observation. Native versions observed: Node 24.1.0, Python 3.13.15, Flutter 3.35.3 with Dart 3.9.2. Remaining Windows/macOS acceptance and the merge gate are tracked in PR #62; no intermediate release is created.

## 2.14 local acceptance, 2026-09-12

Linux Docker passed workspace formatting, Clippy, unit/integration tests and CLI/schema contracts; real TLS rejected an untrusted leaf and accepted the explicitly configured test CA. Source fallback ordering, proxy/auth/timeout failures, full-platform offline hashes and malformed/tampered bundle rejection passed without contacting registered-but-unselected sources.

The real VS Code 1.137 fixture passed Node 24.1.0, Python 3.13.15 in its managed venv, Flutter 3.35.3 / Dart 3.9.2 and Java 21 home selection. Website typecheck, production build and 162-page SEO validation passed; production dependency audit reported zero advisories. Go/Rust/.NET real SDK probes are a separate opt-in fixture in `scripts/tests/sdk_versions_test.py`; compatibility fixtures alone do not establish runtime execution or project builds.

The Rust default-profile regression uses the official package aliases and Windows `rust-mingw` component from the 1.86 manifest, validates the generated lock on all declared targets, and rejects incomplete component sets.
