#requires -Version 5.1
<#
.SYNOPSIS
Completely remove Pinset, its SDKs, caches, credentials and shell integration.
.EXAMPLE
$source = curl.exe -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/uninstall.ps1 | Out-String
if ($LASTEXITCODE -ne 0) { throw 'Download failed' }; & ([scriptblock]::Create($source)) -Yes
.DESCRIPTION
Registered projects and explicit -Project roots are included. Machine settings
require elevation. Unrecognized data is retained and reported as incomplete.
#>
[CmdletBinding()]
param([switch]$Yes, [switch]$Plan, [string[]]$PinsetHome=@(), [string[]]$InstallDir=@(), [string[]]$Project=@())
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest

# Small adapters keep destructive integration tests isolated from real settings.
function Get-PinsetEnvironment([string]$Scope) { [Environment]::GetEnvironmentVariables([EnvironmentVariableTarget]$Scope) }
function Set-PinsetEnvironment([string]$Name,$Value,[string]$Scope) { [Environment]::SetEnvironmentVariable($Name,$Value,[EnvironmentVariableTarget]$Scope) }
function Test-PinsetAdministrator {
    $identity=[Security.Principal.WindowsIdentity]::GetCurrent()
    try { ([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator) }
    finally { $identity.Dispose() }
}
function Get-PinsetRunningProcesses {
    # Process image paths are sufficient to detect resident hard-linked shims.
    # Do not collect command lines: those can contain credentials.
    foreach($process in @(Get-Process -ErrorAction SilentlyContinue)) {
        try {
            $image=$process.Path
            if($image) { [pscustomobject]@{Name=$process.ProcessName; Id=$process.Id; Path=$image} }
        } catch { }
    }
}
function Get-PinsetFullPath([string]$Path) {
    $Path=[Environment]::ExpandEnvironmentVariables($Path)
    if ($Path.StartsWith('\\?\UNC\',[StringComparison]::OrdinalIgnoreCase)) { $Path='\\'+$Path.Substring(8) }
    elseif ($Path.StartsWith('\\?\')) { $Path=$Path.Substring(4) }
    if (-not $Path -or $Path -notmatch '^(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+(?:\\|$))') { throw "Expected absolute Windows path: $Path" }
    [IO.Path]::GetFullPath($Path).TrimEnd('\','/')
}
function Test-PinsetUnder([string]$Path,[string]$Root) {
    $Path.Equals($Root,[StringComparison]::OrdinalIgnoreCase) -or $Path.StartsWith($Root+'\',[StringComparison]::OrdinalIgnoreCase)
}
function Assert-PinsetPath([string]$Path,[switch]$AllowLeafLink) {
    $full=Get-PinsetFullPath $Path
    if ($full -eq [IO.Path]::GetPathRoot($full).TrimEnd('\','/')) { throw "Refusing filesystem root: $full" }
    $cursor=$full; $leaf=$true
    while ($cursor) {
        if (Test-Path -LiteralPath $cursor) {
            $item=Get-Item -LiteralPath $cursor -Force
            if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -and -not ($leaf -and $AllowLeafLink)) { throw "Refusing link/junction in path: $cursor" }
        }
        $cursor=Split-Path -Parent $cursor; $leaf=$false
    }
    $full
}
function Assert-PinsetDeletionRoot([string]$Path) {
    $full=Assert-PinsetPath $Path
    foreach ($protected in @($env:USERPROFILE,$env:WINDIR,$env:ProgramFiles,${env:ProgramFiles(x86)},$env:APPDATA,$env:LOCALAPPDATA,$env:TEMP)) {
        if ($protected -and (Test-PinsetUnder (Get-PinsetFullPath $protected) $full)) { throw "Refusing protected directory or ancestor: $full" }
    }
    $full
}
function Remove-PinsetNode([string]$Path,[bool]$Directory,[bool]$Reparse) {
    if($Reparse) {
        if($Directory) {[IO.Directory]::Delete($Path)} else {[IO.File]::Delete($Path)}
    } else {Remove-Item -LiteralPath $Path -Force}
}
function Invoke-PinsetDeleteNode([string]$Path,[bool]$Directory,[bool]$Reparse) {
    for($attempt=1;$attempt -le 3;$attempt++) {
        try {Remove-PinsetNode $Path $Directory $Reparse; return $true}
        catch {
            if($attempt -lt 3) {Start-Sleep -Milliseconds 100}
            else {
                $message="Cannot remove '$Path': $($_.Exception.Message) Close applications using Pinset runtimes and retry; permissions or a running hard-linked shim can prevent deletion."
                $script:failures.Add($message); Write-Warning $message; return $false
            }
        }
    }
}
function Save-PinsetRecoveryMetadata([string]$Path,[string]$Boundary) {
    $item=Get-Item -LiteralPath $Path -Force
    $candidates=@($Path)
    if($item.PSIsContainer) {
        $candidates=@('projects.json','identities.toml','global.toml','global.lock' | ForEach-Object {Join-Path $Path $_})
        $registry=Join-Path $Path 'projects'
        if(Test-Path -LiteralPath $registry -PathType Container) {[void](Assert-PinsetPath $registry); $candidates+=@(Get-ChildItem -LiteralPath $registry -Filter '*.toml' -File | ForEach-Object {$_.FullName})}
    }
    foreach($candidate in $candidates) {
        if(Test-Path -LiteralPath $candidate -PathType Leaf) {
            [void](Assert-PinsetPath $candidate)
            $script:recoverySnapshots[$candidate]=@{Boundary=$Boundary;Bytes=[IO.File]::ReadAllBytes($candidate)}
        }
    }
}
function Restore-PinsetRecoveryMetadata {
    foreach($entry in $script:recoverySnapshots.GetEnumerator()) {
        $path=$entry.Key; $snapshot=$entry.Value
        # Successfully deleted trees remain deleted. Restore only missing
        # ownership/inventory files inside a tree that could not be removed.
        if((Test-Path -LiteralPath $snapshot.Boundary) -and -not (Test-Path -LiteralPath $path)) {
            try {
                [void](Assert-PinsetDeletionRoot $snapshot.Boundary); [void](Assert-PinsetPath $path)
                if(-not (Test-PinsetUnder $path $snapshot.Boundary)) {throw 'Recovery path escaped its original boundary'}
                [void][IO.Directory]::CreateDirectory((Split-Path -Parent $path)); [IO.File]::WriteAllBytes($path,$snapshot.Bytes)
            } catch {$script:failures.Add("Cannot retain recovery metadata '$path': $($_.Exception.Message)")}
        }
    }
}
function Remove-PinsetTree([string]$Path,[string]$Boundary,[switch]$FinalizeMetadata) {
    # One level at a time; SDK junctions/symlinks are unlinked, never traversed.
    $full=Get-PinsetFullPath $Path
    if (-not (Test-PinsetUnder $full $Boundary)) { throw "Deletion escaped boundary: $full" }
    try {$item=Get-Item -LiteralPath $full -Force}
    catch {$message="Cannot inspect '$full': $($_.Exception.Message)"; $script:failures.Add($message); Write-Warning $message; return $false}
    $reparse=[bool]($item.Attributes -band [IO.FileAttributes]::ReparsePoint)
    # Keep ownership and project registries until every payload deletion has
    # succeeded. A failed/racing lock must not make the next run unrecognizable.
    if(-not $FinalizeMetadata -and -not $reparse -and $item.Name -in @('.pinset-home.json','.pinset-owner.toml','.pinset-venv.toml','.bootstrap.lock','pyvenv.cfg','config.toml','lock.toml','pinset.exe','pinset-shim.exe','state','global')) {
        if($item.Name -notin @('pinset.exe','pinset-shim.exe','.bootstrap.lock')) {
            try {Save-PinsetRecoveryMetadata $full $Boundary}
            catch {$message="Cannot read recovery metadata '$full': $($_.Exception.Message)"; $script:failures.Add($message); Write-Warning $message; return $false}
        }
        [void]$script:deferredMetadata.Add($full); return $true
    }
    if ($reparse) {
        return (Invoke-PinsetDeleteNode $full $item.PSIsContainer $true)
    } elseif ($item.PSIsContainer) {
        $success=$true
        try {$children=@(Get-ChildItem -LiteralPath $full -Force)}
        catch {$message="Cannot inspect directory '$full': $($_.Exception.Message)"; $script:failures.Add($message); Write-Warning $message; return $false}
        $markers=@('.pinset-home.json','.pinset-owner.toml','.pinset-venv.toml','pyvenv.cfg','config.toml')
        foreach ($child in ($children | Sort-Object @{Expression={if($_.Name -in $markers){1}else{0}}},Name)) {
            if($FinalizeMetadata -and -not $success -and $child.Name -in ($markers+@('state','global','lock.toml'))) {continue}
            if(-not (Remove-PinsetTree $child.FullName $Boundary -FinalizeMetadata:$FinalizeMetadata)) {$success=$false}
        }
        if(-not $success) {return $false}
        if(-not $FinalizeMetadata -and @(Get-ChildItem -LiteralPath $full -Force).Count) {return $true}
        return (Invoke-PinsetDeleteNode $full $true $false)
    } else { return (Invoke-PinsetDeleteNode $full $false $false) }
}
function Invoke-PinsetAction([string]$Description,[scriptblock]$Action) {
    Write-Output $Description
    if (-not $Plan) { try { & $Action } catch { $script:failures.Add("${Description}: $($_.Exception.Message)"); Write-Warning $script:failures[-1] } }
}
function Get-PinsetText([string]$Path) {
    $bytes=[IO.File]::ReadAllBytes($Path); $enc=[Text.UTF8Encoding]::new($false); $offset=0
    if ($bytes.Length -ge 3 -and $bytes[0] -eq 239 -and $bytes[1] -eq 187 -and $bytes[2] -eq 191) { $enc=[Text.UTF8Encoding]::new($true); $offset=3 }
    elseif ($bytes.Length -ge 2 -and $bytes[0] -eq 255 -and $bytes[1] -eq 254) { $enc=[Text.Encoding]::Unicode; $offset=2 }
    elseif ($bytes.Length -ge 2 -and $bytes[0] -eq 254 -and $bytes[1] -eq 255) { $enc=[Text.Encoding]::BigEndianUnicode; $offset=2 }
    @{Text=$enc.GetString($bytes,$offset,$bytes.Length-$offset); Encoding=$enc}
}
function Test-PinsetDirectoryRecord([string]$Text,[string]$Root,[switch]$Optional) {
    $m=[regex]::Match($Text,'(?ms)^\s*\[directory\]\s*\r?\n(?:(?!^\s*\[).)*?^\s*root\s*=\s*("(?:[^"\\]|\\.)*"|''[^'']*'')\s*$')
    if (-not $m.Success) { return [bool]$Optional -and $Text -notmatch '(?m)^\s*\[directory\]' }
    $literal=$m.Groups[1].Value
    $path=if($literal.StartsWith("'")){$literal.Substring(1,$literal.Length-2)}else{$literal | ConvertFrom-Json}
    (Get-PinsetFullPath $path) -eq (Get-PinsetFullPath $Root)
}
function Find-PinsetLegacyEnvironments([string]$Root) {
    $pending=[Collections.Generic.Stack[string]]::new(); $pending.Push($Root)
    while($pending.Count) {
        $directory=$pending.Pop(); $marker=Join-Path $directory '.pinset-venv.toml'
        if($directory -ne $Root -and (Test-Path -LiteralPath $marker -PathType Leaf)) {
            [void](Assert-PinsetPath $marker); $text=Get-Content -LiteralPath $marker -Raw
            if($text -match '(?m)^\s*schema\s*=\s*[123]\s*$' -and $text -match '(?m)^\s*distribution\s*=\s*"[^"\r\n]+"\s*$' -and
                $text -match '(?m)^\s*target\s*=\s*"(?:windows|linux|macos)-[^"\r\n]+"\s*$' -and
                (Test-Path -LiteralPath (Join-Path $directory 'pyvenv.cfg') -PathType Leaf) -and (Test-PinsetDirectoryRecord $text $Root -Optional)) {
                [void]$trees.Add((Assert-PinsetDeletionRoot $directory)); [void]$owned.Add($directory)
            } else { $script:unresolved.Add("Legacy virtual environment ownership needs review: $directory") }
            continue
        }
        foreach($child in @(Get-ChildItem -LiteralPath $directory -Directory -Force)) {
            if($child.Name -in @('.git','.pinset') -or ($child.Attributes -band [IO.FileAttributes]::ReparsePoint)) {continue}
            if($trees.Contains($child.FullName)) {continue}
            $pending.Push($child.FullName)
        }
    }
}
function Get-PinsetCleanProfile([string]$Text,[string[]]$OwnedRoots) {
    $lines=[Collections.Generic.List[string]]::new(); $block=$false
    foreach ($line in [regex]::Split($Text,'(?<=\n)')) {
        $trim=$line.Trim()
        if ($trim -match '^#\s*(?:>>>|BEGIN)\s*pinset\b') { $block=$true; continue }
        if ($block) { if ($trim -match '^#\s*(?:<<<|END)\s*pinset\b') { $block=$false }; continue }
        $hasOwned=$false
        foreach ($root in $OwnedRoots) { if ($line -match ([regex]::Escape($root)+'(?=[\\/;''"\s]|$)') -or $line -match ([regex]::Escape($root.Replace('\','/'))+'(?=[\\/;''"\s]|$)')) { $hasOwned=$true } }
        $remove=$false; $handled=$false
        if ($trim -and -not $trim.StartsWith('#') -and ($hasOwned -or $trim -match '(?i)(?:^|[\s(&])pinset(?:\.exe)?(?:\s|$)|\$env:PINSET_')) {
            if ($trim -match '^\$env:PINSET_[A-Za-z0-9_]+\s*=\s*(?:''[^'']*''|"[^"`]*")\s*;?\s*$') { $remove=$true }
            if ($trim -match '^(?:Invoke-Expression\s+\(?|iex\s+\(?)?\(?&?\s*pinset(?:\.exe)?\s+(?:self\s+shell|activate|env\s+shell|hook)\b[^;{}]*$') { $remove=$true }
            # Parse static assignments and preserve every unrelated path, even
            # when Pinset and another SDK were prepended on the same line.
            $assignment=[regex]::Match($trim,'^\$env:(?<name>[A-Za-z0-9_]+)\s*=\s*(?:''(?<single>(?:''''|[^''])*)''|"(?<double>[^"$`]*)")(?<suffix>\s*(?:\+\s*(?:\[IO.Path\]::PathSeparator\s*\+\s*)?\$env:\k<name>)?)\s*;?\s*$')
            if ($assignment.Success -and -not $remove) {
                $literal=$assignment.Groups['single'].Value.Replace("''", "'")+$assignment.Groups['double'].Value
                $kept=[Collections.Generic.List[string]]::new(); $changed=$false
                foreach ($part in ($literal -split ';')) {
                    $isOwned=$false
                    if ($part -match '^(?:[A-Za-z]:[\\/]|\\\\)') {
                        $candidate=Get-PinsetFullPath $part
                        foreach ($root in $OwnedRoots) { if (Test-PinsetUnder $candidate $root) { $isOwned=$true; break } }
                    }
                    if ($isOwned) { $changed=$true } else { $kept.Add($part) }
                }
                if ($changed) {
                    $handled=$true
                    if (-not ($kept -join ';')) { $remove=$true }
                    else {
                        $newline=if ($line.EndsWith("`r`n")) {"`r`n"} elseif ($line.EndsWith("`n")) {"`n"} else {''}
                        $line='$env:'+$assignment.Groups['name'].Value+" = '"+($kept -join ';').Replace("'","''")+"'"+$assignment.Groups['suffix'].Value+$newline
                    }
                }
            }
            if (-not $remove -and -not $handled) { $script:unresolved.Add('Review shell configuration: '+$script:profileBeingRead) }
        }
        if (-not $remove) { $lines.Add($line) }
    }
    if ($block) { throw 'Unterminated Pinset integration marker; profile retained.' }
    $lines -join ''
}
function Remove-PinsetCredentials {
    # Target names only; credential blobs are never read or printed.
    if (-not ('PinsetUninstall.NativeCredentials' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
namespace PinsetUninstall {
 public static class NativeCredentials {
  [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
  struct Credential {
   public uint Flags,Type; public string TargetName,Comment; public long LastWritten;
   public uint CredentialBlobSize; public IntPtr CredentialBlob; public uint Persist,AttributeCount;
   public IntPtr Attributes; public string TargetAlias,UserName;
  }
  [DllImport("advapi32.dll",EntryPoint="CredEnumerateW",CharSet=CharSet.Unicode,SetLastError=true)]
  static extern bool Enumerate(string filter,uint flags,out uint count,out IntPtr credentials);
  [DllImport("advapi32.dll",EntryPoint="CredDeleteW",CharSet=CharSet.Unicode,SetLastError=true)]
  static extern bool Delete(string target,uint type,uint flags);
  [DllImport("advapi32.dll")] static extern void CredFree(IntPtr buffer);
  public static void Remove() {
   uint count; IntPtr data;
   if(!Enumerate(null,0,out count,out data)) { int code=Marshal.GetLastWin32Error(); if(code==1168) return; throw new Win32Exception(code); }
   var targets=new List<string>();
   try {
    for(int i=0;i<count;i++) {
     var c=(Credential)Marshal.PtrToStructure(Marshal.ReadIntPtr(data,i*IntPtr.Size),typeof(Credential));
     if(c.Type==1 && c.TargetName!=null && (c.TargetName.EndsWith(".dev.pinset.identity",StringComparison.Ordinal) || c.TargetName.EndsWith(".dev.pinset.identity.v3",StringComparison.Ordinal))) targets.Add(c.TargetName);
    }
   } finally { CredFree(data); }
   foreach(string target in targets) { if(!Delete(target,1,0)) { int code=Marshal.GetLastWin32Error(); if(code!=1168) throw new Win32Exception(code); } }
  }
 }
}
'@
    }
    [PinsetUninstall.NativeCredentials]::Remove()
}

$script:failures=[Collections.Generic.List[string]]::new(); $script:unresolved=[Collections.Generic.List[string]]::new()
$script:deferredMetadata=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$script:recoverySnapshots=@{}
$homes=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$bins=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$owned=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$trees=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$homeMetadataTrees=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$projectStateTrees=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$files=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$projects=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$managerRoots=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$scopes=@('Process','User','Machine'); $environment=@{}
foreach ($scope in $scopes) { $environment[$scope]=Get-PinsetEnvironment $scope }
$scoopCommand=Get-Command scoop -ErrorAction SilentlyContinue
if($scoopCommand -and $scoopCommand.Source -match '[\\/]shims[\\/]scoop\.(?:ps1|cmd|exe)$') { [void]$managerRoots.Add((Get-PinsetFullPath (Split-Path -Parent (Split-Path -Parent $scoopCommand.Source)))) }
foreach($scope in $scopes) { foreach($key in @('SCOOP','SCOOP_GLOBAL')) { if($environment[$scope].Contains($key) -and $environment[$scope][$key]) { [void]$managerRoots.Add((Get-PinsetFullPath $environment[$scope][$key])) } } }
foreach ($path in @((Join-Path $env:USERPROFILE '.pinset'),(Join-Path $env:LOCALAPPDATA 'Pinset'))+$PinsetHome) { [void]$homes.Add((Assert-PinsetDeletionRoot $path)) }
foreach ($scope in $scopes) {
    foreach ($key in @('PINSET_HOME','PINSET_INSTALL_DIR')) {
        if ($environment[$scope].Contains($key) -and $environment[$scope][$key]) {
            $path=[string]$environment[$scope][$key]
            if ($key -eq 'PINSET_HOME') { [void]$homes.Add((Assert-PinsetDeletionRoot $path)) } else { [void]$bins.Add((Assert-PinsetPath $path)) }
        }
    }
}
foreach ($path in $InstallDir) { [void]$bins.Add((Assert-PinsetPath $path)) }
foreach ($path in $Project) { [void]$projects.Add((Assert-PinsetDeletionRoot $path)) }

$profilePaths=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$documents=[Environment]::GetFolderPath('MyDocuments')
foreach ($directory in @((Join-Path $documents 'PowerShell'),(Join-Path $documents 'WindowsPowerShell'))) {
    if (Test-Path -LiteralPath $directory) { foreach ($file in @(Get-ChildItem -LiteralPath $directory -Filter '*profile.ps1' -File -Force)) { [void]$profilePaths.Add($file.FullName) } }
}
foreach ($profileName in @('AllUsersAllHosts','AllUsersCurrentHost','CurrentUserAllHosts','CurrentUserCurrentHost')) { if ($PROFILE -and $PROFILE.PSObject.Properties[$profileName]) { [void]$profilePaths.Add([string]$PROFILE.$profileName) } }
foreach ($path in $profilePaths) {
    if (Test-Path -LiteralPath $path -PathType Leaf) {
        [void](Assert-PinsetPath $path); $text=(Get-PinsetText $path).Text
        foreach ($m in [regex]::Matches($text,'(?im)^\s*\$env:PINSET_HOME\s*=\s*(?:''([^'']+)''|"([^"$`]+)")\s*;?\s*$')) { [void]$homes.Add((Assert-PinsetDeletionRoot ($m.Groups[1].Value+$m.Groups[2].Value))) }
    }
}
# Validate every data root before any modification.
foreach ($base in $homes) {
    $v3=Join-Path $base 'v3'; $hasV3=$false
    if (Test-Path -LiteralPath $v3) {
        [void](Assert-PinsetPath $v3); $marker=Join-Path $v3 '.pinset-home.json'; [void](Assert-PinsetPath $marker)
        if (-not (Test-Path -LiteralPath $marker -PathType Leaf) -or (Get-Content -LiteralPath $marker -Raw | ConvertFrom-Json).protocol -ne 'pinset/3') { throw "Unowned Pinset v3 directory: $v3" }
        [void]$trees.Add($v3); [void]$homeMetadataTrees.Add($v3); [void]$owned.Add($v3); $hasV3=$true
        $registry=Join-Path $v3 'state\projects.json'
        if (Test-Path -LiteralPath $registry -PathType Leaf) {
            [void](Assert-PinsetPath $registry)
            $records=Get-Content -LiteralPath $registry -Raw | ConvertFrom-Json
            foreach ($record in $records.PSObject.Properties) {
                $root=Assert-PinsetDeletionRoot $record.Name
                if ((Get-PinsetFullPath ([string]$record.Value)) -ne (Join-Path $root '.pinset\config.toml')) { throw "Invalid project registry entry: $registry" }
                if (Test-Path -LiteralPath $root) { [void]$projects.Add($root) }
            }
        }
    }
    $legacy=$false
    foreach ($name in @('state\global.toml','state\global.lock','state\identities.toml')) {
        $record=Join-Path $base $name
        if (Test-Path -LiteralPath $record -PathType Leaf) { [void](Assert-PinsetPath $record); if ((Get-Content -LiteralPath $record -Raw) -match '(?m)^\s*schema\s*=\s*[12]\b|\[\[identities\]\]|\[tools\]|\[tool\.') { $legacy=$true } }
    }
    if ($base -eq (Join-Path $env:LOCALAPPDATA 'Pinset') -and (Test-Path -LiteralPath $base)) { $legacy=$true }
    if ($legacy) {
        foreach ($name in @('bin','state','installs','cache','downloads')) { $path=Join-Path $base $name; if (Test-Path -LiteralPath $path) { [void]$trees.Add((Assert-PinsetPath $path)); [void]$owned.Add($path); if($name -eq 'state'){[void]$homeMetadataTrees.Add($path)} } }
        $registry=Join-Path $base 'state\projects'
        if (Test-Path -LiteralPath $registry) {
            [void](Assert-PinsetPath $registry)
            foreach ($record in @(Get-ChildItem -LiteralPath $registry -Filter '*.toml' -File)) {
                [void](Assert-PinsetPath $record.FullName)
                $m=[regex]::Match((Get-Content -LiteralPath $record.FullName -Raw),'(?m)^\s*config\s*=\s*"((?:[^"\\]|\\.)*)"\s*$')
                if ($m.Success) {
                    $config=Get-PinsetFullPath ('"'+$m.Groups[1].Value+'"' | ConvertFrom-Json)
                    if ((Split-Path -Leaf $config) -ne 'pinset.toml') { throw "Invalid legacy project registry: $($record.FullName)" }
                    if (Test-Path -LiteralPath $config) { [void]$projects.Add((Assert-PinsetDeletionRoot (Split-Path -Parent $config))) }
                } else { throw "Unrecognized legacy project registry: $($record.FullName)" }
            }
        }
    }
    $backups=Join-Path $base 'migration-backups'
    if (Test-Path -LiteralPath $backups) {
        if (-not $legacy -and -not $hasV3) { throw "Migration backups lack Pinset ownership: $backups" }
        [void]$trees.Add((Assert-PinsetPath $backups)); [void]$owned.Add($backups)
    }
    [void]$bins.Add((Join-Path $base 'bin')); [void]$bins.Add((Join-Path $base 'v3\bin'))
    # A v3 marker proves ownership of v3, not unrelated siblings in a custom
    # PINSET_HOME. Conventional Pinset-only bases can be removed from PATH too.
    if ($base -eq (Join-Path $env:USERPROFILE '.pinset') -or $base -eq (Join-Path $env:LOCALAPPDATA 'Pinset')) { [void]$owned.Add($base) }
}
foreach ($root in $projects) {
    $state=Join-Path $root '.pinset'; $projectId=$null
    if (Test-Path -LiteralPath $state) {
        [void](Assert-PinsetPath $state); $config=Join-Path $state 'config.toml'; [void](Assert-PinsetPath $config)
        if (-not (Test-Path -LiteralPath $config -PathType Leaf)) { throw "Missing Pinset project configuration: $config" }
        $text=Get-Content -LiteralPath $config -Raw
        $idMatch=[regex]::Match($text,'(?m)^\s*project_id\s*=\s*"([^"]+)"\s*$')
        if ($text -notmatch '(?m)^\s*protocol\s*=\s*"pinset/3"\s*$' -or -not $idMatch.Success) { throw "Project state lacks Pinset 3 ownership: $state" }
        $projectId=$idMatch.Groups[1].Value
        [void]$trees.Add($state); [void]$projectStateTrees.Add($state); [void]$owned.Add($state)
    }
    foreach ($entry in @(Get-ChildItem -LiteralPath $root -Directory -Force | Where-Object { $_.Name -eq '.venv' -or $_.Name -like '.venv.pinset-v2-*' -or $_.Name -like '.venv.pinset-v3-failed-*' })) {
        $marker=Join-Path $entry.FullName '.pinset-owner.toml'
        if (Test-Path -LiteralPath $marker -PathType Leaf) {
            [void](Assert-PinsetPath $marker); $text=Get-Content -LiteralPath $marker -Raw
            $idMatch=[regex]::Match($text,'(?m)^\s*project_id\s*=\s*"([^"]+)"\s*$')
            if ($text -match '(?m)^\s*protocol\s*=\s*"pinset/3"\s*$' -and $idMatch.Success -and $idMatch.Groups[1].Value -eq $projectId -and (Test-PinsetDirectoryRecord $text $root) -and (Test-Path -LiteralPath (Join-Path $entry.FullName 'pyvenv.cfg') -PathType Leaf)) { [void]$trees.Add((Assert-PinsetPath $entry.FullName)); [void]$owned.Add($entry.FullName) }
            else { $script:unresolved.Add("Virtual environment ownership differs: $($entry.FullName)") }
        } elseif ($entry.Name -ne '.venv' -and -not (Test-Path -LiteralPath (Join-Path $entry.FullName '.pinset-venv.toml') -PathType Leaf)) { $script:unresolved.Add("Unmarked retained environment needs review: $($entry.FullName)") }
    }
    Find-PinsetLegacyEnvironments $root
    $legacyConfig=Join-Path $root 'pinset.toml'
    if (Test-Path -LiteralPath $legacyConfig -PathType Leaf) {
        [void](Assert-PinsetPath $legacyConfig)
        $legacyText=Get-Content -LiteralPath $legacyConfig -Raw
        if($legacyText -notmatch '(?m)^\s*schema\s*=\s*[1-6]\s*$' -or $legacyText -notmatch '(?m)^\s*(?:\[tools\]|project-id\s*=)') {throw "Legacy project configuration lacks ownership: $legacyConfig"}
        [void]$files.Add($legacyConfig)
        foreach ($name in @('pinset.lock','pinset.lock.toml')) { $path=Join-Path $root $name; if (Test-Path -LiteralPath $path -PathType Leaf) { [void](Assert-PinsetPath $path); [void]$files.Add($path) } }
    }
    foreach($profileFile in @(Get-ChildItem -LiteralPath $root -Filter '.env.*' -File -Force)) {
        [void](Assert-PinsetPath $profileFile.FullName)
        if((Get-Content -LiteralPath $profileFile.FullName -TotalCount 1) -ceq '# pinset-encrypted-env v1') { [void]$files.Add($profileFile.FullName) }
    }
    $editor=Join-Path $root '.vscode\settings.json'
    if ((Test-Path -LiteralPath $editor -PathType Leaf) -and (Get-Content -LiteralPath $editor -Raw) -match '(?i)pinset|flutter-sdk') { $script:unresolved.Add("Review editor bindings: $editor") }
}
# Shared command directories are never recursively deleted.
foreach ($scope in $scopes) {
    if ($environment[$scope].Contains('Path')) { foreach ($part in ([string]$environment[$scope]['Path'] -split ';')) { if ($part -and $part -match '^(?:[A-Za-z]:[\\/]|\\\\)') { [void]$bins.Add((Get-PinsetFullPath $part.Trim('"'))) } } }
}
foreach ($bin in $bins) {
    $managerOwned=$false
    foreach($managerRoot in $managerRoots) {if(Test-PinsetUnder $bin $managerRoot){$managerOwned=$true;break}}
    if($managerOwned) {continue}
    foreach ($name in @('pinset.exe','pinset-shim.exe')) {
        $path=Join-Path $bin $name
        if ((Test-Path -LiteralPath $path) -and $path -notmatch '\\(?:scoop\\(?:apps|shims)|WinGet\\(?:Links|Packages))\\') { [void](Assert-PinsetPath $path -AllowLeafLink); [void]$files.Add($path) }
    }
    if(Test-Path -LiteralPath $bin -PathType Container) {
        foreach($wrapper in @(Get-ChildItem -LiteralPath $bin -Filter '*.cmd' -File -Force)) {
            if($wrapper.Length -gt 8192) {continue}
            $content=Get-Content -LiteralPath $wrapper.FullName -Raw
            $m=[regex]::Match($content,'\A@echo off\r?\nsetlocal DisableDelayedExpansion\r?\n"(?<shim>[^"\r\n]*pinset-shim\.exe)" --as (?<command>[A-Za-z0-9_.+-]+) -- %\*\r?\nexit /b %ERRORLEVEL%\r?\n?\z')
            if(-not $m.Success -or $wrapper.Name -ne ($m.Groups['command'].Value+'.cmd')) {continue}
            $shim=Get-PinsetFullPath ($m.Groups['shim'].Value.Replace('%%','%')); $ours=$files.Contains($shim)
            foreach($ownedRoot in $owned) {if(Test-PinsetUnder $shim $ownedRoot){$ours=$true}}
            if(-not $ours) {continue}
            [void](Assert-PinsetPath $wrapper.FullName); [void]$files.Add($wrapper.FullName)
            $posix=Join-Path $bin $m.Groups['command'].Value
            if(Test-Path -LiteralPath $posix -PathType Leaf) {
                [void](Assert-PinsetPath $posix)
                $expected='#!/bin/sh'+"`n"+'exec "$(dirname "$0")/'+$m.Groups['command'].Value+'.cmd" "$@"'+"`n"
                if((Get-Content -LiteralPath $posix -Raw) -ceq $expected) {[void]$files.Add($posix)}
            }
        }
    }
}
$profileChanges=@{}
foreach ($path in $profilePaths) {
    if (Test-Path -LiteralPath $path -PathType Leaf) { $script:profileBeingRead=$path; $original=Get-PinsetText $path; $clean=Get-PinsetCleanProfile $original.Text @($owned); if ($clean -ne $original.Text) { $profileChanges[$path]=@{Text=$clean; Encoding=$original.Encoding} } }
}
$envChanges=[Collections.Generic.List[object]]::new()
foreach ($scope in $scopes) {
    foreach ($entry in $environment[$scope].GetEnumerator()) {
        $name=[string]$entry.Key; $old=[string]$entry.Value; $new=$old
        if ($name -like 'PINSET_*') { $new=$null }
        else {
            $kept=[Collections.Generic.List[string]]::new(); $removedOwnedComponent=$false
            foreach ($part in ($old -split ';')) {
                $remove=$false; $candidate=[Environment]::ExpandEnvironmentVariables($part.Trim().Trim('"'))
                if ($candidate -match '^(?:[A-Za-z]:[\\/]|\\\\)') { try { $candidate=Get-PinsetFullPath $candidate; foreach ($ownedRoot in $owned) { if (Test-PinsetUnder $candidate $ownedRoot) { $remove=$true; break } } } catch {} }
                if ($remove) {$removedOwnedComponent=$true} else { $kept.Add($part) }
            }
            if($removedOwnedComponent) {$new=$kept -join ';'; if (-not $new -and $name -ne 'Path') { $new=$null }}
        }
        if ($new -cne $old) { $envChanges.Add(@{Scope=$scope; Name=$name; Value=$new}) }
    }
}
Write-Output 'Complete Pinset uninstall permanently removes the following SDKs, identities and configuration.'
foreach ($path in @($trees)+@($files)) { Write-Output "Remove: $path" }
foreach ($path in $profileChanges.Keys) { Write-Output "Clean profile: $path" }
foreach ($change in $envChanges) { Write-Output "Clean environment: $($change.Scope)/$($change.Name)" }
Write-Output 'Remove Credential Manager services dev.pinset.identity and dev.pinset.identity.v3; unregister WinGet/Scoop Pinset when present.'
$preflightIssues=[Collections.Generic.List[string]]::new()
$machineChanges=@($envChanges | Where-Object {$_.Scope -eq 'Machine'})
if($machineChanges.Count -and -not (Test-PinsetAdministrator)) {
    $names=@($machineChanges | ForEach-Object {'Machine/'+$_.Name}) -join ', '
    $preflightIssues.Add("Administrator PowerShell is required to update $names. Open PowerShell with 'Run as administrator' and rerun this script.")
}
$blockedProcesses=[Collections.Generic.List[object]]::new()
foreach($process in @(Get-PinsetRunningProcesses)) {
    try {$image=Get-PinsetFullPath $process.Path} catch {continue}
    $blocked=$files.Contains($image)
    foreach($tree in $trees) {if(Test-PinsetUnder $image $tree){$blocked=$true;break}}
    if($blocked) {$blockedProcesses.Add($process)}
}
if($blockedProcesses.Count) {
    $details=@($blockedProcesses | Sort-Object Id -Unique | ForEach-Object {"$($_.Name) (PID $($_.Id)): $($_.Path)"}) -join "`n"
    $preflightIssues.Add("Pinset-managed programs are still running. Close their applications, development servers and terminals, then retry. Hard-linked shim aliases can keep another name such as xjc.exe locked.`n$details")
}
if($preflightIssues.Count) {
    if($Plan) {foreach($issue in $preflightIssues){Write-Warning $issue}}
    else {throw (($preflightIssues -join "`n")+"`nPreflight stopped; no uninstall changes have been made.")}
}
if (-not $Plan -and -not $Yes) { if ([Console]::IsInputRedirected) { throw 'Use -Yes for noninteractive execution or -Plan to preview.' }; if ((Read-Host 'Permanently remove listed Pinset data? [y/N]') -notmatch '^[yY]$') { return } }
if (Get-Command winget -ErrorAction SilentlyContinue) {
    Invoke-PinsetAction 'Check/uninstall WinGet FutureElement.Pinset' {
        $listed=& winget list --id FutureElement.Pinset --exact --disable-interactivity --accept-source-agreements 2>&1 | Out-String
        if ($LASTEXITCODE -eq 0 -and $listed -match 'FutureElement\.Pinset') { & winget uninstall --id FutureElement.Pinset --exact --silent --disable-interactivity | Out-Host; if ($LASTEXITCODE -ne 0) { throw "winget returned $LASTEXITCODE" } }
        elseif ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne -1978335212) { throw "winget list returned $LASTEXITCODE" }
    }
}
if (Get-Command scoop -ErrorAction SilentlyContinue) {
    Invoke-PinsetAction 'Check/uninstall Scoop pinset and its download cache' {
        $installed=@(& scoop list '^pinset$')
        if($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne 1) {throw "scoop list returned $LASTEXITCODE"}
        foreach($item in $installed) {
            if($item.Name -ne 'pinset') {continue}
            $arguments=@('uninstall','pinset','--purge')
            if($item.Info -match 'Global install') {$arguments+='--global'}
            & scoop @arguments | Out-Host; if($LASTEXITCODE -ne 0) {throw "scoop uninstall returned $LASTEXITCODE"}
        }
        & scoop cache rm pinset | Out-Host; if($LASTEXITCODE -ne 0) {throw "scoop cache returned $LASTEXITCODE"}
    }
}
Invoke-PinsetAction 'Remove Pinset Credential Manager identities' { Remove-PinsetCredentials }
foreach ($path in $profileChanges.Keys) { $change=$profileChanges[$path]; Invoke-PinsetAction "Clean profile: $path" { [void](Assert-PinsetPath $path); [IO.File]::WriteAllText($path,$change.Text,$change.Encoding) } }
foreach ($change in $envChanges) { Invoke-PinsetAction "Clean environment: $($change.Scope)/$($change.Name)" { Set-PinsetEnvironment $change.Name $change.Value $change.Scope } }
foreach ($path in ($trees | Sort-Object Length -Descending)) { Invoke-PinsetAction "Remove managed payloads: $path" { if (Test-Path -LiteralPath $path) { $checked=Assert-PinsetDeletionRoot $path; [void](Remove-PinsetTree $checked $checked) } } }
if($script:failures.Count -eq 0) {
    # Project metadata must finalize before global registries, regardless of
    # path length. Stop on a failure so registered roots remain discoverable.
    $finalTrees=@($trees | Sort-Object @{Expression={if($homeMetadataTrees.Contains($_)){2}elseif($projectStateTrees.Contains($_)){1}else{0}}},@{Expression={$_.Length};Descending=$true})
    foreach ($path in $finalTrees) {
        Invoke-PinsetAction "Finalize managed directory: $path" { if (Test-Path -LiteralPath $path) { $checked=Assert-PinsetDeletionRoot $path; [void](Remove-PinsetTree $checked $checked -FinalizeMetadata) } }
        if($script:failures.Count) {Restore-PinsetRecoveryMetadata; break}
    }
}
if($script:failures.Count -eq 0) {
    foreach ($path in $files) { Invoke-PinsetAction "Remove command/file: $path" { if (Test-Path -LiteralPath $path) { [void](Assert-PinsetPath $path -AllowLeafLink); [void](Invoke-PinsetDeleteNode $path $false $false) } } }
} elseif(-not $Plan) {Write-Warning 'Remaining ownership metadata, project registries and command files were retained so the uninstall can be retried.'}
foreach ($base in $homes) {
    if (Test-Path -LiteralPath $base) {
        if (@(Get-ChildItem -LiteralPath $base -Force).Count -eq 0) { Invoke-PinsetAction "Remove empty home: $base" { [void](Assert-PinsetDeletionRoot $base); Remove-Item -LiteralPath $base -Force } }
        elseif (-not $Plan) { $script:unresolved.Add("Unrecognized files retained in home: $base") }
    }
}
foreach ($notice in ($script:unresolved | Select-Object -Unique)) { Write-Warning $notice }
if ($script:failures.Count -or $script:unresolved.Count) { throw "Pinset uninstall needs attention: $($script:failures.Count) failed actions; $($script:unresolved.Count) retained/review items." }
if ($Plan) { Write-Output 'Plan only; no files, settings, packages or credentials changed.' } else { Write-Output 'Pinset removed. Restart all terminals and applications to discard inherited environments.' }
