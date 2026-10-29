# Windows release validation

Pinset 3.1.1 is a local release candidate. Its Windows CLI and shim must both use
the static MSVC C runtime. These instructions prepare validation; they do not
record a successful Windows run or authorize publication. Keep 3.1.0 assets,
channel manifests and the existing WinGet PR unchanged.

## Build and inspect the candidate

On an authorized Windows x64 build host with the pinned toolchain and MSVC tools:

```powershell
rustup toolchain install 1.97.1 --profile minimal --target x86_64-pc-windows-msvc
cargo +1.97.1 build --release --locked --target x86_64-pc-windows-msvc -p pinset-cli -p pinset-shim
if ($LASTEXITCODE -ne 0) { throw 'Windows release build failed' }
$BuildRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
$WindowsRelease = Join-Path $BuildRoot 'x86_64-pc-windows-msvc/release'
python scripts/check_windows_imports.py --binary-dir $WindowsRelease --output output/windows-imports.json
if ($LASTEXITCODE -ne 0) { throw 'PE import inspection failed' }
$env:PINSET_RELEASE_TARGET = 'windows-x86_64'
$env:PINSET_RELEASE_VERSION = 'v3.1.1'
python scripts/package_release.py
if ($LASTEXITCODE -ne 0) { throw 'Packaging failed' }
Get-FileHash dist/pinset-v3.1.1-windows-x86_64.zip -Algorithm SHA256
```

The checked-in `.cargo/config.toml` enables `+crt-static` only for
`x86_64-pc-windows-msvc`. Always pass `--target`, including for Windows tests and
lint, to separate host build scripts/proc macros from target compilation. Do not
override the target's Rust flags. Build scripts for native dependencies must see
`crt-static`; preserve verbose build output when investigating a mismatch. A
successful build is insufficient: inspect **both** executable import tables.

Extract the ZIP into a new directory and repeat the import check on that
directory, preserving its JSON and the SHA-256 of the ZIP and both executables.
The checker covers normal and delay DLL imports and rejects dynamic VC/UCRT
imports and DLLs outside its explicit OS allowlist. Review the complete DLL list;
extend that allowlist only after identifying a genuine supported Windows OS DLL.
This does not detect every possible runtime `LoadLibrary` call, and it does not
prove native process startup. On Windows 10 and later, `ucrtbase.dll` and the UCRT
API-set contracts are OS components; `msvcrt.dll` and `msvcp_win.dll` can also be
system runtimes. The checker distinguishes these from VC runtime imports and
ordinary allowed Windows DLLs. It rejects direct UCRT/system-CRT imports because
this candidate requires static CRT linkage, not because their presence proves a
missing VC++ Redistributable. `ucrtbased.dll` is the debug UCRT. Even a
`vcruntime140.dll` import proves a dynamic dependency, not that a particular host
is missing it. This inspection does not reject or remove system files, and it
does not recursively scan the dependencies of allowed Windows OS DLLs.

The release workflow only compiles and packages. Keep this inspection and the
following runtime checks separate from publication. `verify-platforms.yml` is an
independently dispatched native smoke. In source mode its Windows job builds the
release pair, checks imports, packages those executables, and preserves the ZIP,
SBOM and import report as a workflow artifact for inspection. Its Windows runner
may already contain VC++ runtimes, so its success cannot supply the clean-machine
evidence below or replace the required matching local Docker release report.
`scripts/tests/windows_checks.py` checks a Windows GNU target and cannot validate
MSVC CRT linkage. `scripts/tests/ci_platform.py` is deliberately restricted to an
explicit GitHub Actions dispatch; do not impersonate that environment locally.

## Prepare an uncontaminated Windows x64 image

Use a newly provisioned, authorized disposable Windows x64 VM or Windows Sandbox
with a documented OS image/build and a fresh snapshot. Keep Defender, SmartScreen,
archive scanning, hash verification and organization policies enabled. Do not use
an existing personal workstation or the unauthorized ROG host. Do not uninstall
any user's runtimes, change security policies, create credentials, or install
Visual Studio/build tools into the image.

The image owner must provide an already working WinGet client and, for local
manifest tests, an existing authorized `LocalManifestFiles` configuration. Record
`winget --info` and `winget settings export` where supported. If local manifests
are blocked, record the exact policy error and leave the WinGet installation
stage blocked; do not change that setting to complete this procedure. A Sandbox
without WinGet can still provide archive startup/routing evidence, but cannot
provide WinGet installation evidence.

Record the image provenance, Windows version/architecture, installed App Installer
package/dependencies, process PATH, and installed-package inventory before and
after the test. Record both uninstall registry views and these read-only checks:

```powershell
Get-ComputerInfo | Select-Object WindowsProductName, WindowsVersion, OsBuildNumber, OsArchitecture
Get-AppxPackage Microsoft.DesktopAppInstaller | Select-Object Name, Version, Dependencies
$env:PATH
Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*' -ErrorAction SilentlyContinue |
  Select-Object DisplayName, DisplayVersion, InstallLocation
Get-ChildItem "$env:WINDIR\System32\vcruntime*.dll", "$env:WINDIR\System32\msvcp*.dll",
  "$env:WINDIR\SysWOW64\vcruntime*.dll", "$env:WINDIR\SysWOW64\msvcp*.dll" -ErrorAction SilentlyContinue
```

