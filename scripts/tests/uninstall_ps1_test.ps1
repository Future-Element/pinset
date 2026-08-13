$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$script = Join-Path $root 'uninstall.ps1'
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('pinset-uninstall-ps1-test-' + [guid]::NewGuid().ToString('N'))
$testHome = Join-Path $testRoot 'user home'
$installDir = Join-Path $testRoot 'install dir'
$pinsetHome = Join-Path $testRoot 'data\Pinset'
$shimDir = Join-Path $testRoot 'custom shims'
$pathDir = Join-Path $testRoot 'path shims'
$projectDir = Join-Path $testRoot 'project'
$engine = (Get-Process -Id $PID).Path

try {
    foreach ($directory in @(
        $testHome,
        $installDir,
        (Join-Path $pinsetHome 'installs\node\24.0.0\windows-x86_64'),
        (Join-Path $pinsetHome 'installs\python\3.14.0\windows-x86_64'),
        $shimDir,
        $pathDir,
        $projectDir
    )) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }

    $cli = Join-Path $installDir 'pinset.exe'
    $router = Join-Path $installDir 'pinset-shim.exe'
    [IO.File]::WriteAllText($cli, 'pinset test binary')
    [IO.File]::WriteAllText($router, 'pinset shim test binary')

    foreach ($command in @('node', 'npm', 'go')) {
        $escapedRouter = $router.Replace('%', '%%')
        $wrapper = "@echo off`r`nsetlocal DisableDelayedExpansion`r`n`"$escapedRouter`" --as $command -- %*`r`nexit /b %ERRORLEVEL%`r`n"
        [IO.File]::WriteAllText((Join-Path $installDir "$command.cmd"), $wrapper)
    }
    Copy-Item -LiteralPath $router -Destination (Join-Path $shimDir 'corepack.exe')
    Copy-Item -LiteralPath $router -Destination (Join-Path $pathDir 'npx.exe')
    [IO.File]::WriteAllText((Join-Path $pathDir 'python.exe'), 'foreign python')
    [IO.File]::WriteAllText((Join-Path $projectDir 'pinset.toml'), 'project config')
    [IO.File]::WriteAllText((Join-Path $projectDir 'pinset.lock'), 'project lock')
    [IO.File]::WriteAllText((Join-Path $pinsetHome 'installs\node\24.0.0\windows-x86_64\node.exe'), 'node runtime')
    [IO.File]::WriteAllText((Join-Path $pinsetHome 'installs\python\3.14.0\windows-x86_64\python.exe'), 'python runtime')

    $previousPath = $env:PATH
    $previousHome = $env:HOME
    $env:PATH = $pathDir + [IO.Path]::PathSeparator + $env:PATH
    $env:HOME = $testHome
    try {
        & $engine -NoProfile -File $script `
            -InstallDir $installDir `
            -PinsetHome $pinsetHome `
            -AllowNonstandardHome `
            -ShimDir $shimDir `
            -ShimBinary $router
        if ($LASTEXITCODE -eq 0) {
            throw 'uninstall without -Yes unexpectedly succeeded'
        }
        if (-not (Test-Path -LiteralPath $cli) -or -not (Test-Path -LiteralPath $pinsetHome)) {
            throw 'unconfirmed uninstall changed files'
        }

        & $engine -NoProfile -File $script -DryRun `
            -InstallDir $installDir `
            -PinsetHome $pinsetHome `
            -AllowNonstandardHome `
            -ShimDir $shimDir `
            -ShimBinary $router
        if ($LASTEXITCODE -ne 0) {
            throw 'dry run failed'
        }
        if (-not (Test-Path -LiteralPath $cli) -or -not (Test-Path -LiteralPath $pinsetHome)) {
            throw 'dry run changed files'
        }

        & $engine -NoProfile -File $script -Yes `
            -InstallDir $installDir `
            -PinsetHome $testHome `
            -ShimBinary $router
        if ($LASTEXITCODE -eq 0) {
            throw 'broad PINSET_HOME unexpectedly succeeded'
        }
        if (-not (Test-Path -LiteralPath $testHome)) {
            throw 'broad PINSET_HOME was removed'
        }

        & $engine -NoProfile -File $script -Yes `
            -InstallDir $installDir `
            -PinsetHome (Join-Path $pinsetHome 'child\..') `
            -AllowNonstandardHome `
            -ShimBinary $router
        if ($LASTEXITCODE -eq 0) {
            throw 'PINSET_HOME traversal unexpectedly succeeded'
        }
        if (-not (Test-Path -LiteralPath $pinsetHome)) {
            throw 'PINSET_HOME traversal changed files'
        }

        & $engine -NoProfile -File $script -Yes `
            -InstallDir $installDir `
            -PinsetHome $pinsetHome `
            -AllowNonstandardHome `
            -ShimDir $shimDir `
            -ShimBinary $router
        if ($LASTEXITCODE -ne 0) {
            throw 'confirmed uninstall failed'
        }
    } finally {
        $env:PATH = $previousPath
        $env:HOME = $previousHome
    }

    foreach ($removed in @(
        $cli,
        $router,
        (Join-Path $installDir 'node.cmd'),
        (Join-Path $installDir 'npm.cmd'),
        (Join-Path $shimDir 'corepack.exe'),
        (Join-Path $installDir 'go.cmd'),
        (Join-Path $pathDir 'npx.exe'),
        $pinsetHome
    )) {
        if (Test-Path -LiteralPath $removed) {
            throw "expected removed target still exists: $removed"
        }
    }
    foreach ($preserved in @(
        (Join-Path $pathDir 'python.exe'),
        (Join-Path $projectDir 'pinset.toml'),
        (Join-Path $projectDir 'pinset.lock'),
        $installDir,
        $shimDir
    )) {
        if (-not (Test-Path -LiteralPath $preserved)) {
            throw "expected preserved target is missing: $preserved"
        }
    }

    Write-Output 'uninstall.ps1 isolated tests passed'
} finally {
    if (Test-Path -LiteralPath $testRoot) {
        Remove-Item -LiteralPath $testRoot -Recurse -Force
    }
}
