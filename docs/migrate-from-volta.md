# Migrate a Volta project to Pinset

Import the Node and pnpm versions declared in your project's `package.json`, review the exact lock, and verify Pinset's commands before changing your shell. This guide also explains the npm pin that Pinset cannot preserve independently.

[简体中文](migrate-from-volta.zh-CN.md) · [Command reference](commands.md) · [Core concepts](concepts.md)

Volta's maintainers [state that it is unmaintained](https://github.com/volta-cli/volta#readme). They also say that working installations do not require an urgent migration. Plan the change around your project's own requirements; the instructions below leave Volta installed for comparison and rollback.

## What maps to Pinset

Run the import at the project root. For example, an existing `package.json` might contain:

```json
{
  "volta": {
    "node": "24.0.0",
    "npm": "11.4.2",
    "pnpm": "10.15.0"
  }
}
```

These versions illustrate existing declarations, rather than recommending versions for a new project.

| Volta declaration | Pinset result |
| --- | --- |
| `volta.node` | An exact Node selection in `.pinset/config.toml` and its artifact records in `.pinset/lock.toml`. |
| `volta.pnpm` | An exact pnpm selection and lock. The project must also select Node; import both declarations together. |
| `volta.npm` | An `unsupported` entry in the import report. Pinset has no independent npm Provider or `[tools].npm` selection. |
| `volta.extends` | Ignored. Pinset does not follow the referenced package file or import its inherited versions. |

Pinset routes `npm` and `npx` to the versions bundled with the selected Node distribution. The npm version may differ from `volta.npm`, and a passing `pinset check` does not establish that the old npm pin was preserved. If your project requires that exact independent npm version, resolve that requirement before switching the project to Pinset. `pinset use npm@11.4.2` is not a supported substitute.

Volta's [pnpm support](https://docs.volta.sh/advanced/pnpm) requires its experimental feature flag. An actual `volta.pnpm` declaration can be imported; a pnpm installation that exists only in Volta's machine settings is outside this import. Volta's global defaults, installed package binaries, hooks and cached downloads are also not migrated.

## Save the current working setup

Keep Volta installed and preserve `package.json` and the other version files. Save any existing `.pinset/` directory and `.gitignore`, along with the shell startup configuration and PATH order. Import retains source files, but creating a Pinset project also adds its local-state ignore rules.

Record the versions and paths that work with Volta before changing command precedence:

```sh
volta list
volta which node
volta which npm
node --version
npm --version
```

If the project uses pnpm, also record `volta which pnpm` and `pnpm --version`. Save the current shell's PATH for the later rollback step:

```bash
volta_migration_path=$PATH
```

In PowerShell, use:

```powershell
$voltaMigrationPath = $env:PATH
```

Install the Pinset CLI using the website's installation instructions. Keep the CLI accessible, but defer adding `pinset self shell` output to your shell startup configuration until the comparisons below pass. `pinset self info` reports the CLI path and Pinset home; inspect `pinset self shell` output for the managed command directory.

## Review and import the declarations

Run each command separately and stop on an error. There is no need to run `pinset init` first:

```sh
pinset import --plan
```

Inspect `imports`, `retained`, `conflicts`, `unresolved` and `unsupported` in the plan. `pinset import --plan --json` exposes the same information for scripts. A plan can exit `0` while skipping entries, so the exit code alone does not mean that every declaration was imported.

Check that Node and pnpm have the intended exact versions. For the example above, npm appears in `unsupported`; accept its replacement by Node's bundled npm only after comparing the actual version and your project's requirements. Investigate every other skipped item before proceeding.

The importer also reads `engines`, `packageManager`, `.nvmrc`, `.node-version` and other supported root version files. Their compatible declarations jointly constrain a tool's version; there is no rule that makes a Volta pin override contradictory sources. Existing Pinset selections are retained rather than replaced. Resolve conflicting declarations or deliberately change an existing selection with `pinset use`, then review a fresh import plan.

For a project that uses `volta.extends`, identify the effective versions in each affected package first. Import does not traverse those references or recursively discover package manifests. Explicitly select the effective versions for each intended Pinset project; see [project boundaries](concepts.md#project-and-global-scope). An empty conflict list does not prove that inherited Volta settings were imported.

When the plan is complete and the npm boundary is acceptable:

```sh
pinset import
pinset install
pinset check
```

`import` presents a plan for confirmation and writes the configuration and exact lock; it does not install the tools. Review that displayed plan before confirming. `install` restores the locked artifacts. `check` exits `1` if a selected tool fails its requested checks. Keep both `.pinset/config.toml` and `.pinset/lock.toml` for version control.

## Verify before switching PATH

Inspect Pinset's routes and run its selected tools explicitly while Volta remains available:

```sh
pinset which node --explain
pinset which npm --explain
pinset which pnpm --explain
pinset exec -- node --version
pinset exec -- npm --version
pinset exec -- pnpm --version
```

Omit the pnpm commands if the project does not select pnpm. Compare these results with your saved versions and run the project's usual checks through `pinset exec -- <command>`. A Pinset integrity check does not replace those application checks or an explicit npm version comparison.

After the results meet the project's requirements, enable the managed command directory for the current shell. In Bash:

```bash
pinset_shell=$(pinset self shell bash) && eval "$pinset_shell"
hash -r
command -v node npm pnpm
```

In PowerShell:

```powershell
$pinsetShell = pinset self shell powershell
if ($LASTEXITCODE -ne 0) { throw "Pinset could not generate shell integration." }
$pinsetShell | Out-String | Invoke-Expression
Get-Command node, npm, pnpm
```

Again, omit pnpm from the lookup if unused. Confirm that bare `node`, `npm` and, if applicable, `pnpm` now resolve through Pinset and report the expected versions. Inspect earlier aliases or functions if command lookup disagrees. Only then save the matching shell snippet in your startup configuration. `self shell` only prints the snippet; it does not edit that file.

Keep the original Volta fields until the team and CI have switched and the rollback period has ended. Pinset requires its own configuration and lock during normal execution; it does not use Volta settings as a runtime fallback. For CI, see [setup-pinset](ci.md).

## Roll back

Restore the PATH captured in the same shell:

```bash
export PATH="$volta_migration_path"
hash -r
```

Or, in PowerShell:

```powershell
$env:PATH = $voltaMigrationPath
```

If you saved Pinset integration in your shell startup file, restore that file's previous PATH order and open a fresh shell. Confirm command paths with `volta which node`, shell command lookup and the saved version outputs.

Restore only the project files changed for this migration from their saved copies, including any edited version declarations, `.pinset/` state and `.gitignore` rules. If `.pinset/` already existed, restore its previous contents rather than deleting it. If migration created it, remove only that new project state after checking that it contains no work you need to keep. Pinset's shared installation/cache directory can remain; it does not need to be deleted to run Volta again.

Do not uninstall Volta or remove its version declarations until the project, collaborators and CI have accepted the replacement behavior, including npm.
