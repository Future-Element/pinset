# Pinset 3.0 implementation ledger

This branch implements the breaking Pinset 3.0 design. Publication is separate from implementation and local acceptance.

Implemented scope:

- Five crates with pure core models/routing, one engine installer/cache/transaction path, encrypted environment services, thin CLI and a paired read-only shim/private broker.
- Eight compiled-in Providers, twelve public command families, strict `pinset/3` models, `.pinset/config.toml` and `lock.toml`, v3 home and bounded project discovery.
- Standard-library root `.venv` ownership/package isolation without uv; Rust channel/date/profile/components/targets; complete untrimmed Temurin JDK inventory and Java runtime/compiler/target evidence separation.
- Read-only checks, explicit probes, conservative reference-aware cleanup, locked installation, safe repair and concurrent state guards.
- Age profiles, real OS credential-store integration, authorization, fingerprint-bound trust and child-process secret exclusion.
- Independent baseline/candidate snapshots, latest-result and stale-fingerprint gating, limited-evidence rules, apply/restore and interrupted transaction recovery.
- Slim VS Code integration, installation-only GitHub Action, installers, bilingual command documentation, schemas, examples and the existing website stack.
- Local Docker verification and metadata-bound reports; CI performs publication only. Legacy execution paths, compatibility models, task/workspace/dotnet/plugin/source/bundle surfaces and their tests/docs are removed.

Acceptance is determined by the report from `verify.ps1 -Suite all` on the exact source fingerprint, not by this ledger. The report binds every required suite, executed command, evidence digest, exact SDK selection and security applicability exception. Focused development runs do not replace a final all-suite report. A dirty source tree cannot authorize publication.

The user explicitly excluded download tests for large artifacts such as Flutter. Flutter SDK execution and Android SDK/APK builds remain exempt and unverified; metadata, locks, routing and deterministic Android evidence contracts remain required. The report and release gate preserve this scope. Windows/macOS native execution remains unverified; Linux ARM64 execution is labeled as QEMU emulation. GUI startup does not claim interactive GUI acceptance.
