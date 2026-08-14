[CmdletBinding()]
param(
    [switch]$Yes,
    [switch]$DryRun,
    [string]$InstallDir = $env:PINSET_INSTALL_DIR,
    [string]$PinsetHome = $env:PINSET_HOME,
    [string]$ShimDir = $env:PINSET_SHIM_DIR,
    [string]$ShimBinary = $env:PINSET_SHIM_BINARY,
    [switch]$AllowNonstandardHome
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

if ($Yes -and $DryRun) {
    throw 'pinset uninstaller: -Yes and -DryRun cannot be used together'
}

function Fail([string]$Message) {
    throw "pinset uninstaller: $Message"
}

function Normalize-AbsolutePath([string]$Label, [string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path) -or -not [IO.Path]::IsPathRooted($Path)) {
        Fail "$Label must be an absolute path: $Path"
    }
    $full = [IO.Path]::GetFullPath($Path)
    $root = [IO.Path]::GetPathRoot($full)
    if ($full.Length -gt $root.Length) {
        $full = $full.TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)
    }
    if ([string]::Equals($full, $root, [StringComparison]::OrdinalIgnoreCase)) {
        Fail "$Label cannot be a filesystem root: $full"
    }
    return $full
}

function Paths-Equal([string]$Left, [string]$Right) {
    if ([string]::IsNullOrWhiteSpace($Left) -or [string]::IsNullOrWhiteSpace($Right)) {
        return $false
    }
    return [string]::Equals(
        [IO.Path]::GetFullPath($Left).TrimEnd('\', '/'),
        [IO.Path]::GetFullPath($Right).TrimEnd('\', '/'),
        [StringComparison]::OrdinalIgnoreCase
    )
}

if ([string]::IsNullOrWhiteSpace($InstallDir)) {
    $command = Get-Command pinset.exe -CommandType Application -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($null -ne $command) {
        $InstallDir = Split-Path -Parent $command.Source
    } elseif (-not [string]::IsNullOrWhiteSpace($HOME)) {
        $InstallDir = Join-Path $HOME '.local\bin'
    } else {
        Fail 'cannot find pinset.exe; pass -InstallDir'
    }
}
if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
    Fail 'LOCALAPPDATA is not set; pass -PinsetHome'
}
$defaultPinsetHome = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Pinset'))
if ([string]::IsNullOrWhiteSpace($PinsetHome)) {
    $PinsetHome = $defaultPinsetHome
}
$rawPinsetHome = $PinsetHome
if (($rawPinsetHome -split '[\\/]') -contains '.' -or ($rawPinsetHome -split '[\\/]') -contains '..') {
    Fail "PINSET_HOME cannot contain . or .. path components: $rawPinsetHome"
}

$InstallDir = Normalize-AbsolutePath 'install directory' $InstallDir
$PinsetHome = Normalize-AbsolutePath 'PINSET_HOME' $PinsetHome
if (-not $AllowNonstandardHome -and -not (Paths-Equal $PinsetHome $defaultPinsetHome)) {
    Fail "custom PINSET_HOME requires -AllowNonstandardHome: $PinsetHome"
}
if (-not [string]::IsNullOrWhiteSpace($ShimDir)) {
    $ShimDir = Normalize-AbsolutePath 'shim directory' $ShimDir
}
if ([string]::IsNullOrWhiteSpace($ShimBinary)) {
    $ShimBinary = Join-Path $InstallDir 'pinset-shim.exe'
} elseif (-not [IO.Path]::IsPathRooted($ShimBinary)) {
    Fail "shim binary must be an absolute path: $ShimBinary"
} else {
    $ShimBinary = [IO.Path]::GetFullPath($ShimBinary)
}

$protectedHomes = @($HOME, $env:LOCALAPPDATA)
if (-not [string]::IsNullOrWhiteSpace($HOME)) {
    $protectedHomes += (Join-Path $HOME '.local')
    $protectedHomes += (Join-Path $HOME '.local\share')
}
foreach ($protected in $protectedHomes) {
    if (-not [string]::IsNullOrWhiteSpace($protected) -and (Paths-Equal $PinsetHome $protected)) {
        Fail "refusing to use a broad PINSET_HOME: $PinsetHome"
    }
}
$homeLeaf = Split-Path -Leaf $PinsetHome
if (-not $AllowNonstandardHome -and $homeLeaf -notmatch '(?i)^pinset(?:[-._].*)?$') {
    Fail "nonstandard PINSET_HOME name requires -AllowNonstandardHome: $PinsetHome"
}
if (Test-Path -LiteralPath $PinsetHome) {
    $homeItem = Get-Item -LiteralPath $PinsetHome -Force
    if (($homeItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        Fail 'PINSET_HOME is a symbolic link or junction; pass its resolved Pinset-owned directory explicitly'
    }
}

$cliPath = Join-Path $InstallDir 'pinset.exe'
$routerPath = Join-Path $InstallDir 'pinset-shim.exe'
$routeCommands = @(
    'node', 'npm', 'npx', 'corepack', 'pnpm', 'bun', 'bunx', 'go', 'gofmt',
    'flutter', 'dart', 'python', 'python3', 'pip', 'pip3', 'java', 'javac', 'jar',
    'javadoc', 'javap', 'keytool', 'jshell', 'rustc', 'cargo', 'rustdoc', 'rustfmt',
    'cargo-fmt', 'clippy-driver', 'cargo-clippy', 'dotnet'
)

function Test-ManagedRoute([string]$Path) {
    if ((Paths-Equal $Path $cliPath) -or (Paths-Equal $Path $routerPath)) {
        return $false
    }
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        return $false
    }

    if ([IO.Path]::GetExtension($Path) -ieq '.cmd') {
        $command = [IO.Path]::GetFileNameWithoutExtension($Path)
        $escapedRouter = $ShimBinary.Replace('%', '%%')
        $expected = "@echo off`r`nsetlocal DisableDelayedExpansion`r`n`"$escapedRouter`" --as $command -- %*`r`nexit /b %ERRORLEVEL%`r`n"
        try {
            if ([IO.File]::ReadAllText($Path) -ceq $expected) {
                return $true
            }
        } catch {
            return $false
        }
    }

    if (-not (Test-Path -LiteralPath $ShimBinary -PathType Leaf)) {
        return $false
    }
    try {
        $routeItem = Get-Item -LiteralPath $Path -Force
        $shimItem = Get-Item -LiteralPath $ShimBinary -Force
        if ($routeItem.Length -ne $shimItem.Length) {
            return $false
        }
        return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash -eq
            (Get-FileHash -Algorithm SHA256 -LiteralPath $ShimBinary).Hash
    } catch {
        return $false
    }
}

$directories = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
[void]$directories.Add($InstallDir)
if (-not [string]::IsNullOrWhiteSpace($ShimDir)) {
    [void]$directories.Add($ShimDir)
}
foreach ($entry in ($env:PATH -split [IO.Path]::PathSeparator)) {
    if ([string]::IsNullOrWhiteSpace($entry) -or -not [IO.Path]::IsPathRooted($entry)) {
        continue
    }
    try {
        [void]$directories.Add([IO.Path]::GetFullPath($entry))
    } catch {
        continue
    }
}

$managedRoutes = New-Object 'System.Collections.Generic.List[string]'
$managedRouteSet = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
foreach ($directory in $directories) {
    if (-not (Test-Path -LiteralPath $directory -PathType Container)) {
        continue
    }
    foreach ($command in $routeCommands) {
        foreach ($name in @($command, "$command.exe", "$command.cmd", "$command.bat")) {
            $candidate = Join-Path $directory $name
            if ((Test-ManagedRoute $candidate) -and $managedRouteSet.Add($candidate)) {
                $managedRoutes.Add($candidate)
            }
        }
    }
}

Write-Output 'Pinset uninstall plan:'
if (Test-Path -LiteralPath $cliPath) {
    Write-Output "  CLI: $cliPath"
}
if (Test-Path -LiteralPath $routerPath) {
    Write-Output "  command router: $routerPath"
}
foreach ($route in $managedRoutes) {
    Write-Output "  managed command route: $route"
}
if (Test-Path -LiteralPath $PinsetHome) {
    Write-Output "  PINSET_HOME and all managed runtimes: $PinsetHome"
}
Write-Output '  preserved: project pinset.toml/pinset.lock files, PowerShell profiles, and foreign runtimes'

if ($DryRun) {
    Write-Output 'Dry run complete. Nothing was removed.'
    exit 0
}
if (-not $Yes) {
    [Console]::Error.WriteLine('Nothing was removed. Re-run with -Yes after reviewing this plan.')
    exit 2
}

# Verify and remove routes before deleting pinset-shim.exe. This keeps hard-link
# and copied fallback ownership checks available for the entire route pass.
foreach ($route in $managedRoutes) {
    if (Test-ManagedRoute $route) {
        Remove-Item -LiteralPath $route -Force
        Write-Output "Removed managed command route $route"
    }
}
foreach ($binary in @($cliPath, $routerPath)) {
    if (Test-Path -LiteralPath $binary) {
        Remove-Item -LiteralPath $binary -Force
        Write-Output "Removed $binary"
    }
}
if (Test-Path -LiteralPath $PinsetHome) {
    Remove-Item -LiteralPath $PinsetHome -Recurse -Force
    Write-Output "Removed PINSET_HOME and all managed runtimes $PinsetHome"
}

Write-Output 'Pinset uninstall complete. Manually remove any PATH entry or persistent PINSET_* variable you added.'
