# Minimal polyglot template

Three standard-library-only programs use Node.js, Python and Go from one Pinset project. There are no application packages to install. This directory can be used in place or copied into a new Git repository.

Get Pinset from the [website](https://pinset.future-element.com) or [main repository](https://github.com/Future-Element/pinset). Installation does not modify shell profiles. `pinset self shell <bash|zsh|fish|powershell>` only prints a PATH fragment; follow the [shell setup guide](https://pinset.future-element.com/en/docs/commands/self-shell) to review and apply it yourself. No shell setup is run by this template.

## Create a standalone repository

From the root of a Pinset checkout, choose an empty destination:

```sh
cp -R examples/polyglot ../pinset-polyglot
cd ../pinset-polyglot
git init
```

## Resolve once, then share the lock

Run from this directory after setting up PATH for the current shell:

```sh
pinset use node@24 python@3.14 go@1.25 --no-install
pinset install
pinset check
pinset which node --explain
pinset which python --explain
pinset which go --explain
```

The supplied `.pinset/config.toml` declares version selectors. It is deliberately not an exact lock: the first `use --no-install` reads official metadata and generates `.pinset/lock.toml` with exact versions, platform artifacts and digests, without downloading SDK payloads. `install` explicitly downloads and installs the locked SDKs and creates the project Python `.venv`. Both commands need network access on a cold cache.

Review and commit the generated `.pinset/lock.toml` together with `.pinset/config.toml`. Teammates clone that repository and run `pinset install`, then the commands below; they do not repeat `use`. Each required platform needs supported artifacts in the lock. Running `use` again is an explicit re-resolution and may select newer patch versions.

Ignore `.pinset/local/`, `.venv/` and Python bytecode. Keep `.pinset/config.toml` and `.pinset/lock.toml` in Git. The template has no encrypted profiles or private values.

## Run and inspect

```sh
pinset exec -- node main.mjs
pinset exec -- python main.py
pinset exec -- go run main.go
```

Expect one line per program, beginning with `PINSET_NODE_OK`, `PINSET_PYTHON_OK` and `PINSET_GO_OK`, followed by that runtime's actual version. Compare those versions and the `which --explain` paths with the generated lock. The programs use only bundled runtime libraries; `go run` compiles the single Go source file without third-party modules. `exec` does not install missing SDKs.

For a fresh-machine reproduction, clone your standalone repository, run `pinset install`, then repeat these three `exec` commands. A missing lock is resolved with `use` by the template maintainer, rather than silently during execution.
