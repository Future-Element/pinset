# CI with setup-pinset

Install Pinset in GitHub Actions, restore the committed toolchain lock, and fail the job if installation or verification fails. The repository-root `setup-pinset` Action is available from v3.0.9; examples pin v3.1.0. The `version` input defaults to the latest stable Pinset 3 release.

[简体中文](ci.zh-CN.md) · [Command reference](commands.md)

```yaml
name: Build
on: [push, pull_request]
jobs:
  build:
    strategy:
      matrix:
        os: [ubuntu-24.04, macos-15, windows-2025]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: Future-Element/pinset@v3.1.0
        id: pinset
        # version defaults to latest; set an exact release for reproducible CI.
      - run: pinset exec -- node --version
```

Commit `.pinset/config.toml` and `.pinset/lock.toml` before running the workflow. The lock must contain artifacts for each runner platform. Pinset binaries are available for Linux x64/ARM64, macOS ARM64 and Windows x64; macOS Intel and Windows ARM64 runners are unsupported. The Action uses the runner's native installer and adds the CLI and `PINSET_HOME/v3/bin` to PATH.

## Inputs and outputs

| Input | Default | Meaning |
| --- | --- | --- |
| `version` | `latest` | Latest stable Pinset 3 release, or an exact `3.x.y`/`v3.x.y` release (including supported `-rc.N` releases). |
| `install` | `true` | Restore project tools and run checks. `false` installs only Pinset and skips project cache/restore/check steps. |
| `cache` | `true` | Cache and restore the tool installation directory. |
| `skip-check` | `false` | Skip the post-install check; locked installation and its download digest verification still run. |
| `working-directory` | `.` | Project directory, or a directory inside its Pinset project boundary. |
| `token` | `${{ github.token }}` | GitHub token used to read release metadata. |

| Output | Meaning |
| --- | --- |
| `version` | Resolved exact Pinset version, without a leading `v`. |
| `cache-hit` | Whether an exact installation cache key was restored (`true` or `false`); `false` when the cache step is skipped. |
| `check-report` | Absolute path of the JSON check report in the runner's temporary directory; empty when checking is skipped. |

Boolean inputs accept `true` or `false`. Installation, lock restoration and requested checks each fail the workflow on an error. Skipping checks is explicit:

```yaml
- uses: Future-Element/pinset@v3.1.0
  with:
    version: '3.1.0'
    working-directory: packages/api
    skip-check: 'true'
```

## Locked restoration and caching

The Action resolves and installs the requested Pinset release, restores cached installations, then always runs `pinset install`, including on a cache hit. `install` reads exact artifact URLs and digests from the existing lock; it does not resolve newer tool versions. It restores project bindings, managed command entries and any project Python environment required by the lock.

Only `PINSET_HOME/v3/installs` is cached. The key includes runner OS/architecture/image, the exact Pinset version, the absolute Pinset home and the lock digest. A different lock gets a different key. Cache reuse does not skip receipt validation or the requested check. Set `PINSET_HOME` in the job's `env` to use a custom home consistently across steps.

Project `.pinset/local`, `.venv`, environment identities, trust and encrypted profile state are outside this cache. Project package dependencies remain the responsibility of your build steps. The cache Action requires a self-hosted runner version compatible with `actions/cache@v5` (2.327.1 or later).

By default, setup runs `pinset check --deep --probe --format json`, prints the report, and exposes its path as `check-report`. `--deep` recomputes the installed payload fingerprint and compares it with the verified installation receipt. `--probe` starts bounded version probes and records their evidence. These checks do not run your application build.

Archive the report even when a tool check fails:

```yaml
- uses: Future-Element/pinset@v3.1.0
  id: pinset
- uses: actions/upload-artifact@v4
  if: ${{ always() && steps.pinset.outputs.check-report != '' }}
  with:
    name: pinset-check-${{ runner.os }}-${{ runner.arch }}
    path: ${{ steps.pinset.outputs.check-report }}
```

## JSON check contract

`pinset check --format json` and `pinset check --json` produce the same envelope. Text remains the default. The `protocol` is `pinset/3`; `schema_version` is `1`. Existing fields keep their names, types and meanings; future revisions may add fields, so consumers must tolerate unknown fields.

The following excerpt illustrates one successful tool with deep inspection and a version probe. Other existing fields, including observation evidence, are omitted:

```json
{
  "protocol": "pinset/3",
  "schema_version": 1,
  "passed": true,
  "report": {
    "protocol": "pinset/3",
    "project": "/workspace/example",
    "checks": [
      {
        "tool": "node",
        "expected_version": "24.0.0",
        "actual_version": "24.0.0",
        "artifact_digest": {
          "expected": "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
          "receipt_matches_lock": true,
          "payload_verified": true,
          "passed": true
        },
        "passed": true,
        "configured": true,
        "installed": true,
        "bound": true,
        "actually_verified": true,
        "detail": "exact installation receipt matches"
      }
    ]
  }
}
```

| Field | Type and meaning |
| --- | --- |
| `passed` | Boolean. Every selected tool passes the checks requested for this invocation. |
| `report.checks[].tool` | String. Selected tool name. |
| `expected_version` | String. Exact version in the lock. |
| `actual_version` | String or `null`. Parsed version observed by an explicit `--probe`, when available. |
| `artifact_digest.expected` | String or `null`. Canonical locked digest for the host artifact, such as `sha256:<hex>` or SHA-512 SRI. |
| `artifact_digest.receipt_matches_lock` | Boolean. An installation receipt is valid and agrees with the exact lock, including artifact digests. A missing/invalid receipt is `false`. |
| `artifact_digest.payload_verified` | Boolean or `null`. Installed payload matches its receipt when `--deep` can inspect it; `null` means it was not inspected. |
| `artifact_digest.passed` | Boolean. The requested receipt and payload integrity checks pass. |
| `source_verification` | Present for generic tools. `status` is `signature-verified`, `digest-only`, or `invalid`; `method`, `repository` and `passed` describe the locked source evidence. |
| `report.checks[].passed` | Boolean. The tool is installed and bound, and its requested deep inspection/version probe passes. |

`expected_version` preserves the locked version; `actual_version` is parsed from the probe. Generic tag `v` prefixes, Python build markers and Rust nightly identities may be normalized by their tool-specific rules. Read `passed` and `actually_verified` instead of requiring the two version strings to be byte-for-byte identical.

Without `--probe`, `actual_version` is `null` and `actually_verified` is `false`. Without `--deep`, `payload_verified` is `null`. A plain check remains read-only: it does not start tools, access the network or write evidence. `--probe` explicitly writes observation evidence under the Pinset home.

Downloaded artifacts are verified against the lock during installation. The check report distinguishes receipt/lock agreement from installed payload verification; it does not claim to rehash an original archive that may no longer be present. Java wrapper and Android observations remain separate evidence and do not certify a project build.

For attested generic GitHub Releases, plain `check` also cryptographically verifies the locked bundle offline. `digest-only` means only the first downloaded bytes were pinned, with no publisher signature verified. Invalid source evidence fails the tool check and prevents its version probe. See [custom CLI tools](generic-tools.md) for scope and policy behavior.

A tool check failure still prints the complete report and exits 1. An operational error, such as a missing lock, exits 1 with the same protocol/schema version and `passed: false`, plus an `error` object containing `code` and `message`. Redirect stdout to archive a standalone report:

```sh
pinset check --deep --probe --format json > pinset-check.json
```


