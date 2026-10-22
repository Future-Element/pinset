# Manage custom CLI tools

Pinset can manage CLIs beyond its built-in Providers. Supply an exact version, a public HTTPS download rule, archive format and executable path, then use the same `use`, `install`, `which` and command routes.

[简体中文](generic-tools.zh-CN.md) · [Command reference](commands.md)

## Start with Terraform

Run `pinset init` for a new project. In an existing project, edit `.pinset/config.toml` directly to add a definition:

```toml
[generic.terraform]
url = "https://releases.hashicorp.com/terraform/{version}/terraform_{version}_{os}_{arch}.zip"
format = "zip"
executable = "terraform{exe}"
os = { macos = "darwin" }
arch = { x86_64 = "amd64", aarch64 = "arm64" }
```

Preview and select a version:

```sh
pinset use terraform@1.9.0 --plan
pinset use terraform@1.9.0
pinset which terraform --explain
pinset exec -- terraform version
```

`use` records the choice in `[tools]`. The resulting configuration includes both sections below. If `[tools]` exists, add a field to it instead of repeating the table header:

```toml
[tools]
terraform = "1.9.0"

[generic.terraform]
url = "https://releases.hashicorp.com/terraform/{version}/terraform_{version}_{os}_{arch}.zip"
format = "zip"
executable = "terraform{exe}"
os = { macos = "darwin" }
arch = { x86_64 = "amd64", aarch64 = "arm64" }
```

`1.9.0` is an illustrative exact version, not a latest-version recommendation. Initial selection downloads artifacts and computes real SHA-256 digests. Commit configuration and lock; others restore with `pinset install`. To generate the lock but defer installation, use `pinset use terraform@1.9.0 --no-install`.

## Fields and variables

| Field | Meaning |
| --- | --- |
| Tool name in `[tools]` | Public command name and selected version; the value remains a string. |
| `[generic.<name>].url` | Public HTTPS download template. |
| `format` | Required: `tar.gz`, `zip` or `binary`. |
| `executable` | Main executable's relative path inside the unpacked archive; raw binaries are saved at this path. |
| `os` | Optional map to the publisher's OS names. |
| `arch` | Optional map to the publisher's architecture names. |

Both `url` and `executable` support:

| Variable | Default or meaning |
| --- | --- |
| `{version}` | Exact version or tag, substituted verbatim. |
| `{os}` | `linux`, `macos`, `windows`; adjustable with `os`. |
| `{arch}` | `x86_64`, `aarch64`; adjustable with `arch`. |
| `{target}` | Pinset platform, such as `linux-x86_64`, unaffected by maps. |
| `{exe}` | `.exe` on Windows, empty elsewhere. |

Names must be portable lowercase ASCII, using letters, digits, `-` and `_`, starting with a letter or digit. Built-in tools/commands, `pinset`, `pinset-shim` and system-reserved names cannot be reused. The public command may differ from the archive filename.

Use `/` separators for relative executable paths; absolute paths and unsafe components such as `..` are rejected. Unknown template variables fail. For multiple platforms, declare top-level `platforms` before `use`; missing publisher artifacts fail resolution.

## Locking and installation

`use` renders platform templates, downloads artifacts and calculates digests. It writes actual URLs, SHA-256, format and executable paths to `.pinset/lock.toml`. Normal selection reuses downloads. `--plan` can still download temporary artifacts to compute genuine digests, but writes neither project nor managed Pinset state.

`install` uses locked URLs, digests, formats and paths only. It neither resolves versions nor renders templates again. A valid download-rule edit does not affect an existing lock's installation; the next explicit `use` updates that record. Changed version choices require matching regenerated locks. Cache reuse and fresh downloads both verify digests:

```sh
pinset install terraform --offline
pinset check --deep --probe --format json
```

Offline installation needs a valid local install or complete cache. Receipt/lock digest agreement and installed-payload inspection have distinct report fields; see [CI](ci.md#json-check-contract).

The first HTTPS download establishes the locked digest for repeatable content; it is not a publisher signature check. Generic tools have checksum verification strength and cannot satisfy signed-checksum policy. Without a release timestamp they also cannot satisfy a policy requiring a minimum release age. Generic tools do not bypass project policy.

## Command routing and scope

After shell integration, run `terraform version` directly; otherwise use `pinset exec -- terraform version`. Routing, project priority, global defaults and strict project policy follow built-in tool rules.

For global tools, define downloads in global configuration before `pinset use terraform@1.9.0 --global`. Project `remove terraform` deselects only; `clean installs` removes unreferenced payloads. Declared generic tools can also import explicit versions from source files, but `list --remote` does not discover their versions.

## Initial support

Supports precompiled native programs at public HTTPS URLs, including GitHub Releases and HTTPS-preserving CDN redirects. Installation checks executable OS/architecture without running archive contents to install them.

Source builds, script installers, installation hooks, private-repository authentication, floating versions and range discovery are unsupported. HTTP downgrades, URL credentials/fragments, local addresses, unsafe paths and scripts as main executables are rejected.

For `PINSET_GENERIC_INVALID`, check names, URLs, variables, formats, relative paths and explicit versions. For download errors, check the rendered target URL exists. Stop on digest errors and review upstream content and the agreed lock; never remove digests to bypass verification.

