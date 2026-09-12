# Development environment delivery

Implementation authorized on 2026-09-12. Development versions merge after validation; no intermediate tags, releases, Marketplace publication, or website deployment.

| Development version | Scope | State |
| --- | --- | --- |
| 2.13.0 | M0–M2: shared environment report, setup, execution evidence, VS Code integration | In progress |
| 2.14.0 | M3–M4: compatibility and team delivery | Pending |
| 2.15.0 | M5–M6: worktree isolation and candidate evidence | Pending |

`release.json` records the published distribution version. Installers, the Action default and downloadable devcontainer examples keep using that version during development. Release preparation updates it and distribution defaults together with the final version, before the final preflight. No intermediate development version is advertised as downloadable.

Validation uses local Docker first, as authorized on 2026-09-12. Merge-required CI and platform-specific native acceptance are consolidated after local checks pass. Linux containers do not establish Windows/macOS native behavior. Preserve the existing icon/package changes in the original checkout; implementation runs in a separate worktree.

Acceptance remains pending until supported entry points have native execution evidence. Source changes and static checks alone do not complete M0–M2.

2.13 Docker acceptance passed on Linux x64: workspace formatting/Clippy/tests, integration contracts, extension typing/tests/VSIX packaging, website typing/build/SEO, setup/resume/explicit-task handling, managed Node/Python probes, actual VS Code Node/Python/Flutter debug launches and Java home observation. Native versions observed: Node 24.1.0, Python 3.13.15, Flutter 3.35.3 with Dart 3.9.2. Remaining Windows/macOS acceptance and the merge gate are tracked in PR #62; no intermediate release is created.
