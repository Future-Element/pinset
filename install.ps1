[CmdletBinding()]
param(
    [string] $Version = '2.0.0',
    [string] $InstallDir = (Join-Path $env:LOCALAPPDATA 'Pinset\bin')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:-rc\.[0-9]+)?$') {
    throw 'Version must be an exact stable or rc release without a leading v.'
}

$archive = 'pinset-windows-x86_64.zip'
$release = "https://github.com/Future-Element/pinset/releases/download/v$Version"
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ("pinset-install-" + [guid]::NewGuid().ToString('N'))
$archivePath = Join-Path $temporaryRoot $archive
$checksumsPath = Join-Path $temporaryRoot 'SHA256SUMS'
$extractPath = Join-Path $temporaryRoot 'extract'

function Invoke-PinsetDownload([string] $Uri, [string] $OutFile) {
    for ($attempt = 1; $attempt -le 4; $attempt++) {
        try {
            Invoke-WebRequest -Uri $Uri -OutFile $OutFile
            return
        } catch {
            if ($attempt -eq 4) { throw }
            Start-Sleep -Seconds 2
        }
    }
}

try {
    New-Item -ItemType Directory -Force -Path $temporaryRoot, $extractPath | Out-Null
    Invoke-PinsetDownload "$release/$archive" $archivePath
    Invoke-PinsetDownload "$release/SHA256SUMS" $checksumsPath

    $escapedArchive = [regex]::Escape($archive)
    $line = Get-Content -LiteralPath $checksumsPath | Where-Object {
        $_ -match "^[0-9a-fA-F]{64}\s+$escapedArchive$"
    } | Select-Object -First 1
    if (-not $line) { throw "SHA256SUMS has no exact entry for $archive" }
    $expected = ($line -split '\s+')[0]
    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $archivePath).Hash
    if ($actual -ne $expected) { throw "SHA-256 mismatch for $archive" }

    Expand-Archive -LiteralPath $archivePath -DestinationPath $extractPath
    $entries = @(Get-ChildItem -LiteralPath $extractPath -Force)
    if ($entries.Count -ne 2 -or
        -not (Test-Path -LiteralPath (Join-Path $extractPath 'pinset.exe') -PathType Leaf) -or
        -not (Test-Path -LiteralPath (Join-Path $extractPath 'pinset-shim.exe') -PathType Leaf)) {
        throw 'Release archive must contain exactly pinset.exe and pinset-shim.exe.'
    }

    $resolvedParent = [IO.Path]::GetFullPath((Split-Path -Parent $InstallDir))
    $resolvedInstall = [IO.Path]::GetFullPath($InstallDir)
    if (-not $resolvedInstall.StartsWith($resolvedParent, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Install directory did not resolve under its expected parent.'
    }
    New-Item -ItemType Directory -Force -Path $resolvedInstall | Out-Null
    $newCli = Join-Path $resolvedInstall '.pinset.new.exe'
    $newShim = Join-Path $resolvedInstall '.pinset-shim.new.exe'
    Copy-Item -LiteralPath (Join-Path $extractPath 'pinset.exe') -Destination $newCli
    Copy-Item -LiteralPath (Join-Path $extractPath 'pinset-shim.exe') -Destination $newShim
    $expectedVersion = "pinset $Version"
    $reportedVersion = (& $newCli --version | Out-String).Trim()
    if ($LASTEXITCODE -ne 0 -or $reportedVersion -ne $expectedVersion) {
        throw "Downloaded Pinset CLI reported '$reportedVersion', expected '$expectedVersion'."
    }

    $cli = Join-Path $resolvedInstall 'pinset.exe'
    $shim = Join-Path $resolvedInstall 'pinset-shim.exe'
    $cliBackup = Join-Path $resolvedInstall 'pinset.exe.bak'
    $shimBackup = Join-Path $resolvedInstall 'pinset-shim.exe.bak'
    Remove-Item -LiteralPath $cliBackup, $shimBackup -Force -ErrorAction SilentlyContinue
    try {
        if (Test-Path -LiteralPath $cli -PathType Leaf) {
            Move-Item -LiteralPath $cli -Destination $cliBackup
        }
        if (Test-Path -LiteralPath $shim -PathType Leaf) {
            Move-Item -LiteralPath $shim -Destination $shimBackup
        }
        Move-Item -LiteralPath $newCli -Destination $cli
        Move-Item -LiteralPath $newShim -Destination $shim

        $installedVersion = (& $cli --version | Out-String).Trim()
        if ($LASTEXITCODE -ne 0 -or $installedVersion -ne $expectedVersion) {
            throw "Installed Pinset CLI failed its version handshake."
        }
        & $cli shim install --all --binary $shim --dir $resolvedInstall
        if ($LASTEXITCODE -ne 0) {
            throw 'Pinset failed to register Provider command shims.'
        }
    } catch {
        Remove-Item -LiteralPath $cli, $shim -Force -ErrorAction SilentlyContinue
        if (Test-Path -LiteralPath $cliBackup -PathType Leaf) {
            Move-Item -LiteralPath $cliBackup -Destination $cli
        }
        if (Test-Path -LiteralPath $shimBackup -PathType Leaf) {
            Move-Item -LiteralPath $shimBackup -Destination $shim
        }
        throw
    }

    Write-Output "Installed Pinset CLI and Provider command shims in $resolvedInstall"
    Write-Output "Runtime payloads remain isolated under PINSET_HOME\installs and are not downloaded by this installer."
    Write-Output "For this PowerShell session: `$env:PATH = '$resolvedInstall' + [IO.Path]::PathSeparator + `$env:PATH"
} finally {
    if (Test-Path -LiteralPath $temporaryRoot) {
        Remove-Item -LiteralPath $temporaryRoot -Recurse -Force
    }
}