No VC++ Redistributable may be installed, no VC runtime DLL may be supplied beside
the candidate executables, and the process search path must not expose a build
toolchain/runtime directory. Registry/file checks support the image provenance;
they alone are not proof that an arbitrary existing machine is clean. Python or
other inspection tools may bundle app-local runtimes: run PE inspection on the
build/analysis host and transfer its hash-bound report, or document those tools'
isolation from the candidate's DLL search path. Do not install a Redistributable
to make the test harness work. Stop and choose a suitable fresh image if needed.

Transfer only the exact candidate ZIP and its reviewable evidence. Verify its
SHA-256 after transfer, then extract it to an empty path containing spaces. The
archive must contain exactly `pinset.exe` and `pinset-shim.exe` at its root. Record
their hashes and confirm that they match the inspected pair.

## Start both executables and exercise managed routing

Use the following PowerShell function in the disposable image. Set `$Evidence`
to an empty evidence directory and `$CandidateCli` to the extracted `pinset.exe`.
Use a fresh isolated Pinset home per invocation. Run this once for the extracted
pair and again for the WinGet-installed CLI below. The only SDK download is the
exact Terraform 1.9.0 artifact from its official HTTPS origin; its generic Provider
path exercises the managed shim and its private, version-matched CLI broker.

```powershell
$Evidence = 'C:\Pinset evidence'
$CandidateCli = 'C:\Pinset candidate\pinset.exe'
New-Item -ItemType Directory -Path $Evidence -ErrorAction Stop | Out-Null
Start-Transcript -Path (Join-Path $Evidence 'transcript.txt')

function Test-PinsetPair([string]$Cli, [string]$Label) {
    $run = Join-Path $Evidence $Label
    $project = Join-Path $run 'project'
    New-Item -ItemType Directory -Path $project -ErrorAction Stop | Out-Null
    $priorHome = $env:PINSET_HOME
    $priorNoEnv = $env:PINSET_NO_ENV
    $env:PINSET_HOME = Join-Path $run 'home'
    $env:PINSET_NO_ENV = '1'
    try {
        $infoText = & $Cli --json self info
        if ($LASTEXITCODE -ne 0) { throw "self info failed: $LASTEXITCODE" }
        $infoText | Set-Content (Join-Path $run 'self-info.json')
        $info = $infoText | ConvertFrom-Json
        if ($info.version -ne '3.1.1' -or $info.platform -ne 'windows-x86_64') {
            throw 'Unexpected version or native target'
        }
        $realCli = [string]$info.cli
        $shim = Join-Path (Split-Path $realCli) 'pinset-shim.exe'
        Get-FileHash $realCli, $shim -Algorithm SHA256 |
            ConvertTo-Json | Set-Content (Join-Path $run 'pair-hashes.json')
        $guard = Start-Process -FilePath $shim -Wait -PassThru -NoNewWindow `
            -RedirectStandardOutput (Join-Path $run 'shim.stdout.txt') `
            -RedirectStandardError (Join-Path $run 'shim.stderr.txt')
        $guard.ExitCode | Set-Content (Join-Path $run 'shim.exit.txt')
        if ($guard.ExitCode -ne 1 -or
            (Get-Content -Raw (Join-Path $run 'shim.stderr.txt')) -notmatch 'PINSET_COMMAND_INVALID') {
            throw 'Shim failed to reach its native entry guard'
        }
        & $Cli -C $project init
        if ($LASTEXITCODE -ne 0) { throw "init failed: $LASTEXITCODE" }
        @'

[generic.terraform]
url = "https://releases.hashicorp.com/terraform/{version}/terraform_{version}_{os}_{arch}.zip"
format = "zip"
executable = "terraform{exe}"
os = { macos = "darwin" }
arch = { x86_64 = "amd64", aarch64 = "arm64" }
'@ | Add-Content -Encoding UTF8 (Join-Path $project '.pinset\config.toml')
        & $Cli -C $project use terraform@1.9.0
        if ($LASTEXITCODE -ne 0) { throw "use failed: $LASTEXITCODE" }
        $direct = & $Cli -C $project exec --no-env -- terraform --version
        if ($LASTEXITCODE -ne 0 -or $direct[0] -ne 'Terraform v1.9.0') { throw 'exec routing failed' }
        Push-Location $project
        try {
            $routed = & (Join-Path $env:PINSET_HOME 'v3\bin\terraform.exe') --version
            if ($LASTEXITCODE -ne 0 -or $routed[0] -ne 'Terraform v1.9.0') { throw 'Managed shim/broker routing failed' }
        } finally { Pop-Location }
        $checkText = & $Cli -C $project check --deep --probe --format json
        if ($LASTEXITCODE -ne 0) { throw "Deep probe failed: $LASTEXITCODE" }
        $checkText | Set-Content (Join-Path $run 'check.json')
        if (($checkText | ConvertFrom-Json).passed -ne $true) { throw 'Structured check failed' }
        $lock = Join-Path $project '.pinset\lock.toml'
        $before = (Get-FileHash $lock -Algorithm SHA256).Hash
        & $Cli -C $project install --offline
        if ($LASTEXITCODE -ne 0 -or (Get-FileHash $lock -Algorithm SHA256).Hash -ne $before) {
            throw 'Offline restoration failed or changed the lock'
        }
        Copy-Item $lock (Join-Path $run 'lock.toml')
    } finally {
        $env:PINSET_HOME = $priorHome
        $env:PINSET_NO_ENV = $priorNoEnv
    }
}

Test-PinsetPair $CandidateCli 'archive'
```

