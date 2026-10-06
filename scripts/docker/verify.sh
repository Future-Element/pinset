#!/usr/bin/env bash
set -euo pipefail
exec python3 /source/scripts/docker/runner.py "${1:-fast}"
