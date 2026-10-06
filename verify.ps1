param([ValidateSet('fast','acceptance','platform','integrations','all')][string]$Suite='fast')
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $MyInvocation.MyCommand.Path
$image='pinset-verify:3.0'
New-Item -ItemType Directory -Path (Join-Path $root 'output') -Force | Out-Null
docker build --file "$root/scripts/docker/Dockerfile.verify" --tag $image $root
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$digest=docker image inspect $image --format '{{.Id}}'
docker run --rm --init --env "PINSET_VERIFY_IMAGE_DIGEST=$digest" --mount "type=bind,source=$root,target=/source,readonly" --mount 'type=volume,source=pinset-v3-cargo,target=/usr/local/cargo/registry' --mount 'type=volume,source=pinset-v3-target,target=/build/target' --mount 'type=volume,source=pinset-v3-sdk-cache,target=/sdk-cache' --mount 'type=volume,source=pinset-v3-sdk-cache,target=/run/pinset/v3/cache' --mount "type=bind,source=$root/output,target=/reports" $image bash /source/scripts/docker/verify.sh $Suite
exit $LASTEXITCODE
