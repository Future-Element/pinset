[CmdletBinding()]
param([switch]$Yes,[switch]$Plan,[string]$PinsetHome=$env:PINSET_HOME)
$ErrorActionPreference='Stop'
if (-not $PinsetHome) { $PinsetHome=Join-Path $env:USERPROFILE '.pinset' }
if (-not [IO.Path]::IsPathRooted($PinsetHome)) { throw 'Pinset home must be absolute' }
$base=[IO.Path]::GetFullPath($PinsetHome).TrimEnd([IO.Path]::DirectorySeparatorChar)
$target=[IO.Path]::GetFullPath((Join-Path $base 'v3'))
if ($base -eq [IO.Path]::GetPathRoot($base) -or $base -eq [IO.Path]::GetFullPath($env:USERPROFILE)) { throw 'Unsafe Pinset home' }
if (-not $target.StartsWith($base+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Pinset 3 path escaped its parent' }
if (-not (Test-Path -LiteralPath $target)) { Write-Output "No Pinset 3 data: $target"; return }
$item=Get-Item -LiteralPath $target -Force
if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or -not $item.PSIsContainer) { throw 'Refusing a linked or invalid v3 directory' }
$marker=Get-Content -LiteralPath (Join-Path $target '.pinset-home.json') -Raw | ConvertFrom-Json
if ($marker.protocol -ne 'pinset/3') { throw 'Directory is not owned by Pinset 3' }
Write-Output "Remove Pinset 3 data and command entries: $target"
if ($Plan) { return }
if (-not $Yes -and (Read-Host 'Continue? [y/N]') -notmatch '^[yY]$') { return }
Remove-Item -LiteralPath $target -Recurse -Force
