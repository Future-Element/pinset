#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$root/output"
docker build -f "$root/scripts/docker/Dockerfile.verify" -t pinset-verify:3.0 "$root"
digest="$(docker image inspect pinset-verify:3.0 --format '{{.Id}}')"
docker run --rm --init --env "PINSET_VERIFY_IMAGE_DIGEST=$digest" --mount "type=bind,source=$root,target=/source,readonly" --mount type=volume,source=pinset-v3-cargo,target=/usr/local/cargo/registry --mount type=volume,source=pinset-v3-target,target=/build/target --mount type=volume,source=pinset-v3-sdk-cache,target=/sdk-cache --mount type=volume,source=pinset-v3-sdk-cache,target=/run/pinset/v3/cache --mount "type=bind,source=$root/output,target=/reports" pinset-verify:3.0 bash /source/scripts/docker/verify.sh "${1:-fast}"
