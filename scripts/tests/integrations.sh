#!/usr/bin/env bash
set -euo pipefail
export XDG_RUNTIME_DIR="$(mktemp -d /tmp/pinset-runtime-XXXXXX)"
chmod 700 "$XDG_RUNTIME_DIR"
export XDG_DATA_HOME="$(mktemp -d /tmp/pinset-keyring-XXXXXX)"
eval "$(printf 'ephemeral-local-test-keyring' | gnome-keyring-daemon --unlock --components=secrets)"
xvfb-run -a python3 scripts/tests/integrations.py "$@"
