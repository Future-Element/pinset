# Upgrade Pinset 2.x to 3.x

`scripts/migrate_v2_to_v3.py` is a standalone upgrade utility requiring Python 3.11+. It is not a Pinset command. Stop other Pinset operations during the upgrade. **No manual 3.x installation is needed**: it installs 3.x, migrates and verifies environments, switches command entries, then cleans up legacy files.

```sh
# Read-only preview; no downloads
python3 scripts/migrate_v2_to_v3.py

# Complete upgrade, including global selections and all registered 2.x projects
python3 scripts/migrate_v2_to_v3.py --apply

# Choose an exact official release; default recommendation: 3.0.3
python3 scripts/migrate_v2_to_v3.py --apply --version 3.0.3

# Include an unregistered project; --project is repeatable
python3 scripts/migrate_v2_to_v3.py --apply --project /path/to/project
```

`--registered` remains accepted; registered projects are always included. With no explicit project, the current directory's `pinset.toml` is also included. On Windows, use `py -3.11` or newer.

The upgrade proceeds in this order:

1. Validate legacy configurations, exact locks, caches and identities; back up sources and positively identified 2.x command entries.
2. Download the official GitHub Release CLI and matching shim, verify SHA-256, ZIP contents and CLI version, then install into `v3/bin`. Reuse an existing managed 3.x pair rather than downgrade it. Version detection uses `--version`, supported by both generations.
3. Translate configuration, locks, encrypted profiles and native identities, import verified caches, and install exact locked SDKs with 3.x. Selectors and exact versions are preserved without resolving `latest`, `lts` or channels again.
4. Run `check --deep`. Automatically back up existing venvs, inventory `pip freeze --all`, restore dependencies into the new environment, run `pip check`, and compare package inventories. Failure restores the original venv.
5. Switch identified old CLI/tool entries to 3.x. Touch only entries proven to belong to Pinset; preserve other executables. Remove old shim entries unavailable in 3.x. Failed activation restores original entries.
6. Remove migrated project `pinset.toml`, `pinset.lock`, old encrypted profiles and old download caches. Move old `state/`, `installs/` and `cache/` into recovery backups, removing their active legacy paths. Backups retain exact SDKs and historical state and are not deleted automatically.

Legacy native pnpm artifacts have a different installation layout. The script retains the exact locked version and original selector, asks the 3.x official resolver to verify signed metadata for that exact version, and generates compatible artifacts for the same locked platforms. Before/after records are saved in backup `pnpm-lock-conversions.json`. This conversion requires npm registry access; `--offline` fails before environment changes and asks for an online retry.

Backups live under `<3.x base>/migration-backups/v2-to-v3-<timestamp>-<id>/`, normally `~/.pinset/migration-backups/`. They include source bytes/hashes, executable files, Python package inventories, SDK/history recovery data and result reports. Old venvs remain beside projects as `.venv.pinset-v2-<id>`; restore archived SDKs to original paths before recovering them. Old SDK receipts are never relabeled as 3.x receipts.

| Option | Default directory |
| --- | --- |
| `--home`: legacy data | `PINSET_HOME` if set; otherwise `%LOCALAPPDATA%/Pinset` on Windows, or `$XDG_DATA_HOME/pinset` / `~/.local/share/pinset` on Unix/macOS |
| `--v3-home`: destination base | `PINSET_HOME` if set, otherwise `~/.pinset`; managed data uses its `v3/` child |

Relative bases, symlinked state paths and destination/backup directories inside legacy cleanup directories are rejected.

```sh
python3 scripts/migrate_v2_to_v3.py --home /old/pinset --v3-home /new/pinset --apply
```

Custom Unix destinations use forwarding scripts at old command locations that set the target `PINSET_HOME`. For custom Windows destinations, set `PINSET_HOME` to the new base before running and pass the old directory with `--home`. Use `--legacy-pinset /path/to/pinset` to identify a particular 2.x CLI; discovery otherwise checks PATH and legacy default binary locations. Shell profiles and `.gitignore` are not modified.

For offline upgrades or source builds, supply an existing 3.x executable with its matching adjacent `pinset-shim`:

```sh
python3 scripts/migrate_v2_to_v3.py --apply --offline --pinset /path/to/v3/pinset
```

`--offline` forbids CLI/SDK downloads; `--pinset` can be omitted when the destination already has 3.x. All SDK artifacts must be cached and verified. Python dependencies need local wheels/indexes, for example `PIP_FIND_LINKS`. Online upgrades may download missing locked artifacts and Python packages.

Encrypted profiles move to `.pinset/env/<name>.env`. Only the version envelope changes; age payloads remain identical and variables are never decrypted. Private keys are copied in memory through the native credential store, never written to files, arguments or reports. Old credentials remain for recovery. macOS uses Keychain, Windows Credential Manager, and Linux an unlocked Secret Service with `secret-tool`. Python requirements can contain private index URLs; save them only in restricted backups without printing pip output.

Project IDs, shared/local profile choices, Rust options and supported policies are retained. Workspace/global inheritance is materialized into independent configurations. Old trust is not imported: inspect configuration, then explicitly run `pinset env trust add` in each project. Access history, tasks, verification declarations, variable contracts and custom Python declarations are reported and retained in backups. Custom/named Python environment directories are not deleted.

Partial migration requires `--keep-v2`, preserving old files and command entries and skipping activation/cleanup:

```sh
python3 scripts/migrate_v2_to_v3.py --apply --keep-v2 --no-install
python3 scripts/migrate_v2_to_v3.py --apply --keep-v2 --no-global --project /path/to/project
```

`--no-install`, `--no-global`, `--skip-unsupported` and `--skip-identities` require `--keep-v2`. Skipped identities require a matching `PINSET_IDENTITY` later. Removed tools are rejected by default; explicit skips retain their legacy environment. Plaintext profiles, reserved-variable overrides, inconsistent locks and identity conflicts are rejected.

Existing 3.x configurations are never overwritten. A complete upgrade can continue only from this script's installed receipts with unchanged source hashes, then revalidate. Configurations without receipts or completed installation require `--keep-v2`. Ordinary failures allow retries: completed projects remain migrated; failed projects remove this invocation's new configuration and restore original venvs. Activation/cleanup occur only after all environments validate.

Hard termination can leave `.pinset/local/migration-v2-incomplete.json` or `<3.x base>/upgrade-v2-pending.json`; further cleanup is refused. Stop Pinset operations and use the marker, backup `manifest.json`, `binaries.json` and `upgrade-result.json` for recovery. Remove only generated configuration whose hashes match; preserve edits. Move failed new venvs aside and restore old environments. For activation/cleanup interruptions, restore directories from `legacy-home/`, restore files/modes/links from `binaries.json`, verify consistency, remove incomplete markers, then retry.

Commands work at their original PATH locations after upgrading. New shells can put `v3/bin` first using the 3.x `self shell` fragment. Commit `.pinset/config.toml`, `.pinset/lock.toml`, `.pinset/env/`; ignore `.pinset/local/`, `.venv/` and recovery directories. Docker `fast` covers complete upgrades, failure recovery and real 3.x offline installation/routing. Native credentials still require platform acceptance.
