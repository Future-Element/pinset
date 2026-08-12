$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if (-not $env:PINSET_BIN) {
    throw 'PINSET_BIN must point to the release pinset.exe binary'
}

$Pinset = [System.IO.Path]::GetFullPath($env:PINSET_BIN)
$GlobalVersion = if ($env:PINSET_GLOBAL_VERSION) { $env:PINSET_GLOBAL_VERSION } else { '24.0.0' }
$ProjectVersion = if ($env:PINSET_PROJECT_VERSION) { $env:PINSET_PROJECT_VERSION } else { '22.0.0' }
$VersionPattern = '^\d+\.\d+\.\d+$'

if ($GlobalVersion -notmatch $VersionPattern -or $ProjectVersion -notmatch $VersionPattern) {
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
    Assert-ExactOutput "v$GlobalVersion" (& $Pinset exec -- node --version) 'global pinset exec node'
    Assert-VersionOutput (& $Pinset exec -- npm --version) 'global pinset exec npm'
    Assert-VersionOutput (& $Pinset exec -- npx --version) 'global pinset exec npx'
    Assert-VersionOutput (& $Pinset exec -- corepack --version) 'global pinset exec corepack'

    $ShimDirectory = ((& $Pinset shim path) | Out-String).Trim()
    if (-not $ShimDirectory) {
        throw 'pinset shim path returned an empty path'
    }
    $env:PATH = "$ShimDirectory;$OriginalPath"
    Assert-ExactOutput "v$GlobalVersion" (& node --version) 'global direct node'
    Assert-VersionOutput (& npm --version) 'global direct npm'
    Assert-VersionOutput (& npx --version) 'global direct npx'
    Assert-VersionOutput (& corepack --version) 'global direct corepack'

    $Project = Join-Path $TestRoot 'project'
    New-Item -ItemType Directory -Path $Project | Out-Null
    Set-Location $Project
    & $Pinset init
    & $Pinset use "node@$ProjectVersion"
    Assert-ExactOutput "v$ProjectVersion" (& node --version) 'project direct node'
    Assert-VersionOutput (& npm --version) 'project direct npm'
    Assert-VersionOutput (& npx --version) 'project direct npx'
    Assert-VersionOutput (& corepack --version) 'project direct corepack'

    Set-Location $TestRoot
    Assert-ExactOutput "v$GlobalVersion" (& node --version) 'restored global direct node'
    Write-Host 'Windows real runtime acceptance passed'
}
finally {
    $env:PATH = $OriginalPath
    Set-Location ([System.IO.Path]::GetTempPath())
    if (Test-Path -LiteralPath $TestRoot) {
        Remove-Item -LiteralPath $TestRoot -Recurse -Force
    }
}
