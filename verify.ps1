param([ValidateSet('fast','acceptance','platform','integrations','all')][string]$Suite='fast')
$ErrorActionPreference='Stop'
$root=Split-Path -Parent $MyInvocation.MyCommand.Path
$image='pinset-verify:3.0'
$containerName='pinset-v3-run'
New-Item -ItemType Directory -Path (Join-Path $root 'output') -Force | Out-Null
docker info --format '{{.ServerVersion}}' | Out-Null
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$raw=docker container inspect $containerName 2>$null
if ($LASTEXITCODE -ne 0) {
    docker image inspect $image --format '{{.Id}}' 2>$null | Out-Null
    if ($LASTEXITCODE -ne 0) {
        docker build --file "$root/scripts/docker/Dockerfile.verify" --tag $image $root
        if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    }
    docker create --name $containerName --init --label "pinset.verify.root=$root" --mount "type=bind,source=$root,target=/source,readonly" --mount 'type=volume,source=pinset-v3-cargo,target=/usr/local/cargo/registry' --mount 'type=volume,source=pinset-v3-target,target=/build/target' --mount 'type=volume,source=pinset-v3-sdk-cache,target=/sdk-cache' --mount 'type=volume,source=pinset-v3-sdk-cache,target=/run/pinset/v3/cache' --mount "type=bind,source=$root/output,target=/reports" $image sleep infinity | Out-Null
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $raw=docker container inspect $containerName
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
$container=($raw | ConvertFrom-Json)[0]
function Normalize-MountPath([string]$path) {
    $path=$path -replace '^/run/desktop/mnt/host/([a-zA-Z])/', '$1:/'
    return [IO.Path]::GetFullPath($path.Replace('/', '\')).TrimEnd('\')
}
$binds=@{'/source'=$root;'/reports'=(Join-Path $root 'output')}
$volumes=@{'/usr/local/cargo/registry'='pinset-v3-cargo';'/build/target'='pinset-v3-target';'/sdk-cache'='pinset-v3-sdk-cache';'/run/pinset/v3/cache'='pinset-v3-sdk-cache'}
if ($container.Mounts.Count -ne ($binds.Count+$volumes.Count)) { throw 'Verification container has unexpected mounts; refusing to reuse it.' }
foreach ($mount in $container.Mounts) {
    if ($binds.ContainsKey($mount.Destination)) {
        if ($mount.Type -ne 'bind' -or (Normalize-MountPath $mount.Source) -ne (Normalize-MountPath $binds[$mount.Destination]) -or $mount.RW -ne ($mount.Destination -eq '/reports')) { throw "Verification container mount mismatch: $($mount.Destination)" }
    } elseif ($volumes.ContainsKey($mount.Destination)) {
        if ($mount.Type -ne 'volume' -or $mount.Name -ne $volumes[$mount.Destination] -or -not $mount.RW) { throw "Verification container volume mismatch: $($mount.Destination)" }
    } else { throw "Unexpected verification mount: $($mount.Destination)" }
}
if (($container.Config.Cmd -join ' ') -ne 'sleep infinity') { throw 'Verification container must run sleep infinity.' }
if (-not $container.State.Running) {
    docker start $containerName | Out-Null
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
Write-Host "Reusing $containerName ($($container.Id.Substring(0,12)))"
docker exec --env "PINSET_VERIFY_IMAGE_DIGEST=$($container.Image)" --env "PINSET_VERIFY_CONTAINER_ID=$($container.Id)" $containerName bash /source/scripts/docker/verify.sh $Suite
exit $LASTEXITCODE
