# Security

## Supported versions

Security fixes are applied on [`main`](https://github.com/Future-Element/pinset) and shipped in the [latest release](https://github.com/Future-Element/pinset/releases/latest). Use that release. Older tags are not supported.

## Reporting a vulnerability

Do not open a public GitHub issue, discussion, or pull request for an unfixed vulnerability.

Report it privately with [GitHub private vulnerability reporting](https://github.com/Future-Element/pinset/security/advisories/new) (open a draft Security Advisory on this repository). Private vulnerability reporting is enabled. There is no separate security email.

Include the affected component and version, how to reproduce the issue, and the impact you expect. Do not include decrypted profile values, identity keys, or host credential-store contents in the report, in issues, or in logs.

## Response

We acknowledge reports on a best-effort basis within 7 days. A fix and any public advisory follow after the report is confirmed. This is not a contracted response time.

## Scope

**In scope:** Pinset’s own code and distribution. That includes the CLI and engine, installer and uninstall scripts, verification scripts, packaging, and the [project website](https://pinset.future-element.com). How Pinset resolves, downloads, verifies, stages, and installs an artifact is in scope.

**Out of scope:** Vulnerabilities in third-party tools and runtimes that Pinset installs (Node.js, pnpm, Bun, Go, Python, Temurin JDK, Rust, Flutter/Dart, and generic binaries declared by a project). Those belong to the upstream projects. Report them there.

## Security properties

Pinset 3 validates official artifact digests, rejects unsafe archive paths and installs through staging and atomic commits. Its receipt records the actual verification method; a checksum does not imply a verified signature. Offline cache use still validates the digest.

Encrypted profile values use age. Private identities live in the OS credential store or explicit `PINSET_IDENTITY`, never a project key file. Trust binds the project ID, directory identity and configuration/profile fingerprint. Child processes do not inherit the private identity. Reports/history contain public metadata and verification results, not decrypted values.

`check` is read-only by default. Explicit probes and execution run native tools and project commands; they are not a sandbox. Build wrappers, toolchains, IDEs and native tools can use external state or the network. Candidate copies do not isolate databases or reverse external effects.

An interrupted transaction blocks routing until recovery. Cleanup retains uncertain references and never adopts an external venv or installation. Old-version data remains outside the v3 ownership boundary.

Security scans execute locally through Docker `verify.ps1 -Suite fast` and `-Suite integrations`. Publication CI consumes the local report and does not perform scans or tests.

The local scanner records RUSTSEC-2023-0071 for transitive `rsa 0.9.10`. Its private-key timing attack is outside Pinset’s public-only Node signature verifier; a version- and source-scoped applicability exception expires on 2026-12-31. The finding remains visible in the report. Any new PGP/RSA use, version or advisory fails the gate and requires review. [RustSec advisory](https://rustsec.org/advisories/RUSTSEC-2023-0071.html).
