$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if (-not $env:PINSET_BIN) {
    throw 'PINSET_BIN must point to the release pinset.exe binary'
}

$Pinset = [System.IO.Path]::GetFullPath($env:PINSET_BIN)
$GlobalVersion = if ($env:PINSET_GLOBAL_VERSION) { $env:PINSET_GLOBAL_VERSION } else { '24.0.0' }
$ProjectVersion = if ($env:PINSET_PROJECT_VERSION) { $env:PINSET_PROJECT_VERSION } else { '22.0.0' }
$BunVersion = if ($env:PINSET_BUN_VERSION) { $env:PINSET_BUN_VERSION } else { '1.3.14' }
$GlobalPnpmSelector = if ($env:PINSET_GLOBAL_PNPM_SELECTOR) { $env:PINSET_GLOBAL_PNPM_SELECTOR } else { 'latest' }
$ProjectPnpmSelector = if ($env:PINSET_PROJECT_PNPM_SELECTOR) { $env:PINSET_PROJECT_PNPM_SELECTOR } else { '10' }
$ProjectBunSelector = if ($env:PINSET_PROJECT_BUN_SELECTOR) { $env:PINSET_PROJECT_BUN_SELECTOR } else { '1.2' }
$GlobalGoSelector = if ($env:PINSET_GLOBAL_GO_SELECTOR) { $env:PINSET_GLOBAL_GO_SELECTOR } else { 'latest' }
$ProjectGoSelector = if ($env:PINSET_PROJECT_GO_SELECTOR) { $env:PINSET_PROJECT_GO_SELECTOR } else { '1.24' }
$GlobalPythonSelector = if ($env:PINSET_GLOBAL_PYTHON_SELECTOR) { $env:PINSET_GLOBAL_PYTHON_SELECTOR } else { 'latest' }
$ProjectPythonSelector = if ($env:PINSET_PROJECT_PYTHON_SELECTOR) { $env:PINSET_PROJECT_PYTHON_SELECTOR } else { '3.13' }
$GlobalJavaSelector = if ($env:PINSET_GLOBAL_JAVA_SELECTOR) { $env:PINSET_GLOBAL_JAVA_SELECTOR } else { 'lts' }
$GlobalRustSelector = if ($env:PINSET_GLOBAL_RUST_SELECTOR) { $env:PINSET_GLOBAL_RUST_SELECTOR } else { 'latest' }
$GlobalDotnetSelector = if ($env:PINSET_GLOBAL_DOTNET_SELECTOR) { $env:PINSET_GLOBAL_DOTNET_SELECTOR } else { 'lts' }
$GlobalFlutterSelector = if ($env:PINSET_GLOBAL_FLUTTER_SELECTOR) { $env:PINSET_GLOBAL_FLUTTER_SELECTOR } else { 'latest' }
$ProjectFlutterSelector = if ($env:PINSET_PROJECT_FLUTTER_SELECTOR) { $env:PINSET_PROJECT_FLUTTER_SELECTOR } else { '3.44' }
$SkipFlutterRuntimeValue = if ($env:PINSET_SKIP_FLUTTER_RUNTIME) { $env:PINSET_SKIP_FLUTTER_RUNTIME } else { '0' }
$SkipFlutterRuntime = $SkipFlutterRuntimeValue -eq '1'
$VersionPattern = '^\d+\.\d+\.\d+$'

