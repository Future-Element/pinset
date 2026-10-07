#!/usr/bin/env bash
set -euo pipefail
# Protect the shared workspace, credentials and binary snapshots from concurrent runs.
exec flock --nonblock --conflict-exit-code 75 /run/pinset-verify.lock python3 /source/scripts/docker/runner.py "${1:-fast}"
