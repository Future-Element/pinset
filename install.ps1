[CmdletBinding()]
param(
    [string] $Version = '3.0.1',
    [string] $InstallDir = (Join-Path $(if ($env:PINSET_HOME) { $env:PINSET_HOME } else { Join-Path $env:USERPROFILE '.pinset' }) 'v3\bin')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if ($Version -notmatch '^3\.[0-9]+\.[0-9]+(?:-rc\.[0-9]+)?$') {
    throw 'Version must be an exact stable or rc release without a leading v.'
}
$archive = "pinset-v$Version-windows-x86_64.zip"
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
    $managedRoot = [IO.Path]::GetFullPath((Join-Path $(if ($env:PINSET_HOME) { $env:PINSET_HOME } else { Join-Path $env:USERPROFILE '.pinset' }) 'v3'))
    if ($resolvedInstall -eq (Join-Path $managedRoot 'bin')) {
        $ownership = Join-Path $managedRoot '.pinset-home.json'
        if (Test-Path -LiteralPath $managedRoot) {
            $rootItem = Get-Item -LiteralPath $managedRoot -Force
            if ($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Pinset home cannot be a reparse point.' }
            if (-not (Test-Path -LiteralPath $ownership -PathType Leaf) -and @(Get-ChildItem -LiteralPath $managedRoot -Force).Count) {
                throw 'Refusing to adopt a populated, unmarked Pinset v3 home.'
            }
        }
        if (Test-Path -LiteralPath $ownership) {
            if ((Get-Content -LiteralPath $ownership -Raw | ConvertFrom-Json).protocol -ne 'pinset/3') { throw 'Invalid Pinset home ownership.' }
        } else {
            New-Item -ItemType Directory -Force -Path $managedRoot | Out-Null
            [IO.File]::WriteAllText($ownership, '{"protocol":"pinset/3","owner":"pinset"}', [Text.UTF8Encoding]::new($false))
        }
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
        & $cli self repair
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
    Write-Output "Runtime payloads remain isolated under PINSET_HOME\v3\installs and are downloaded by explicit install commands."
    Write-Output "For this PowerShell session: `$env:PATH = '$resolvedInstall' + [IO.Path]::PathSeparator + `$env:PATH"
} finally {
    if (Test-Path -LiteralPath $temporaryRoot) {
        Remove-Item -LiteralPath $temporaryRoot -Recurse -Force
    }
}
