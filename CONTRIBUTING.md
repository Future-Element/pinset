# Contributing

Pinset 3 has one protocol and one execution chain. Do not add migration paths, command aliases, task orchestration, Provider plugins or mirror settings.

Keep `pinset-core` read-only, with no networking, credential access or decryption. Provider and installation work belongs in `pinset-engine`; cryptography and credentials belong in `pinset-env`. The CLI delegates to services; the shim uses core routing and the private, version-matched CLI broker.

Use the local Docker verifier:

```powershell
./verify.ps1 -Suite fast
./verify.ps1 -Suite acceptance
./verify.ps1 -Suite platform
./verify.ps1 -Suite integrations
./verify.ps1 -Suite all
```

Unix uses `./verify.sh <suite>`. Never run project tests in CI. Packaging scripts and hooks may compile and package, but must not hide tests, scans, lint or independent typechecks. Changes require meaningful tests for contracts and failure paths, not assertions that merely repeat an implementation.

The verifier mounts source read-only, copies it into the container and records its initial fingerprint. Source changes during a run invalidate the report. SDK/dependency caches use isolated Docker volumes; projects, Pinset home, venvs, credentials and trust are fresh each run. Never mount host secrets or change host shell profiles.

Report native Linux, emulated ARM64, cross compilation, static contracts and observed editor behavior separately. Missing required suites fail `all`. Native Windows/macOS behavior cannot be claimed from Linux cross compilation.

Publication requires an existing v3 tag and a clean, matching local `all` report. The manual release workflow validates metadata and builds/packages/publishes only. Local development does not authorize a commit, tag, push or release.

Before publication, repository administrators must remove required branch-protection checks from the deleted test/preflight workflows. Those remote settings are outside a source-only implementation and are not changed by the verifier.