Preserve the command output and every nonzero status, including its hexadecimal
representation for a Windows loader failure such as `0xC0000135`. Exit `1` plus
`PINSET_COMMAND_INVALID` for a bare shim is the expected entry guard; a loader
failure is not. A matching version string alone does not validate shim routing.
Compare the `self-info` pair hashes with the archive/import report. Preserve the
lock's exact platform URL and digest, both Terraform outputs and deep-probe report.
Do not invoke identity/profile/trust commands or use real project secrets.

## Exercise WinGet without dependency installation

Prepare three **scratch** 3.1.1 manifest files outside `packaging/` by copying the
3.1.0 manifest set. Update the version, license/release URLs, installer URL and
SHA-256 for the exact candidate. Keep both `NestedInstallerFiles` entries adjacent,
with their original names and command aliases. Remove the VC++ package dependency
from this scratch candidate only after the import inspection passes. Preserve the
manifest bytes in the evidence bundle. Do not submit or alter the existing PR.

The installer URL must already serve the exact candidate ZIP through an authorized
HTTPS endpoint. A not-yet-published `v3.1.1` GitHub URL is not a usable endpoint;
do not publish a release merely to run this test. If no endpoint is available,
record WinGet installation as blocked while completing archive validation.

Set `$Manifest` to the scratch multi-file manifest directory. Confirm no Pinset
package is already installed, then run:

```powershell
$Manifest = 'C:\Pinset candidate manifest\3.1.1'
winget --info
winget validate --manifest $Manifest --disable-interactivity
if ($LASTEXITCODE -ne 0) { throw "Manifest validation failed: $LASTEXITCODE" }
winget install --manifest $Manifest --architecture x64 --scope user --skip-dependencies `
    --accept-package-agreements --accept-source-agreements --disable-interactivity --verbose-logs
if ($LASTEXITCODE -ne 0) { throw "WinGet install failed: $LASTEXITCODE" }
winget list --id FutureElement.Pinset --exact --disable-interactivity
if ($LASTEXITCODE -ne 0) { throw "Installed registration missing: $LASTEXITCODE" }
$wingetCli = Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Links\pinset.exe'
Test-PinsetPair $wingetCli 'winget'
winget uninstall --manifest $Manifest --scope user --disable-interactivity --verbose-logs
if ($LASTEXITCODE -ne 0) { throw "WinGet uninstall failed: $LASTEXITCODE" }
Stop-Transcript
```

Compare the installed real executable hashes with the inspected archive. Inspect
WinGet's verbose logs for the actual package directory and aliases; if this client
uses another documented link path, pass that recorded link to `Test-PinsetPair`.
After uninstall, verify the registration, both aliases and the recorded package
directory have been removed. Preserve the before/after inventories and WinGet logs
from `%LOCALAPPDATA%\Packages\Microsoft.DesktopAppInstaller_8wekyb3d8bbwe\LocalState\DiagOutputDir`.
Pinset's isolated test home is separate application data; its remaining presence
does not imply that WinGet failed to remove the portable package.

`--skip-dependencies` intentionally reproduces the upstream dependency boundary.
Do not use hash or malware-scan bypass flags. If Defender returns `0x8050111c`,
retain that log and report the scan failure independently; static CRT does not
establish that the Defender concurrency issue or all upstream validation is fixed.

WinGet command behavior is documented by Microsoft: [install and local manifests](https://learn.microsoft.com/en-us/windows/package-manager/winget/install),
[manifest validation](https://learn.microsoft.com/en-us/windows/package-manager/winget/validate),
and [uninstall](https://learn.microsoft.com/en-us/windows/package-manager/winget/uninstall).

## Required result record

Record source commit plus local diff/fingerprint, pinned compiler/target and build
command, archive and both executable hashes, full normal/delay import report,
clean-image provenance and inventories, native startup/guard/routing/check logs,
scratch manifest files, WinGet version/install/uninstall logs, and explicit failed,
blocked or unrun stages. Report Mac tests, GNU cross compilation, hosted-runner
smoke, clean Windows archive behavior and WinGet lifecycle as separate results.
The Windows and WinGet stages remain **unverified** until these artifacts exist.
Publication still requires the project release gate and the user's confirmation.
