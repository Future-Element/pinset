$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if (-not $env:PINSET_BIN) {
    throw 'PINSET_BIN must point to the release pinset.exe binary'
}

$Pinset = [System.IO.Path]::GetFullPath($env:PINSET_BIN)
$GlobalVersion = if ($env:PINSET_GLOBAL_VERSION) { $env:PINSET_GLOBAL_VERSION } else { '24.0.0' }
$ProjectVersion = if ($env:PINSET_PROJECT_VERSION) { $env:PINSET_PROJECT_VERSION } else { '22.0.0' }
$PnpmVersion = if ($env:PINSET_PNPM_VERSION) { $env:PINSET_PNPM_VERSION } else { '11.21.0' }
$BunVersion = if ($env:PINSET_BUN_VERSION) { $env:PINSET_BUN_VERSION } else { '1.3.14' }
$VersionPattern = '^\d+\.\d+\.\d+$'

if ($GlobalVersion -notmatch $VersionPattern -or $ProjectVersion -notmatch $VersionPattern -or
    $PnpmVersion -notmatch $VersionPattern -or $BunVersion -notmatch $VersionPattern) {
    throw 'acceptance versions must use x.y.z'
}
if (-not (Test-Path -LiteralPath $Pinset -PathType Leaf)) {
    throw "PINSET_BIN is not a file: $Pinset"
}

$TestRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("pinset-acceptance-" + [guid]::NewGuid().ToString('N'))
$OriginalPath = $env:PATH

function Assert-ExactOutput {
    param(
        [Parameter(Mandatory = $true)][string]$Expected,
        [Parameter(Mandatory = $true)][string[]]$Actual,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $Value = ($Actual | Out-String).Trim()
    if ($Value -ne $Expected) {
        throw "$Label expected '$Expected', got '$Value'"
    }
}

function Assert-VersionOutput {
    param(
        [Parameter(Mandatory = $true)][string[]]$Actual,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $Value = ($Actual | Out-String).Trim()
    if ($Value -notmatch '^\d+\.\d+\.\d+') {
        throw "$Label returned an invalid version: '$Value'"
    }
}

try {
    New-Item -ItemType Directory -Path $TestRoot | Out-Null
    $env:PINSET_HOME = Join-Path $TestRoot 'pinset-home'
    Remove-Item Env:PINSET_LANG -ErrorAction SilentlyContinue
    Set-Location $TestRoot

    & $Pinset --lang zh-CN | Out-Null
    if ((Get-Content -LiteralPath (Join-Path $env:PINSET_HOME 'settings.toml') -Raw) -notmatch 'language = "zh-CN"') {
        throw 'language setting was not persisted'
    }

    & $Pinset use "node@$GlobalVersion" --global
    if (-not ((& $Pinset list pnpm --available) -contains "pnpm@$PnpmVersion")) {
        throw "pnpm@$PnpmVersion was not listed as available"
    }
    if (-not ((& $Pinset list bun --available) -contains "bun@$BunVersion")) {
        throw "bun@$BunVersion was not listed as available"
    }
    & $Pinset use "pnpm@$PnpmVersion" --global
    & $Pinset use "bun@$BunVersion" --global
    Assert-ExactOutput "v$GlobalVersion" (& $Pinset exec -- node --version) 'global pinset exec node'
    Assert-VersionOutput (& $Pinset exec -- npm --version) 'global pinset exec npm'
    Assert-VersionOutput (& $Pinset exec -- npx --version) 'global pinset exec npx'
    Assert-VersionOutput (& $Pinset exec -- corepack --version) 'global pinset exec corepack'
    Assert-ExactOutput $PnpmVersion (& $Pinset exec -- pnpm --version) 'global pinset exec pnpm'
    Assert-ExactOutput $BunVersion (& $Pinset exec -- bun --version) 'global pinset exec bun'
    Assert-ExactOutput $BunVersion (& $Pinset exec -- bunx --version) 'global pinset exec bunx'

    $ShimDirectory = ((& $Pinset shim path) | Out-String).Trim()
    if (-not $ShimDirectory) {
        throw 'pinset shim path returned an empty path'
    }
    $env:PATH = "$ShimDirectory;$OriginalPath"
    Assert-ExactOutput "v$GlobalVersion" (& node --version) 'global direct node'
    Assert-VersionOutput (& npm --version) 'global direct npm'
    Assert-VersionOutput (& npx --version) 'global direct npx'
    Assert-VersionOutput (& corepack --version) 'global direct corepack'
    Assert-ExactOutput $PnpmVersion (& pnpm --version) 'global direct pnpm'
    Assert-ExactOutput $BunVersion (& bun --version) 'global direct bun'
    Assert-ExactOutput $BunVersion (& bunx --version) 'global direct bunx'

    $Project = Join-Path $TestRoot 'project'
    New-Item -ItemType Directory -Path $Project | Out-Null
    Set-Location $Project
    Set-Content -LiteralPath (Join-Path $Project 'package.json') -Value '{"private":true}' -NoNewline
    & $Pinset init
    & $Pinset use "node@$ProjectVersion"
    Assert-ExactOutput "v$ProjectVersion" (& node --version) 'project direct node'
    Assert-VersionOutput (& npm --version) 'project direct npm'
    Assert-VersionOutput (& npx --version) 'project direct npx'
    Assert-VersionOutput (& corepack --version) 'project direct corepack'
    Assert-ExactOutput $PnpmVersion (& pnpm --version) 'project direct pnpm'
    Assert-ExactOutput $BunVersion (& bun --version) 'project direct bun'
    $PnpmChildNodeOutput = @(& pnpm exec node --version 2>&1 | ForEach-Object { $_.ToString() })
    $PnpmChildNodeStatus = $LASTEXITCODE
    Write-Host "pnpm exec node --version => status=$PnpmChildNodeStatus output=$($PnpmChildNodeOutput -join ' | ')"
    if ($PnpmChildNodeStatus -ne 0) {
        throw "project pnpm child node exited with $PnpmChildNodeStatus"
    }
    if ($PnpmChildNodeOutput.Count -eq 0) {
        throw 'project pnpm child node returned no output'
    }
    if ($PnpmChildNodeOutput -notcontains "v$ProjectVersion") {
        throw "project pnpm child node did not report v$ProjectVersion"
    }

    Set-Location $TestRoot
    Assert-ExactOutput "v$GlobalVersion" (& node --version) 'restored global direct node'
    Assert-ExactOutput $PnpmVersion (& pnpm --version) 'restored global direct pnpm'
    Assert-ExactOutput $BunVersion (& bun --version) 'restored global direct bun'
    Write-Host 'Windows real Node, pnpm and Bun acceptance passed'
}
finally {
    $env:PATH = $OriginalPath
    Set-Location ([System.IO.Path]::GetTempPath())
    if (Test-Path -LiteralPath $TestRoot) {
        Remove-Item -LiteralPath $TestRoot -Recurse -Force
    }
}
