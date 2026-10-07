#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$root/output"
image=pinset-verify:3.0
container=pinset-v3-run
docker info --format '{{.ServerVersion}}' >/dev/null
if ! docker container inspect "$container" >/dev/null 2>&1; then
    if ! docker image inspect "$image" >/dev/null 2>&1; then
        docker build -f "$root/scripts/docker/Dockerfile.verify" -t "$image" "$root"
    fi
    docker create --name "$container" --init --label "pinset.verify.root=$root" --mount "type=bind,source=$root,target=/source,readonly" --mount type=volume,source=pinset-v3-cargo,target=/usr/local/cargo/registry --mount type=volume,source=pinset-v3-target,target=/build/target --mount type=volume,source=pinset-v3-sdk-cache,target=/sdk-cache --mount type=volume,source=pinset-v3-sdk-cache,target=/run/pinset/v3/cache --mount "type=bind,source=$root/output,target=/reports" "$image" sleep infinity >/dev/null
fi
# Inspect in the existing container with its bundled Python; no host dependency.
# Exact mounts prevent reuse of another checkout or a container carrying host secrets.
if [ "$(docker inspect "$container" --format '{{.State.Running}}')" != true ]; then
    docker start "$container" >/dev/null
fi
docker inspect "$container" | docker exec -i "$container" python3 -c '
import json,sys
container=json.load(sys.stdin)[0]
mounts={m["Destination"]:m for m in container["Mounts"]}
expected={"/source":("bind",sys.argv[1],False),"/reports":("bind",sys.argv[1]+"/output",True),
"/usr/local/cargo/registry":("volume","pinset-v3-cargo",True),"/build/target":("volume","pinset-v3-target",True),
"/sdk-cache":("volume","pinset-v3-sdk-cache",True),"/run/pinset/v3/cache":("volume","pinset-v3-sdk-cache",True)}
assert mounts.keys()==expected.keys(),"unexpected verification mounts"
for path,(kind,origin,writable) in expected.items():
    mount=mounts[path]
    assert (mount["Type"],mount["Source"] if kind=="bind" else mount["Name"],mount["RW"])==(kind,origin,writable),"verification mount mismatch: "+path
assert container["Config"]["Cmd"]==["sleep","infinity"],"unexpected verification command"
' "$root"
digest="$(docker inspect "$container" --format '{{.Image}}')"
id="$(docker inspect "$container" --format '{{.Id}}')"
echo "Reusing $container (${id:0:12})"
docker exec --env "PINSET_VERIFY_IMAGE_DIGEST=$digest" --env "PINSET_VERIFY_CONTAINER_ID=$id" "$container" bash /source/scripts/docker/verify.sh "${1:-fast}"
