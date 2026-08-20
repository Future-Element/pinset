# Integrations and distribution

Pinset integrations always select an exact Pinset release and verify the downloaded archive. They do not use an unverified `latest` redirect.

## GitHub Actions

The repository root contains a composite action for Linux x64/ARM64, Windows x64, and macOS ARM64. It verifies the release archive against `SHA256SUMS`, adds both Pinset binaries to `PATH`, and installs the project's locked runtimes by default.

```yaml
steps:
  - uses: actions/checkout@v4
  - uses: Future-Element/pinset@v1.9.0
    with:
      version: 1.9.0
      install: "true"
  - run: pinset lock audit
  - run: pinset exec -- node --version
```

Set `install: "false"` when the job only needs the Pinset CLI. `working-directory` selects the directory passed to `pinset install --locked`.

## Renovate

Merge [`integrations/renovate/pinset.json5`](../integrations/renovate/pinset.json5) into the repository's Renovate configuration, then annotate each managed selector immediately above its TOML key:

```toml
[tools]
# renovate: datasource=node-version depName=node
node = "24.0.0"
# renovate: datasource=npm depName=pnpm
pnpm = "11.0.0"
```

The explicit datasource is intentional: Pinset Providers consume different upstream version systems, so a single guessed datasource would produce incorrect updates. Renovate changes `pinset.toml`; run `pinset update` in the update workflow to refresh and verify `pinset.lock`.

## VS Code schemas

[`schemas/pinset.schema.json`](../schemas/pinset.schema.json) and [`schemas/pinset-lock.schema.json`](../schemas/pinset-lock.schema.json) describe schema 3 configuration and lockfiles. They are JSON Schema documents, while the files they describe use TOML syntax. VS Code's built-in JSON schema association applies only to JSON files, so install a TOML language extension that supports JSON Schema and associate:

- `**/pinset.toml` with `https://raw.githubusercontent.com/Future-Element/pinset/v1.9.0/schemas/pinset.schema.json`
- `**/pinset.lock` with `https://raw.githubusercontent.com/Future-Element/pinset/v1.9.0/schemas/pinset-lock.schema.json`

Use the tag-pinned URLs for reproducible completion. `pinset.lock` is generated state and should still be updated through Pinset, not hand-edited.

## Dev Containers

Copy [`examples/devcontainer/.devcontainer`](../examples/devcontainer/.devcontainer) into a Pinset project. Its Dockerfile downloads an architecture-specific v1.9.0 archive, verifies `SHA256SUMS`, and installs `pinset` plus `pinset-shim`; `postCreateCommand` then runs `pinset install --locked` after the workspace is mounted.

## Winget, Scoop, and Homebrew

Each GitHub Release includes:

- `pinset-winget.yaml`
- `pinset-scoop.json`
- `pinset.rb`

The release workflow generates these files from the same archives it publishes, appends their hashes to `SHA256SUMS`, and creates GitHub artifact attestations. The manifests can be installed directly or used as the source for an upstream catalog submission. Publishing changes to community-owned package indexes is a separate maintainer action from producing the official Pinset manifests.
