# Development environment delivery

Implementation authorized on 2026-09-12. Development versions merge after validation; no intermediate tags, releases, Marketplace publication, or website deployment.

| Development version | Scope | State |
| --- | --- | --- |
| 2.13.0 | M0–M2: shared environment report, setup, execution evidence, VS Code integration | In progress |
| 2.14.0 | M3–M4: compatibility and team delivery | Pending |
| 2.15.0 | M5–M6: worktree isolation and candidate evidence | Pending |

`release.json` records the published distribution version. Installers, the Action default and downloadable devcontainer examples keep using that version during development. Release preparation updates it and distribution defaults together with the final version, before the final preflight. No intermediate development version is advertised as downloadable.

Validation follows CONTRIBUTING.md: compilation, formatting, tests and native execution run in disposable GitHub Actions runners. Preserve the existing icon/package changes in the original checkout; implementation runs in a separate worktree.

Acceptance remains pending until supported entry points have native execution evidence. Source changes and static checks alone do not complete M0–M2.