if ($GlobalVersion -notmatch $VersionPattern -or $ProjectVersion -notmatch $VersionPattern -or
    $BunVersion -notmatch $VersionPattern) {
    throw 'acceptance versions must use x.y.z'
}
if (-not (Test-Path -LiteralPath $Pinset -PathType Leaf)) {
    throw "PINSET_BIN is not a file: $Pinset"
}
if ($SkipFlutterRuntimeValue -notin @('0', '1')) {
    throw 'PINSET_SKIP_FLUTTER_RUNTIME must be 0 or 1'
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

function ConvertFrom-FlutterMachineOutput {
    param(
        [Parameter(Mandatory = $true)][object[]]$Output,
        [Parameter(Mandatory = $true)][string]$Label
    )

    $Value = (($Output | ForEach-Object { $_.ToString() }) -join "`n").Trim()
    $JsonStart = $Value.IndexOf('{')
    $JsonEnd = $Value.LastIndexOf('}')
    if ($JsonStart -lt 0 -or $JsonEnd -lt $JsonStart) {
        throw "$Label did not return Flutter machine JSON: '$Value'"
    }

    $Json = $Value.Substring($JsonStart, $JsonEnd - $JsonStart + 1)
    try {
        return $Json | ConvertFrom-Json
    }
    catch {
        throw "$Label returned invalid Flutter machine JSON: $($_.Exception.Message)"
    }
}

function Assert-PinsetPipRoutesToPython {
    param([Parameter(Mandatory = $true)][string]$Label)

    $Expected = ((& $Pinset exec -- python -m pip --version) | Out-String).Trim()
    if ($Expected -notmatch '^pip \d+(\.\d+)+') {
        throw "$Label python -m pip returned an invalid version: '$Expected'"
    }
    Write-Host "${Label} python -m pip: $Expected"
    foreach ($PipCommand in @('pip', 'pip3')) {
        $ActualLines = @(& $Pinset exec -- $PipCommand --version 2>&1)
        $Status = $LASTEXITCODE
        $Actual = (($ActualLines | ForEach-Object { $_.ToString() }) -join "`n").Trim()
        if ($Status -ne 0) {
            throw "$Label $PipCommand failed with exit code ${Status}: '$Actual'"
        }
        if ($Actual -ne $Expected) {
            throw "$Label $PipCommand expected '$Expected', got '$Actual'"
        }
    }
}

function Assert-DirectPipRoutesToPython {
    param([Parameter(Mandatory = $true)][string]$Label)

    $Expected = ((& python -m pip --version) | Out-String).Trim()
    if ($Expected -notmatch '^pip \d+(\.\d+)+') {
        throw "$Label python -m pip returned an invalid version: '$Expected'"
    }
    Write-Host "${Label} python -m pip: $Expected"
    foreach ($PipCommand in @('pip', 'pip3')) {
        $ActualLines = @(& $PipCommand --version 2>&1)
        $Status = $LASTEXITCODE
        $Actual = (($ActualLines | ForEach-Object { $_.ToString() }) -join "`n").Trim()
        if ($Status -ne 0) {
            throw "$Label $PipCommand failed with exit code ${Status}: '$Actual'"
        }
        if ($Actual -ne $Expected) {
            throw "$Label $PipCommand expected '$Expected', got '$Actual'"
        }
    }
}

try {
    New-Item -ItemType Directory -Path $TestRoot | Out-Null
    $env:PINSET_HOME = Join-Path $TestRoot 'pinset-home'
    Remove-Item Env:PINSET_LANG -ErrorAction SilentlyContinue
    Remove-Item Env:GOTOOLCHAIN -ErrorAction SilentlyContinue
    Remove-Item Env:VIRTUAL_ENV -ErrorAction SilentlyContinue
    Remove-Item Env:PYTHONHOME -ErrorAction SilentlyContinue
    Remove-Item Env:FLUTTER_ROOT -ErrorAction SilentlyContinue
    Remove-Item Env:FLUTTER_SUPPRESS_ANALYTICS -ErrorAction SilentlyContinue
    Remove-Item Env:JAVA_HOME -ErrorAction SilentlyContinue
    Remove-Item Env:CLASSPATH -ErrorAction SilentlyContinue
    Remove-Item Env:JAVA_TOOL_OPTIONS -ErrorAction SilentlyContinue
    Remove-Item Env:JDK_JAVA_OPTIONS -ErrorAction SilentlyContinue
    Remove-Item Env:_JAVA_OPTIONS -ErrorAction SilentlyContinue
    Remove-Item Env:DOTNET_ROOT -ErrorAction SilentlyContinue
    Set-Location $TestRoot

    & $Pinset --lang zh-CN | Out-Null
    if ((Get-Content -LiteralPath (Join-Path $env:PINSET_HOME 'settings.toml') -Raw) -notmatch 'language = "zh-CN"') {
        throw 'language setting was not persisted'
    }

    & $Pinset use "node@$GlobalVersion" --global
    if (-not ((& $Pinset list pnpm --available) -match '^pnpm@10\.')) {
        throw 'no pnpm 10 release was listed as available'
    }
    if (-not ((& $Pinset list bun --available) -contains "bun@$BunVersion")) {
        throw "bun@$BunVersion was not listed as available"
    }
    if (-not ((& $Pinset list go --available) -match '^go@\d+\.\d+\.\d+$')) {
        throw 'no supported Go release was listed as available'
    }
    if (-not ((& $Pinset list python --available) -match '^python@\d+\.\d+\.\d+\+\d{8} ')) {
        throw 'no supported Python release was listed as available'
    }
    if (-not ((& $Pinset list java --available) -match '^java@\d+\.\d+\.\d+(\.\d+)?\+\d+ temurin (lts|ga) ')) {
        throw 'no supported Eclipse Temurin JDK release was listed as available'
    }
    if (-not ((& $Pinset list rust --available) -match '^rust@\d+\.\d+\.\d+ stable \(\d{4}-\d{2}-\d{2}\)$')) {
        throw 'no supported stable Rust release was listed as available'
    }
    if (-not ((& $Pinset list dotnet --available) -match '^dotnet@\d+\.\d+\.\d+ (lts|sts) (active|maintenance) \(\d{4}-\d{2}-\d{2}\)$')) {
        throw 'no supported .NET SDK release was listed as available'
    }
    $FlutterReleases = @(& $Pinset list flutter --available)
    if (-not ($FlutterReleases -match '^flutter@\d+\.\d+\.\d+ dart@\d+\.\d+\.\d+ stable$')) {
        throw 'no supported Flutter release was listed as available'
    }
    $ProjectFlutterMatch = $FlutterReleases |
        Where-Object { $_ -match "^flutter@$([regex]::Escape($ProjectFlutterSelector))\.(\d+) " } |
        Select-Object -First 1
    $ProjectFlutterVersionMatch = [regex]::Match([string]$ProjectFlutterMatch, '^flutter@(\d+\.\d+\.\d+) ')
    if (-not $ProjectFlutterMatch -or -not $ProjectFlutterVersionMatch.Success) {
        throw "no Flutter release matched project selector '$ProjectFlutterSelector'"
    }
    $ProjectFlutterVersion = $ProjectFlutterVersionMatch.Groups[1].Value
    & $Pinset global "pnpm@$GlobalPnpmSelector"
    $GlobalPnpmVersion = ((& $Pinset exec -- pnpm --version) | Out-String).Trim()
    if ($GlobalPnpmVersion -notmatch '^11\.') {
        throw "global pnpm selector resolved to unexpected version '$GlobalPnpmVersion'"
    }
    & $Pinset use "bun@$BunVersion" --global
    & $Pinset global "go@$GlobalGoSelector"
    $GlobalGoOutput = ((& $Pinset exec -- go version) | Out-String).Trim()
    $GlobalGoMatch = [regex]::Match($GlobalGoOutput, '^go version go(\d+\.\d+\.\d+)')
    if (-not $GlobalGoMatch.Success) {
        throw "global Go returned an invalid version: '$GlobalGoOutput'"
    }
    $GlobalGoVersion = $GlobalGoMatch.Groups[1].Value
    & $Pinset global "python@$GlobalPythonSelector"
    $GlobalPythonVersion = ((& $Pinset exec -- python -c "import sys; print('.'.join(map(str, sys.version_info[:3])))") | Out-String).Trim()
    if ($GlobalPythonVersion -notmatch $VersionPattern) {
        throw "global Python returned an invalid version: '$GlobalPythonVersion'"
    }
    & $Pinset global "java@$GlobalJavaSelector"
    $GlobalJavaCurrent = ((& $Pinset --lang en current java) | Out-String).Trim()
    $GlobalJavaMatch = [regex]::Match($GlobalJavaCurrent, '^java ([^ ]+) installed')
    if (-not $GlobalJavaMatch.Success -or $GlobalJavaMatch.Groups[1].Value -notmatch '^\d+\.\d+\.\d+(\.\d+)?\+\d+$') {
        throw "global Java returned an invalid current selection: '$GlobalJavaCurrent'"
    }
    $GlobalJavaVersion = $GlobalJavaMatch.Groups[1].Value
    $JavaProbePath = Join-Path $TestRoot 'PinsetJavaProbe.java'
    Set-Content -LiteralPath $JavaProbePath -Value @'
public class PinsetJavaProbe {
    public static void main(String[] args) {
        System.out.println("pinset-java-ok");
        System.out.println("java.home=" + System.getProperty("java.home"));
        System.out.println("JAVA_HOME=" + System.getenv("JAVA_HOME"));
    }
}

'@
    & $Pinset exec -- javac $JavaProbePath
    $GlobalJavaProbe = @(& $Pinset exec -- java -cp $TestRoot PinsetJavaProbe)
    if ($GlobalJavaProbe -notcontains 'pinset-java-ok') {
        throw "global Java probe did not run: '$($GlobalJavaProbe -join ' | ') '"
    }
    $ExpectedJavaRoot = Join-Path $env:PINSET_HOME "installs\java\$GlobalJavaVersion"
    if (-not ($GlobalJavaProbe -match "^JAVA_HOME=$([regex]::Escape($ExpectedJavaRoot))")) {
        throw "global Java probe reported an unmanaged JAVA_HOME: '$($GlobalJavaProbe -join ' | ')'"
    }
    & $Pinset global "rust@$GlobalRustSelector"
    $GlobalRustOutput = ((& $Pinset exec -- rustc --version) | Out-String).Trim()
    $GlobalRustMatch = [regex]::Match($GlobalRustOutput, '^rustc (\d+\.\d+\.\d+)')
    if (-not $GlobalRustMatch.Success) {
        throw "global Rust returned an invalid version: '$GlobalRustOutput'"
    }
    $GlobalRustVersion = $GlobalRustMatch.Groups[1].Value
    $RustProbePath = Join-Path $TestRoot 'PinsetRustProbe.rs'
    Set-Content -LiteralPath $RustProbePath -Value 'fn main() { println!("pinset-rust-ok"); }' -NoNewline
    $RustProbeExe = Join-Path $TestRoot 'pinset-rust-probe.exe'
    & $Pinset exec -- rustc $RustProbePath -o $RustProbeExe
    Assert-ExactOutput 'pinset-rust-ok' (& $RustProbeExe) 'global Rust compile and run'
    & $Pinset global "dotnet@$GlobalDotnetSelector"
    $GlobalDotnetVersion = ((& $Pinset exec -- dotnet --version) | Out-String).Trim()
    if ($GlobalDotnetVersion -notmatch $VersionPattern) {
        throw "global .NET SDK returned an invalid version: '$GlobalDotnetVersion'"
    }
    $GlobalDotnetMajor = $GlobalDotnetVersion.Split('.')[0]
    $DotnetProbe = Join-Path $TestRoot 'dotnet-probe'
    New-Item -ItemType Directory -Path $DotnetProbe | Out-Null
    Set-Content -LiteralPath (Join-Path $DotnetProbe 'PinsetDotnetProbe.csproj') -Value @"
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net$GlobalDotnetMajor.0</TargetFramework>
  </PropertyGroup>
</Project>
"@
    Set-Content -LiteralPath (Join-Path $DotnetProbe 'Program.cs') -Value @'
Console.WriteLine("pinset-dotnet-ok");
Console.WriteLine($"DOTNET_ROOT={Environment.GetEnvironmentVariable("DOTNET_ROOT")}");
'@
    $GlobalDotnetProbe = @(& $Pinset exec -- dotnet run --project (Join-Path $DotnetProbe 'PinsetDotnetProbe.csproj'))
    if ($GlobalDotnetProbe -notcontains 'pinset-dotnet-ok') {
        throw "global .NET SDK probe did not run: '$($GlobalDotnetProbe -join ' | ')'"
    }
    $ExpectedDotnetRoot = Join-Path $env:PINSET_HOME "installs\dotnet\$GlobalDotnetVersion\windows-x86_64"
    if ($GlobalDotnetProbe -notcontains "DOTNET_ROOT=$ExpectedDotnetRoot") {
        throw "global .NET SDK probe reported an unmanaged DOTNET_ROOT: '$($GlobalDotnetProbe -join ' | ')'"
    }
    if (-not $SkipFlutterRuntime) {
        & $Pinset global "flutter@$GlobalFlutterSelector"
        $GlobalFlutterInfo = ConvertFrom-FlutterMachineOutput @(& $Pinset exec -- flutter --version --machine) 'global Flutter'
        $GlobalFlutterVersion = [string]$GlobalFlutterInfo.frameworkVersion
        $GlobalDartVersion = [string]$GlobalFlutterInfo.dartSdkVersion
        if ($GlobalFlutterVersion -notmatch $VersionPattern -or $GlobalDartVersion -notmatch $VersionPattern) {
            throw 'global Flutter returned invalid Flutter or Dart version metadata'
        }
        if ($ProjectFlutterVersion -eq $GlobalFlutterVersion) {
            throw 'project Flutter acceptance version must differ from the global version'
        }
    }
    Assert-ExactOutput "v$GlobalVersion" (& $Pinset exec -- node --version) 'global pinset exec node'
    Assert-VersionOutput (& $Pinset exec -- npm --version) 'global pinset exec npm'
    Assert-VersionOutput (& $Pinset exec -- npx --version) 'global pinset exec npx'
    Assert-VersionOutput (& $Pinset exec -- corepack --version) 'global pinset exec corepack'
    Assert-ExactOutput $GlobalPnpmVersion (& $Pinset exec -- pnpm --version) 'global pinset exec pnpm'
    Assert-ExactOutput $BunVersion (& $Pinset exec -- bun --version) 'global pinset exec bun'
    Assert-ExactOutput $BunVersion (& $Pinset exec -- bunx --version) 'global pinset exec bunx'
    if (((& $Pinset exec -- go version) | Out-String).Trim() -notmatch "^go version go$([regex]::Escape($GlobalGoVersion))") {
        throw 'global pinset exec Go returned an unexpected version'
    }
    Assert-ExactOutput 'local' (& $Pinset exec -- go env GOTOOLCHAIN) 'global pinset exec Go toolchain policy'
    Assert-ExactOutput $GlobalPythonVersion (& $Pinset exec -- python3 -c "import sys; print('.'.join(map(str, sys.version_info[:3])))") 'global pinset exec Python'
    Assert-PinsetPipRoutesToPython 'global pinset exec'
    if (((& $Pinset exec -- javac -version 2>&1) | Out-String).Trim() -notmatch '^javac ') {
        throw 'global pinset exec javac returned an invalid version'
    }
    if (((& $Pinset exec -- rustc --version) | Out-String).Trim() -notmatch "^rustc $([regex]::Escape($GlobalRustVersion))") {
        throw 'global pinset exec rustc returned an unexpected version'
    }
    if (((& $Pinset exec -- cargo --version) | Out-String).Trim() -notmatch '^cargo ') {
        throw 'global pinset exec cargo returned an invalid version'
    }
    if (((& $Pinset exec -- rustfmt --version) | Out-String).Trim() -notmatch '^rustfmt ') {
        throw 'global pinset exec rustfmt returned an invalid version'
    }
    Assert-ExactOutput $GlobalDotnetVersion (& $Pinset exec -- dotnet --version) 'global pinset exec dotnet'
    $GlobalGoRoot = ((& $Pinset exec -- go env GOROOT) | Out-String).Trim()
    if ($GlobalGoRoot -notlike "$(Join-Path $env:PINSET_HOME "installs\go\$GlobalGoVersion")*") {
        throw "global Go reported an unmanaged GOROOT: '$GlobalGoRoot'"
    }
    if (-not $SkipFlutterRuntime) {
        $GlobalFlutterPath = ((& $Pinset which flutter) | Out-String).Trim()
        $GlobalDartPath = ((& $Pinset which dart) | Out-String).Trim()
        if ((Split-Path -Parent $GlobalFlutterPath) -ne (Split-Path -Parent $GlobalDartPath)) {
            throw 'global Flutter and Dart resolved from different SDK directories'
        }
        $GlobalFlutterRoot = Split-Path -Parent (Split-Path -Parent $GlobalFlutterPath)
        $FlutterEnvironmentScript = Join-Path $TestRoot 'verify_flutter_env.dart'
        Set-Content -LiteralPath $FlutterEnvironmentScript -Value "import 'dart:io'; void main() => print(Platform.environment['FLUTTER_ROOT']);" -NoNewline
        Assert-ExactOutput $GlobalFlutterRoot (& $Pinset exec -- dart $FlutterEnvironmentScript) 'global Flutter root'
        if (((& $Pinset exec -- dart --version 2>&1) | Out-String).Trim() -notmatch "Dart SDK version: $([regex]::Escape($GlobalDartVersion))") {
            throw 'global bundled Dart returned an unexpected version'
        }
    }

    $InstalledEnvelope = ((& $Pinset list --json) | Out-String | ConvertFrom-Json)
    if ($InstalledEnvelope.schema -ne 1 -or $InstalledEnvelope.command -ne 'list' -or -not $InstalledEnvelope.ok) {
        throw 'list JSON did not use the schema 1 success envelope'
    }
    $InstalledProviders = @($InstalledEnvelope.data.versions)
    foreach ($Provider in @('node', 'pnpm', 'bun', 'go', 'python', 'java', 'rust', 'dotnet')) {
        if ($Provider -notin @($InstalledProviders.tool)) {
            throw "cross-provider list omitted $Provider"
        }
    }
    $RustCurrentEnvelope = ((& $Pinset current rust --json) | Out-String | ConvertFrom-Json)
    $RustCurrent = $RustCurrentEnvelope.data
    if ($RustCurrent.tool -ne 'rust' -or $RustCurrent.source -ne 'global' -or -not $RustCurrent.installed) {
        throw 'current rust JSON did not report the installed global toolchain'
    }
    $UninstallPreview = ((& $Pinset uninstall "node@$GlobalVersion" --force --dry-run --json) | Out-String | ConvertFrom-Json).data
    if (-not $UninstallPreview.dry_run -or $UninstallPreview.tool -ne 'node' -or @($UninstallPreview.targets).Count -eq 0) {
        throw 'Node uninstall dry-run returned an invalid plan'
    }
    $PrunePreview = ((& $Pinset prune --dry-run --json) | Out-String | ConvertFrom-Json).data
    if (-not $PrunePreview.dry_run -or @($PrunePreview.candidates).Count -ne 0 -or $PrunePreview.removed -ne 0) {
        throw 'prune dry-run did not protect every global runtime'
    }
    $CacheInfo = ((& $Pinset cache info --json) | Out-String | ConvertFrom-Json).data
    if ($CacheInfo.archives -lt 1 -or $CacheInfo.archive_bytes -le 0) {
        throw 'cache info did not report downloaded archives'
    }
    $NodeOutdated = @(((& $Pinset outdated node --global --json) | Out-String | ConvertFrom-Json).data.runtimes)
    if ($NodeOutdated.Count -ne 1 -or $NodeOutdated[0].tool -ne 'node' -or $NodeOutdated[0].scope -ne 'global') {
        throw 'global Node outdated JSON returned an invalid report'
    }
    foreach ($Shell in @('bash', 'zsh', 'fish', 'powershell')) {
        if (((& $Pinset completions $Shell) | Out-String) -notmatch 'node@') {
            throw "$Shell completion omitted Provider selections"
        }
    }

    $ShimDirectory = ((& $Pinset shim path) | Out-String).Trim()
    if (-not $ShimDirectory) {
        throw 'pinset shim path returned an empty path'
    }
    $env:PATH = "$ShimDirectory;$OriginalPath"
    Assert-ExactOutput "v$GlobalVersion" (& node --version) 'global direct node'
    Assert-VersionOutput (& npm --version) 'global direct npm'
    Assert-VersionOutput (& npx --version) 'global direct npx'
    Assert-VersionOutput (& corepack --version) 'global direct corepack'
    Assert-ExactOutput $GlobalPnpmVersion (& pnpm --version) 'global direct pnpm'
    Assert-ExactOutput $BunVersion (& bun --version) 'global direct bun'
    Assert-ExactOutput $BunVersion (& bunx --version) 'global direct bunx'
    if (((& go version) | Out-String).Trim() -notmatch "^go version go$([regex]::Escape($GlobalGoVersion))") {
        throw 'global direct Go returned an unexpected version'
    }
    Assert-ExactOutput 'local' (& go env GOTOOLCHAIN) 'global direct Go toolchain policy'
    Assert-ExactOutput $GlobalPythonVersion (& python -c "import sys; print('.'.join(map(str, sys.version_info[:3])))") 'global direct Python'
    Assert-ExactOutput $GlobalPythonVersion (& python3 -c "import sys; print('.'.join(map(str, sys.version_info[:3])))") 'global direct Python3'
    Assert-DirectPipRoutesToPython 'global direct'
    if (((& java -cp $TestRoot PinsetJavaProbe) | Out-String) -notmatch 'pinset-java-ok') {
        throw 'global direct Java probe did not run'
    }
    if (((& rustc --version) | Out-String).Trim() -notmatch "^rustc $([regex]::Escape($GlobalRustVersion))") {
        throw 'global direct rustc returned an unexpected version'
    }
    if (((& cargo --version) | Out-String).Trim() -notmatch '^cargo ') {
        throw 'global direct cargo returned an invalid version'
    }
    Assert-ExactOutput $GlobalDotnetVersion (& dotnet --version) 'global direct dotnet'
    if (-not $SkipFlutterRuntime) {
        $DirectGlobalFlutter = ConvertFrom-FlutterMachineOutput @(& flutter --version --machine) 'global direct Flutter'
        if ($DirectGlobalFlutter.frameworkVersion -ne $GlobalFlutterVersion) {
            throw 'global direct Flutter returned an unexpected version'
        }
        if (((& dart --version 2>&1) | Out-String).Trim() -notmatch "Dart SDK version: $([regex]::Escape($GlobalDartVersion))") {
            throw 'global direct Dart returned an unexpected version'
        }
        foreach ($mutation in @('upgrade', 'downgrade', 'channel')) {
            $MutationOutput = @(& flutter $mutation 2>&1 | ForEach-Object { $_.ToString() })
            if ($LASTEXITCODE -eq 0 -or ($MutationOutput -join "`n") -notmatch "refusing to run ``flutter $mutation`` against a Pinset-managed Flutter SDK") {
                throw "managed flutter $mutation was not blocked"
            }
        }
    }

    $GlobalPythonCurrent = ((& $Pinset --lang en current python) | Out-String).Trim()
    $GlobalPythonDistributionMatch = [regex]::Match($GlobalPythonCurrent, '^python ([^ ]+) installed')
    if (-not $GlobalPythonDistributionMatch.Success) {
        throw "global Python selection could not be read: '$GlobalPythonCurrent'"
    }
    $TraditionalImport = Join-Path $TestRoot 'traditional-import'
    New-Item -ItemType Directory -Path $TraditionalImport | Out-Null
    Set-Location $TraditionalImport
    $ToolVersions = @(
        "node $GlobalVersion"
        "pnpm $GlobalPnpmVersion"
        "bun $BunVersion"
        "go $GlobalGoVersion"
        "python $($GlobalPythonDistributionMatch.Groups[1].Value)"
        "java $GlobalJavaVersion"
        "rust $GlobalRustVersion"
        "dotnet $GlobalDotnetVersion"
    )
    if (-not $SkipFlutterRuntime) {
        $ToolVersions += "flutter $GlobalFlutterVersion"
    }
    Set-Content -LiteralPath (Join-Path $TraditionalImport '.tool-versions') -Value $ToolVersions
    Set-Content -LiteralPath (Join-Path $TraditionalImport 'package.json') -Value "{`"engines`":{`"node`":`">=$GlobalVersion`"}}" -NoNewline
    $DetectReport = ((& $Pinset detect --json) | Out-String) | ConvertFrom-Json
    if ($DetectReport.schema -ne 1 -or $DetectReport.command -ne 'detect' -or
        -not $DetectReport.ok -or -not $DetectReport.data.can_import) {
        throw 'traditional configuration detect report was not importable'
    }
    & $Pinset import
    if (-not (Test-Path -LiteralPath (Join-Path $TraditionalImport 'pinset.toml') -PathType Leaf) -or
        -not (Test-Path -LiteralPath (Join-Path $TraditionalImport 'pinset.lock') -PathType Leaf)) {
        throw 'traditional import did not create Pinset project state'
    }
    if (((& $Pinset --lang en current node) | Out-String).Trim() -notmatch "Node.js $([regex]::Escape($GlobalVersion))") {
        throw 'traditional import did not select Node.js'
    }
    if (((& $Pinset which node) | Out-String).Trim() -notmatch [regex]::Escape("installs\node\$GlobalVersion")) {
        throw 'traditional import did not resolve the managed Node.js command'
    }
    $TraditionalVenv = Join-Path $TraditionalImport '.venv'
    if (-not (Test-Path -LiteralPath (Join-Path $TraditionalVenv '.pinset-venv.toml') -PathType Leaf)) {
        throw 'traditional import did not create the Python environment'
    }
    Assert-ExactOutput $TraditionalVenv (& $Pinset exec -- python -c "import os; print(os.environ['VIRTUAL_ENV'])") 'traditional import Python environment'
    Set-Location $TestRoot
    & $Pinset cache clean

    $Project = Join-Path $TestRoot 'project'
    New-Item -ItemType Directory -Path $Project | Out-Null
    Set-Location $Project
    Set-Content -LiteralPath (Join-Path $Project 'package.json') -Value '{"private":true}' -NoNewline
    & $Pinset init
    & $Pinset use "node@$ProjectVersion"
    & $Pinset use "pnpm@$ProjectPnpmSelector"
    $ProjectPnpmVersion = ((& pnpm --version) | Out-String).Trim()
    if ($ProjectPnpmVersion -notmatch '^10\.') {
        throw "project pnpm selector resolved to unexpected version '$ProjectPnpmVersion'"
    }
    & $Pinset uninstall "pnpm@$ProjectPnpmVersion" --force
    & $Pinset use "bun@$ProjectBunSelector"
    $ProjectBunVersion = ((& bun --version) | Out-String).Trim()
    if ($ProjectBunVersion -notmatch '^1\.2\.') {
        throw "project Bun selector resolved to unexpected version '$ProjectBunVersion'"
    }
    & $Pinset use "go@$ProjectGoSelector"
    $ProjectGoOutput = ((& go version) | Out-String).Trim()
    $ProjectGoMatch = [regex]::Match($ProjectGoOutput, '^go version go(\d+\.\d+\.\d+)')
    if (-not $ProjectGoMatch.Success -or $ProjectGoMatch.Groups[1].Value -notmatch "^$([regex]::Escape($ProjectGoSelector))\.") {
        throw "project Go selector resolved to unexpected version '$ProjectGoOutput'"
    }
    $ProjectGoVersion = $ProjectGoMatch.Groups[1].Value
    & $Pinset use "python@$ProjectPythonSelector"
    $ProjectPythonVersion = ((& python -c "import sys; print('.'.join(map(str, sys.version_info[:3])))") | Out-String).Trim()
    if ($ProjectPythonVersion -notmatch "^$([regex]::Escape($ProjectPythonSelector))\.") {
        throw "project Python selector resolved to unexpected version '$ProjectPythonVersion'"
    }
    & $Pinset use "java@$GlobalJavaVersion" --no-install
    & $Pinset use "rust@$GlobalRustVersion" --no-install
    & $Pinset use "dotnet@$GlobalDotnetVersion" --no-install
    if (-not $SkipFlutterRuntime) {
        & $Pinset use "flutter@$ProjectFlutterVersion" --no-install
    }
    & $Pinset install --locked
    if (-not $SkipFlutterRuntime) {
        $ProjectFlutterInfo = ConvertFrom-FlutterMachineOutput @(& flutter --version --machine) 'project Flutter'
        if ($ProjectFlutterInfo.frameworkVersion -ne $ProjectFlutterVersion) {
            throw "project Flutter returned unexpected version '$($ProjectFlutterInfo.frameworkVersion)'"
        }
        $ProjectDartVersion = [string]$ProjectFlutterInfo.dartSdkVersion
    }
    Assert-ExactOutput "v$ProjectVersion" (& node --version) 'project direct node'
    Assert-VersionOutput (& npm --version) 'project direct npm'
    Assert-VersionOutput (& npx --version) 'project direct npx'
    Assert-VersionOutput (& corepack --version) 'project direct corepack'
    Assert-ExactOutput $ProjectPnpmVersion (& pnpm --version) 'project direct pnpm'
    Assert-ExactOutput $ProjectBunVersion (& bun --version) 'project direct bun'
    Assert-ExactOutput 'local' (& go env GOTOOLCHAIN) 'project direct Go toolchain policy'
    $ProjectVenv = Join-Path $Project '.venv'
    Assert-ExactOutput $ProjectVenv (& python -c "import sys; print(sys.prefix)") 'project direct Python environment'
    Assert-ExactOutput $ProjectVenv (& $Pinset exec -- python3 -c "import os; print(os.environ['VIRTUAL_ENV'])") 'project pinset exec Python environment'
    Assert-PinsetPipRoutesToPython 'project pinset exec'
    Assert-DirectPipRoutesToPython 'project direct'
    if (-not (Test-Path -LiteralPath (Join-Path $ProjectVenv '.pinset-venv.toml') -PathType Leaf)) {
        throw 'project Python environment has no Pinset ownership marker'
    }
    & javac $JavaProbePath
    $ProjectJavaProbe = @(& java -cp $TestRoot PinsetJavaProbe)
    if ($ProjectJavaProbe -notcontains 'pinset-java-ok' -or
        -not ($ProjectJavaProbe -match "^JAVA_HOME=$([regex]::Escape($ExpectedJavaRoot))")) {
        throw "project Java probe did not use the selected JDK: '$($ProjectJavaProbe -join ' | ')'"
    }
    if (((& $Pinset --lang en current java) | Out-String).Trim() -notmatch "^java $([regex]::Escape($GlobalJavaVersion)) installed") {
        throw 'project Java selection was not reported as installed'
    }
    if (((& $Pinset --lang en current rustc) | Out-String).Trim() -notmatch "^rust $([regex]::Escape($GlobalRustVersion)) installed") {
        throw 'project Rust selection was not reported as installed'
    }
    $ProjectRustProbeExe = Join-Path $Project 'pinset-rust-probe.exe'
    & rustc $RustProbePath -o $ProjectRustProbeExe
    Assert-ExactOutput 'pinset-rust-ok' (& $ProjectRustProbeExe) 'project Rust compile and run'
    $ProjectDotnetProbe = @(& dotnet run --project (Join-Path $DotnetProbe 'PinsetDotnetProbe.csproj'))
    if ($ProjectDotnetProbe -notcontains 'pinset-dotnet-ok') {
        throw "project .NET SDK probe did not run: '$($ProjectDotnetProbe -join ' | ')'"
    }
    if (((& $Pinset --lang en current dotnet) | Out-String).Trim() -notmatch "^dotnet $([regex]::Escape($GlobalDotnetVersion)) installed") {
        throw 'project .NET SDK selection was not reported as installed'
    }
    $ProjectGoRoot = ((& go env GOROOT) | Out-String).Trim()
    if ($ProjectGoRoot -notlike "$(Join-Path $env:PINSET_HOME "installs\go\$ProjectGoVersion")*") {
        throw "project Go reported an unmanaged GOROOT: '$ProjectGoRoot'"
    }
    if (-not $SkipFlutterRuntime) {
        $ProjectFlutterPath = ((& $Pinset which flutter) | Out-String).Trim()
        $ProjectDartPath = ((& $Pinset which dart) | Out-String).Trim()
        if ((Split-Path -Parent $ProjectFlutterPath) -ne (Split-Path -Parent $ProjectDartPath)) {
            throw 'project Flutter and Dart resolved from different SDK directories'
        }
        $ProjectFlutterRoot = Split-Path -Parent (Split-Path -Parent $ProjectFlutterPath)
        Assert-ExactOutput $ProjectFlutterRoot (& dart $FlutterEnvironmentScript) 'project Flutter root'
        if (((& dart --version 2>&1) | Out-String).Trim() -notmatch "Dart SDK version: $([regex]::Escape($ProjectDartVersion))") {
            throw 'project bundled Dart returned an unexpected version'
        }
    }
    & $Pinset cache clean
    $LockedReuse = ((& $Pinset install --locked) | Out-String)
    if ($LockedReuse -notmatch "java@$([regex]::Escape($GlobalJavaVersion)) is already installed") {
        throw 'locked Java install did not reuse the completed JDK'
    }
    if ($LockedReuse -notmatch "rust@$([regex]::Escape($GlobalRustVersion)) is already installed") {
        throw 'locked Rust install did not reuse the completed toolchain'
    }
    if ($LockedReuse -notmatch "dotnet@$([regex]::Escape($GlobalDotnetVersion)) is already installed") {
        throw 'locked .NET SDK install did not reuse the completed SDK'
    }
    if (-not $SkipFlutterRuntime -and $LockedReuse -notmatch "flutter@$([regex]::Escape($ProjectFlutterVersion)) is already installed") {
        throw 'locked Flutter install did not reuse the completed SDK'
    }
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
    Assert-ExactOutput $GlobalPnpmVersion (& pnpm --version) 'restored global direct pnpm'
    Assert-ExactOutput $BunVersion (& bun --version) 'restored global direct bun'
    if (((& go version) | Out-String).Trim() -notmatch "^go version go$([regex]::Escape($GlobalGoVersion))") {
        throw 'restored global direct Go returned an unexpected version'
    }
    Assert-ExactOutput 'local' (& go env GOTOOLCHAIN) 'restored global Go toolchain policy'
    Assert-ExactOutput $GlobalPythonVersion (& python -c "import sys; print('.'.join(map(str, sys.version_info[:3])))") 'restored global Python'
    Assert-DirectPipRoutesToPython 'restored global direct'
    if (((& java -cp $TestRoot PinsetJavaProbe) | Out-String) -notmatch 'pinset-java-ok') {
        throw 'restored global Java probe did not run'
    }
    if (((& rustc --version) | Out-String).Trim() -notmatch "^rustc $([regex]::Escape($GlobalRustVersion))") {
        throw 'restored global rustc returned an unexpected version'
    }
    if (((& cargo --version) | Out-String).Trim() -notmatch '^cargo ') {
        throw 'restored global cargo returned an invalid version'
    }
    Assert-ExactOutput $GlobalDotnetVersion (& dotnet --version) 'restored global dotnet'
    if (-not $SkipFlutterRuntime) {
        $RestoredFlutter = ConvertFrom-FlutterMachineOutput @(& flutter --version --machine) 'restored global Flutter'
        if ($RestoredFlutter.frameworkVersion -ne $GlobalFlutterVersion) {
            throw 'restored global Flutter returned an unexpected version'
        }
        if (((& dart --version 2>&1) | Out-String).Trim() -notmatch "Dart SDK version: $([regex]::Escape($GlobalDartVersion))") {
            throw 'restored global Dart returned an unexpected version'
        }
    }
    if ($SkipFlutterRuntime) {
        Write-Host 'Windows real Node, pnpm, Bun, Go, Python, Java, Rust and .NET acceptance passed; Flutter runtime download skipped'
    }
    else {
        Write-Host 'Windows real Node, pnpm, Bun, Go, Python, Java, Rust, .NET and Flutter acceptance passed'
    }
}
finally {
    $env:PATH = $OriginalPath
    Set-Location ([System.IO.Path]::GetTempPath())
    if (Test-Path -LiteralPath $TestRoot) {
        Remove-Item -LiteralPath $TestRoot -Recurse -Force
    }
}
