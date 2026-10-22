# Local Docker verification

The local validation entry is `verify.ps1 -Suite fast|acceptance|platform|integrations|x64|all|hotfix`, or `./verify.sh <suite>`. No local suite invokes CI. Native checks on other operating systems and architectures are available separately through the manually dispatched `verify-platforms.yml` workflow. The Docker image pins the current Rust and Node baselines by digest.

The original checkout is mounted read-only at `/source`, copied to `/workspace`, and built there. Cargo and official SDK archives use separate Docker volumes. The same container is reused, with fresh Pinset state, projects, venvs, profiles, identities and trust for every verification run. SDK cache hits are reverified. Host credentials, PATH and shell profiles are never mounted or edited.

| Suite | Required checks |
| --- | --- |
| fast | Format, clippy, Rust contracts, command/project/failure contracts, standalone 2.x upgrade/migration/cleanup contracts, dependency audit, distribution contracts |
| acceptance | Official SDK installs and execution; Java 8/11/17/21/25/latest GA, Python interpreter drift/versioned entries/package isolation, partial installation/runtime dependencies, current-policy rejection, venv interruption recovery, Node/pnpm/Bun/Go, Rust components and direct version switching |
| platform | Linux ARM64 cross build and QEMU execution, official ARM64 SDK combinations, Windows/macOS target compilation and path/credential contracts |
| integrations | Independent extension/website typechecks, package audits, packaging and SEO, real Linux Secret Service and grant/owner revocation, nested and multi-root VS Code bindings under Xvfb, Java wrapper builds and GUI startup, Flutter metadata, locks, routing and Android evidence contracts |
| x64 | Fast checks, real Linux x64 SDK acceptance, Windows x64 target compilation and integrations, all inside Linux x64 Docker; excludes ARM64 and macOS checks |
| all | All fast, acceptance, platform and integration checks; missing/failed checks prevent success |
| hotfix | Explicitly authorized global-default/shim repair scope: format, clippy, binary build, workspace Rust contracts, application contracts and installer/self-update/distribution contracts |

Flutter SDK execution and Android SDK/APK builds are exempt from download tests by explicit user request. They remain unverified, including when an archive happens to be cached. Required metadata, lock, route and deterministic evidence contracts still run. Reports carry this fixed exemption and the publication metadata gate requires it; suite success does not imply Flutter runtime or APK acceptance.

Exact SDK builds and artifacts are fixed in the run's SDK manifest at initialization. Platform combinations without official artifacts must produce explicit errors. GUI reports distinguish complete inventory, observed startup and the actions actually exercised.

When an upstream release is still publishing its platform artifacts, a rerun can explicitly reuse an earlier run's exact SDK baseline: `./verify.ps1 -Suite x64 -SdkSelections output/verify-v3-<UTC>/frozen-selectors.json`. The input must be a nonempty JSON selector-to-exact-version object inside this repository. The report records its path, SHA-256 and selections. All installations, checksum checks and runtime checks required by the selected suite still run. Without this option, selectors resolve normally. This fixes verification inputs only; it does not change Provider selection behavior or add platform coverage.

Reports are written under `output/verify-v3-<UTC>/`. `report.json` and its SHA-256 bind commit, initial source fingerprint, Cargo lock, image digests, executed commands and results. Changes to the original source during verification invalidate the report. A development report from a dirty checkout cannot authorize publication.

Linux x86_64 runs natively inside the container. The existing `platform`/`all` definitions retain their broader cross-compilation and emulated ARM64 scope. They do not establish native Windows/macOS behavior. The current local scope does not configure QEMU or run unrelated-platform checks; use the independent manual workflow below for native cross-platform evidence.

For the user-selected local 3.0.9 scope, run `./verify.ps1 -Suite x64`. It performs fast checks, real SDK acceptance, Windows x64 target compilation and integrations inside Linux x64 Docker. It does not compile ARM64 targets, configure or run QEMU, run native ARM64 tests, compile macOS targets, or run native macOS tests. Windows target compilation is not native Windows runtime validation. The report must retain these exclusions. This scope does not invoke `all`; `all` keeps its original full-platform meaning. The separately authorized native CI run has its own report and does not change what the local report covers.

Keep `all` for an explicitly selected full local scope; do not invoke it for the current-system-only local scope. When the user explicitly requests limited hotfix validation, `hotfix` produces its own report rather than an `all` or `fast` result. Its fixed command list and command logs are required by the release gate. It does not rerun SDK downloads/runtime acceptance, ARM64/QEMU or Windows/macOS cross compilation, native Windows/macOS execution, dependency security scans, or editor/website/credential-store integrations. The report records every one of these limits and remains bound to a clean commit, source fingerprint, Cargo lock and image digests.

Publication is a separate manual workflow that accepts the existing tag, exact compact `all`, explicitly authorized `x64` or authorized `hotfix` report and SHA-256. It performs metadata validation, compilation, packaging, checksums/SBOM/provenance and publishing, with no testing hooks. Release compilation does not expand the runtime coverage in the local report.

## Manual native-platform verification

`.github/workflows/verify-platforms.yml` is an independent, opt-in verification workflow. It runs only through `workflow_dispatch`; pushes, pull requests, tags and publication do not automatically trigger it. The release workflow continues to build and publish without tests. The earlier repository-wide CI testing restriction is replaced by this separation: publication and explicitly requested verification have different workflows.

The native matrix covers the four published targets: Linux x64, Linux ARM64, Windows x64 and macOS ARM64. Shared checks live in `scripts/tests/ci_platform.py`. Native runners provide their own platform evidence; the local machine does not configure emulation or test unrelated platforms.

| Mode | Purpose |
| --- | --- |
| `source` | Build the candidate source and run native platform checks before publication. |
| `release` | Download a published version and verify the release package and `setup-pinset` Action. `version` defaults to `latest` and also accepts an explicit release. |

For example, manually dispatch candidate checks or the published 3.0.9 checks:

```sh
gh workflow run verify-platforms.yml -f mode=source
gh workflow run verify-platforms.yml -f mode=release -f version=3.0.9
```

For 3.0.9, the user has authorized native verification across this matrix in addition to the limited local suite. Report each run's actual checks and results separately. A dispatched or planned run is not a pass, and source checks do not establish that a later published package or Action was verified.
