# Use Pinset with AI coding agents

Let a coding agent read stable JSON, install the exact locked tools and check the resulting toolchain. The agent uses fields and exit codes to decide its next action and stops dependent work when installation or verification fails.

[简体中文](agents.zh-CN.md) · [Command reference](commands.md) · [CI and check reports](ci.md)

## Complete a migration

Declare the required tools first, such as `node`, then execute each command and parse its stdout separately:

```sh
pinset -C /path/to/project import --plan --json
pinset -C /path/to/project import --yes --json
pinset -C /path/to/project install --json
pinset -C /path/to/project check --deep --probe --json
```

`import --plan` may resolve version metadata online. It shows a plan without writing the project or installing tools. `import --json` without `--yes` also returns only a plan. Applying requires explicit `--yes`; that invocation generates and applies a new plan, so inspect its exact versions and skipped entries again.

| Step | Conditions before continuing |
| --- | --- |
| Plan | The union of `imports` and `retained` contains every required tool. Record each `version` and inspect `unsupported`, `conflicts` and `unresolved`. |
| Apply | `plan` is `false`. Check tools, exact versions and skipped entries again. `applied: false` can mean there were no changes left to write. |
| Install | `installed` is `true`. `tools` includes the required names; `installed_tools` provides exact `version`, `provider`, `target` and `install_identity` values. |
| Check | `passed` is `true`, `report.checks` contains every required tool and each entry has `passed: true`. With `--probe`, also check `actual_version`. |

Stop and report skipped entries, or handle them individually under existing authorization. Import can report skipped entries and still exit `0`. Checking an empty tool set can also return `passed: true`. Required tools being present is therefore part of acceptance; an empty array does not establish that the toolchain is ready.

A project with a committed `.pinset/lock.toml` normally starts at `install --json`. Installation uses exact artifacts and digests from that lock without selecting versions again. Ordinary `which`, `list` and `check` requests do not install missing tools for the agent.

## Parse the protocol and exit status

Management commands with `--json` write one JSON object to stdout. Keep stdout and stderr separate. These success reports, operational errors and argument errors all contain:

```json
{"protocol":"pinset/3","schema_version":1}
```

Existing field names, types and meanings remain stable; changes only add fields. Consumers must ignore unknown fields. Object key order is not part of the contract. `null` means a value was not observed or does not apply; it does not mean success, an empty string or `false`.

| Exit | Agent action |
| --- | --- |
| `0` | The command completed. Still inspect required tools, skipped entries and status fields. |
| `1` | An operation or tool check failed. Stop dependent steps and inspect `error.code` or the complete check report. |
| `2` | Arguments were invalid. Correct the request identified by `error.code: PINSET_ARGUMENT_INVALID`. |

Operational errors contain `error.code` and `error.message`. Branch on the stable code and use the message to explain the problem to the user. Do not match terminal prose. Failed tool checks may omit `error` and instead preserve a complete `report.checks` with `passed: false`. Stop on malformed JSON or an unexpected protocol as well.

`import --yes --json` writes only the final result to stdout and a separate JSON plan to stderr. That plan is available for review; never append it to stdout. Other JSON management commands disable interactive progress output.

## Query state

```sh
pinset list --json
pinset which node --json
pinset which --json
pinset env list --json
pinset env list --profile dev --json
```

| Query | Fields to read |
| --- | --- |
| `list` | `selected` is the selected lock or `null`. `installs` is the local inventory sorted by tool, version, identity, platform and path. |
| `which node` | A single route object with `executable`, `sdk`, `version`, `source` and `provider`. |
| `which` | A collection of route objects under `commands`. |
| `env list` | `profiles`, `shared_default` and `local_default`. |
| `env list --profile dev` | `profile` and a `variables` array of names. Variable values are not returned. |

By default, `check` inspects configuration, bindings and installation receipts. For generic tools, `source_verification` also distinguishes `signature-verified` from `digest-only` and revalidates locked GitHub evidence offline. `--deep` verifies installed payload fingerprints; `--probe` explicitly runs version probes. Without a probe, `actual_version` is `null`; without payload inspection, `payload_verified` is `null`. These reports do not establish that the application builds successfully.

## Native and interactive output exceptions

`exec` preserves application output and status. It explicitly rejects `--json` with the structured error `PINSET_NATIVE_OUTPUT` and exit `1`. Prepare the environment through JSON queries and checks, then run the application separately:

```sh
pinset exec --no-env -- node --version
```

This command returns child output and child status, which can fall outside `0/1/2`. Environment preparation failures still exit `1`.

`env access request --ci` displays an identity private key once in an interactive terminal. It rejects `--json` with `PINSET_CI_IDENTITY_TTY`. Automated `env set` calls should use `--stdin` to provide values without a password prompt. Keep secret values out of command arguments and logs.

`self shell --json` and `self completions --json` wrap generated scripts in `text`. `--help` and `--version` retain text output and are outside the JSON report interface. Use `self info --json` when an agent needs to read the Pinset version.
