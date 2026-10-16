# Local Docker verification

The only public validation entry is `verify.ps1 -Suite fast|acceptance|platform|integrations|all|hotfix`, or `./verify.sh <suite>`. No suite invokes CI. The image pins the current Rust and Node baselines by digest.

The original checkout is mounted read-only at `/source`, copied to `/workspace`, and built there. Cargo and official SDK archives use separate Docker volumes. The same container is reused, with fresh Pinset state, projects, venvs, profiles, identities and trust for every verification run. SDK cache hits are reverified. Host credentials, PATH and shell profiles are never mounted or edited.

| Suite | Required checks |
| --- | --- |
| fast | Format, clippy, Rust contracts, command/project/failure contracts, standalone 2.x upgrade/migration/cleanup contracts, dependency audit, distribution contracts |
| acceptance | Official SDK installs and execution; Java 8/11/17/21/25/latest GA, Python interpreter drift/versioned entries/package isolation, partial installation/runtime dependencies, current-policy rejection, venv interruption recovery, Node/pnpm/Bun/Go, Rust components and direct version switching |
| platform | Linux ARM64 cross build and QEMU execution, official ARM64 SDK combinations, Windows/macOS target compilation and path/credential contracts |
| integrations | Independent extension/website typechecks, package audits, packaging and SEO, real Linux Secret Service and grant/owner revocation, nested and multi-root VS Code bindings under Xvfb, Java wrapper builds and GUI startup, Flutter metadata, locks, routing and Android evidence contracts |
| all | Every suite above; missing/failed checks prevent success |
| hotfix | Explicitly authorized global-default/shim repair scope: format, clippy, binary build, workspace Rust contracts, application contracts and installer/self-update/distribution contracts |

Flutter SDK execution and Android SDK/APK builds are exempt from download tests by explicit user request. They remain unverified, including when an archive happens to be cached. Required metadata, lock, route and deterministic evidence contracts still run. Reports carry this fixed exemption and the publication metadata gate requires it; suite success does not imply Flutter runtime or APK acceptance.

Exact SDK builds and artifacts are fixed in the run's SDK manifest at initialization. Platform combinations without official artifacts must produce explicit errors. GUI reports distinguish complete inventory, observed startup and the actions actually exercised.

Reports are written under `output/verify-v3-<UTC>/`. `report.json` and its SHA-256 bind commit, initial source fingerprint, Cargo lock, image digests, executed commands and results. Changes to the original source during verification invalidate the report. A development report from a dirty checkout cannot authorize publication.

Linux x86_64 runs natively inside the container. ARM64 runs under QEMU and is labeled as emulated. Windows x86_64 and macOS aarch64 receive target compilation and contract checks only; their native behavior remains explicitly unverified.

Use `all` on a stable source tree for full acceptance. When the user explicitly requests limited hotfix validation, `hotfix` produces its own report rather than an `all` or `fast` result. Its fixed command list and command logs are required by the release gate. It does not rerun SDK downloads/runtime acceptance, ARM64/QEMU or Windows/macOS cross compilation, native Windows/macOS execution, dependency security scans, or editor/website/credential-store integrations. The report records every one of these limits and remains bound to a clean commit, source fingerprint, Cargo lock and image digests.

Publication is a separate manual workflow that accepts the existing tag, exact compact `all` or authorized `hotfix` report and SHA-256. It performs metadata validation, compilation, packaging, checksums/SBOM/provenance and publishing, with no testing hooks. Release compilation does not expand the runtime coverage in the local report.
