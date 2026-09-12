# Work directory environments / 工作目录环境

Development version 2.15.0. Public distribution defaults remain unchanged until the combined release.

Each worktree or workspace member uses its own checked-out `pinset.toml` and `pinset.lock`. Run `pinset setup --plan`, then `pinset setup` in an already-created directory. Pinset does not create branches or worktrees.

| State | Scope |
| --- | --- |
| Verified downloads and base SDK installations | Shared by exact version, installation options, archive identity and target |
| Local profile choice, trust, preparation/probe context and editor binding ownership | Canonical directory, directory generation and runtime host |
| Python venv | Member-local path with directory/host ownership marker |
| Flutter SDK/cache | Owned local copy under `PINSET_HOME/state/workspaces`; never hard-linked to another directory's mutable files |
| Project dependencies, build outputs and task files | Remain project-owned; Pinset does not link these between worktrees |

Directory identity includes the filesystem file ID and birth time, so editing project files retains the directory identity while deleting/recreating or moving the directory invalidates local state. Symbolic directory aliases resolve to one canonical identity. Filesystems without reliable birth-time support return an error rather than reusing unbound local state. Host identity stays local and is excluded from portable reports. This is a namespace boundary, not an OS sandbox or a promise to intercept arbitrary external tools.

Workspace members may inherit the root environment declaration and encrypted profile files. `pinset env use` selects a profile for that member only. Trust the member explicitly before injection; trusting the root does not trust every member. A member override owns its own encrypted files. `pinset env` and the environment report use the effective declaration; `configuration_origins` explains root defaults, member overrides and check scope. The editor Environment panel displays these origins. Editing an inherited encrypted profile edits its root-owned file, so that change is shared by inheriting members.

工作目录移动、重建或换宿主后，需要重新选择本机 profile、信任项目并准备环境。已有解密身份可以继续使用，但不会因此自动扩展信任范围。配置来源和检查范围可通过环境报告以及 VS Code Environment 面板查看。

## Existing local state and rollback

- Legacy profile records are never automatically adopted. Explicit `pinset env use <profile>` or `pinset env use --reset` preserves the old record as a versioned `.bak` file before creating/resetting the new choice.
- Legacy trust files remain untouched. Re-run `pinset trust add` only after reviewing the effective declaration in this directory.
- Legacy Python markers lack host/directory identity. `pinset venv recreate [name]` preserves the whole legacy directory in a sibling `.pinset-backup-*` directory before building a fresh venv. A marker belonging to another known directory/host is not permission to delete that venv.
- Old editor binding records remain in extension storage. New records include directory identity; automatic restoration only operates on matching records whose current setting still equals the recorded Pinset value. Review older settings manually.
- Old local state is retained for inspection. Pinset does not recursively sweep unknown state. SDK pruning rechecks registered project references; records from a different host block shared-SDK pruning until their references can be reviewed.

Returning to an older Pinset binary may require restoring that binary's backed-up local profile/venv state explicitly. Configuration/lock migration backups and application data are separate. Do not merge isolated mutable directories together as a rollback procedure.

## Validation boundary

Linux Docker unit and process tests cover directory replacement, aliases, foreign-host trust, independent member profiles and inherited ciphertext, legacy profile/venv preservation, local Flutter cache copies and shared SDK reference protection. Native Windows/macOS, SSH, WSL and editor execution evidence are tracked separately; these tests alone do not establish those entry points.
