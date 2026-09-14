# Candidate validation / 候选升级验证

Pinset 2.15 requires an explicit validation task list in schema 6. Migrate existing configuration with `pinset migrate --dry-run`, then `pinset migrate` before adding this declaration. Every dependency task must also be listed. No task runs during `candidate prepare` or `candidate apply --plan`.

```toml
[verification]
tasks = ["check", "test"]
inputs = ["fixtures/ignored-input.json"]
external-state = false
timeout-seconds = 300

[tasks.check]
command = ["node", "scripts/check.mjs"]

[tasks.test]
command = ["node", "--test"]
depends-on = ["check"]
```

```sh
pinset candidate prepare
pinset candidate test test --compare
pinset candidate apply --plan
pinset candidate apply
pinset candidate history
pinset candidate restore
```

`--compare` runs the current lock and candidate lock in separate temporary directories copied from the same captured input manifest. Without it, only the candidate is tested. Both SDK sets must be installed. Dependencies are not copied from the original project; an explicitly listed setup task can install dependencies into the snapshot. Python environments, Flutter copies and common package/build caches are isolated per snapshot. Task output is streamed, not saved in history. Temporary copies are removed after the process tree finishes. The timeout is per task (1–3600 seconds); timeout exits 124 and cancellation exits 130.

The manifest includes Git tracked and untracked non-ignored regular files, executable flags and content SHA-256 values. It detects changes even when the worktree stays dirty. Explicit `inputs` add required ignored files/directories. Outside Git a bounded directory inventory is used and reported as limited evidence. Limits are 20,000 files, 16 MiB per file, 256 MiB total, depth 64 and 64 explicit input paths. Symlinks, special files and paths outside the project are rejected. `.git`, Pinset state, managed Python environments and generated dependency/cache directories are omitted. `.env`, `.env.*`, `pinset.env` and `.age` files are excluded; encrypted profiles are injected from the original project's trusted declaration.

Application rechecks the exact candidate lock, configuration and inherited defaults, input content, task/profile context, platform and directory generation/host while holding project state locks. A failed, interrupted, concurrent or superseded run cannot reuse a previous pass. Secret values and additional argument values or hashes are never stored; arguments are redacted. Secret context, external state, unmanaged commands, excluded sensitive files or fallback inventory produce limited evidence. Inspect it using `candidate status --json` or `candidate apply --plan`; `--allow-limited` explicitly accepts these limits but never overrides stale input or failed-test checks.

Snapshots are **not an operating-system sandbox**. Declared commands retain the user's OS permissions and can access absolute paths or networks. Set `external-state = true` for databases/services or other external dependencies. Only list tasks you intend to run for validation, including their dependency chain; deployment, publication and database migration tasks are not selected automatically. OS-level forced termination or a host crash can leave a temporary directory for manual cleanup.

Apply/restore changes exact `pinset.lock` selections only. History does not restore source, installed package dependencies, databases, services, editor settings or existing Python environments. Review the restored lock, then run `install --locked` / `venv recreate` as needed. A workspace apply journals all member locks and preserves recovery data on conflicts. Version 1 candidate/history files remain readable but cannot authorize applying or restoring on a directory whose generation/host is unproven. Prepare and test a fresh candidate. Legacy recovery journals remain preserved for manual review.

2.15 的候选验证需要 schema 6 的 `[verification]` 显式列出任务及全部前置任务。`--compare` 将同一份输入分别复制到两个临时目录，运行当前与候选 SDK；项目依赖需由白名单内的准备任务安装，原目录中的依赖、虚拟环境和缓存不会直接复用。应用前重新核对文件内容，因此“原本已修改，又继续修改”的情况也会使旧结果失效。

额外输入通过 `inputs` 声明，边界和上限与英文说明一致。密钥值不参与持久化或指纹，额外命令参数只保留脱敏占位；涉及这些上下文、外部服务或系统命令时显示验证范围受限，需要查看预览后显式使用 `--allow-limited`。这个参数不能跳过内容变更或失败结果。临时目录提供数据隔离，不限制任务的系统权限。恢复仅回退 Pinset 锁定的 SDK 选择，不能当作源码、依赖、数据库或编辑器状态的完整回滚。
