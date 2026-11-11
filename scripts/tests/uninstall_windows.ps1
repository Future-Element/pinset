# Isolated destructive-fixture tests. Never uses real registry, packages or keyring.
# Run: pwsh -NoProfile -File scripts/tests/uninstall_windows.ps1
$ErrorActionPreference='Stop'
Set-StrictMode -Version Latest
$repository=Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$source=[IO.File]::ReadAllText((Join-Path $repository 'uninstall.ps1'))
$tokens=$null; $parseErrors=$null
$ast=[Management.Automation.Language.Parser]::ParseInput($source,[ref]$tokens,[ref]$parseErrors)
if($parseErrors.Count){throw ($parseErrors | Out-String)}
function Assert-Test($Condition,[string]$Message) { if(-not $Condition){throw $Message} }
$fixture=Join-Path ([IO.Path]::GetTempPath()) ('pinset-uninstall-test-'+[guid]::NewGuid().ToString('N'))
$user=Join-Path $fixture 'user'; $local=Join-Path $user 'AppData\Local'
$homePath=Join-Path $user '.pinset'; $v3=Join-Path $homePath 'v3'; $docs=Join-Path $user 'Documents'
$project=Join-Path $fixture 'project'; $external=Join-Path $fixture 'external'
$previous=@{}
foreach($name in @('USERPROFILE','LOCALAPPDATA','APPDATA','PINSET_HOME')) {$previous[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
try {
    foreach($dir in @($v3,(Join-Path $v3 'installs\node'),(Join-Path $docs 'PowerShell'),$local,$external,(Join-Path $project '.pinset'),(Join-Path $project '.venv'))) {New-Item -ItemType Directory -Path $dir -Force | Out-Null}
    [IO.File]::WriteAllText((Join-Path $v3 '.pinset-home.json'),'{"protocol":"pinset/3"}')
    [IO.File]::WriteAllText((Join-Path $v3 'installs\node\payload'),'SDK')
    [IO.File]::WriteAllText((Join-Path $external 'keep'),'external')
    # Junction inside SDK must be unlinked without following its target.
    New-Item -ItemType Junction -Path (Join-Path $v3 'installs\external-link') -Target $external | Out-Null
    $profilePath=Join-Path $docs 'PowerShell\Microsoft.PowerShell_profile.ps1'
    $profileText="`$env:PATH = '$v3\bin' + [IO.Path]::PathSeparator + `$env:PATH`r`n`$env:KEEP = 'ok'`r`n"
    [IO.File]::WriteAllText($profilePath,$profileText,[Text.Encoding]::Unicode)
    [IO.File]::WriteAllText((Join-Path $project '.pinset\config.toml'),"protocol = `"pinset/3`"`nproject_id = `"fixture`"`n")
    [IO.File]::WriteAllText((Join-Path $project '.venv\.pinset-owner.toml'),("protocol = `"pinset/3`"`nproject_id = `"fixture`"`n[directory]`nroot = "+($project | ConvertTo-Json -Compress)+"`n"))
    [IO.File]::WriteAllText((Join-Path $project '.venv\pyvenv.cfg'),'home = fixture')
    $named=Join-Path $project 'envs\testing.pinset-backup-fixture'; New-Item -ItemType Directory -Path $named -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $named '.pinset-venv.toml'),("schema = 3`ndistribution = `"3.14.0`"`ntarget = `"windows-x86_64`"`n[directory]`nroot = "+($project | ConvertTo-Json -Compress)+"`n"))
    [IO.File]::WriteAllText((Join-Path $named 'pyvenv.cfg'),'home = fixture')
    [IO.File]::WriteAllText((Join-Path $project 'pinset.toml'),"schema = 6`n[tools]`nnode = `"lts`"`n")
    [IO.File]::WriteAllText((Join-Path $project '.env.secret'),"# pinset-encrypted-env v1`nSECRET=fixture`n")
    [IO.File]::WriteAllText((Join-Path $project '.env.keep'),'UNRELATED=yes')
    $registry=Join-Path $v3 'state\projects.json'; New-Item -ItemType Directory -Path (Split-Path -Parent $registry) | Out-Null
    $map=@{}; $map[$project]=Join-Path $project '.pinset\config.toml'; [IO.File]::WriteAllText($registry,($map | ConvertTo-Json))
    $env:USERPROFILE=$user; $env:LOCALAPPDATA=$local; $env:APPDATA=Join-Path $user 'AppData\Roaming'; $env:PINSET_HOME=$null
    $script:testEnvironment=@{Process=@{}; User=@{PINSET_HOME=$homePath; Path="$v3\bin;C:\Unrelated\bin"; JAVA_HOME="$v3\installs\java"; KEEP='yes'}; Machine=@{}}
    $script:testChanges=[Collections.Generic.List[object]]::new(); $script:credentialCalls=0
    $replacements=@{
        'Get-PinsetEnvironment'='function Get-PinsetEnvironment([string]$Scope) { $script:testEnvironment[$Scope] }'
        'Set-PinsetEnvironment'='function Set-PinsetEnvironment([string]$Name,$Value,[string]$Scope) { $script:testChanges.Add(@{Name=$Name; Value=$Value; Scope=$Scope}); if($null -eq $Value){$script:testEnvironment[$Scope].Remove($Name)}else{$script:testEnvironment[$Scope][$Name]=$Value} }'
        'Remove-PinsetCredentials'='function Remove-PinsetCredentials { $script:credentialCalls++ }'
    }
    $modified=$source
    foreach($fn in @($ast.FindAll({param($a) $a -is [Management.Automation.Language.FunctionDefinitionAst]},$true) | Sort-Object {$_.Extent.StartOffset} -Descending)) {
        if($replacements.ContainsKey($fn.Name)) {$modified=$modified.Remove($fn.Extent.StartOffset,$fn.Extent.EndOffset-$fn.Extent.StartOffset).Insert($fn.Extent.StartOffset,$replacements[$fn.Name])}
    }
    $modified=$modified.Replace('$documents=[Environment]::GetFolderPath(''MyDocuments'')',('$documents='''+$docs.Replace("'","''")+'''')).Replace('if (Get-Command winget -ErrorAction SilentlyContinue)', 'if ($false)').Replace('if (Get-Command scoop -ErrorAction SilentlyContinue)', 'if ($false)')
    # Force all host profile paths to fixture; built-in PROFILE is process-local.
    $PROFILE=Join-Path $docs 'fixture-profile.ps1'
    & ([scriptblock]::Create($modified)) -Plan | Out-Null
    Assert-Test (Test-Path -LiteralPath $v3) 'Plan removed SDK data'
    Assert-Test ($script:testChanges.Count -eq 0 -and $script:credentialCalls -eq 0) 'Plan mutated environment or keyring'
    Assert-Test ([IO.File]::ReadAllText($profilePath) -eq $profileText) 'Plan modified profile'
    & ([scriptblock]::Create($modified)) -Yes | Out-Null
    Assert-Test (-not (Test-Path -LiteralPath $v3)) 'Managed home remained'
    Assert-Test (-not (Test-Path -LiteralPath (Join-Path $project '.pinset'))) 'Registered project state remained'
    Assert-Test (-not (Test-Path -LiteralPath (Join-Path $project '.venv'))) 'Owned project venv remained'
    Assert-Test (-not (Test-Path -LiteralPath $named)) 'Legacy named environment/backup remained'
    Assert-Test (-not (Test-Path -LiteralPath (Join-Path $project '.env.secret'))) 'Pinset encrypted profile remained'
    Assert-Test (Test-Path -LiteralPath (Join-Path $project '.env.keep')) 'Unrelated plaintext env deleted'
    Assert-Test (Test-Path -LiteralPath (Join-Path $external 'keep')) 'SDK junction target was deleted'
    Assert-Test ([IO.File]::ReadAllText($profilePath) -eq "`$env:KEEP = 'ok'`r`n") 'Profile cleanup damaged unrelated content'
    Assert-Test ($script:testEnvironment.User.Path -eq 'C:\Unrelated\bin') 'Unrelated PATH entry was damaged'
    Assert-Test ($script:testEnvironment.User.KEEP -eq 'yes') 'Unrelated variable was damaged'
    Assert-Test (-not $script:testEnvironment.User.ContainsKey('JAVA_HOME') -and -not $script:testEnvironment.User.ContainsKey('PINSET_HOME')) 'Owned variables remained'
    & ([scriptblock]::Create($modified)) -Yes | Out-Null
    Assert-Test (Test-Path -LiteralPath (Join-Path $external 'keep')) 'Idempotent rerun damaged unrelated directory'
    # Extract actual function definitions for adversarial path checks.
    foreach($fn in $ast.FindAll({param($a) $a -is [Management.Automation.Language.FunctionDefinitionAst]},$true)) {Invoke-Expression $fn.Extent.Text}
    $script:unresolved=[Collections.Generic.List[string]]::new(); $script:profileBeingRead='fixture'
    $mixed='$env:PATH = ''C:\owned\v3\bin;D:\keep'' + [IO.Path]::PathSeparator + $env:PATH'+"`r`n"
    $clean=Get-PinsetCleanProfile $mixed @('C:\owned\v3')
    Assert-Test ($clean -eq ('$env:PATH = ''D:\keep'' + [IO.Path]::PathSeparator + $env:PATH'+"`r`n")) 'Mixed PATH profile entry damaged'
    $prefix='$env:JAVA_HOME = ''C:\owned\v3-other\java'''+"`n"
    Assert-Test ((Get-PinsetCleanProfile $prefix @('C:\owned\v3')) -eq $prefix) 'Prefix sibling falsely removed'
    Assert-Test ((Get-PinsetCleanProfile '$env:JAVA_HOME = ''C:/owned/v3/installs/java''' @('C:\owned\v3')) -eq '') 'Forward slash owned variable remained'
    Assert-Test ($script:unresolved.Count -eq 0) 'Supported profile edit falsely reported incomplete'
    $copied="protocol = `"pinset/3`"`nproject_id = `"fixture`"`n[directory]`nroot = `"C:\\different`"`n"
    Assert-Test (-not (Test-PinsetDirectoryRecord $copied $project)) 'Copied venv marker accepted for another project root'
    foreach($unsafe in @($env:USERPROFILE,'C:\','C:relative',(Split-Path -Parent $env:USERPROFILE))) {
        $rejected=$false; try {[void](Assert-PinsetDeletionRoot $unsafe)} catch {$rejected=$true}; Assert-Test $rejected "Unsafe path accepted: $unsafe"
    }
    $junction=Join-Path $fixture 'ancestor-link'; New-Item -ItemType Junction -Path $junction -Target $external | Out-Null
    $rejected=$false; try {[void](Assert-PinsetDeletionRoot (Join-Path $junction 'nested'))}catch{$rejected=$true}; Assert-Test $rejected 'Reparse ancestor accepted'
    # Native interop compiles, but enumeration/deletion is never called in tests.
    $nativeSource=@($tokens | Where-Object {$_.Kind -eq 'HereStringLiteral' -and $_.Value -match 'namespace PinsetUninstall'})[0].Value
    if(-not ('PinsetUninstall.NativeCredentials' -as [type])) {Add-Type -TypeDefinition $nativeSource}
    Write-Output 'Windows uninstall fixtures passed: parser, plan isolation, SDKs, registered projects, v3/v2/named venvs, encrypted profile ownership, mixed/forward-slash profile paths, PATH, unrelated vars, junction preservation, idempotence, unsafe roots and native interop compilation.'
} finally {
    foreach($name in $previous.Keys){[Environment]::SetEnvironmentVariable($name,$previous[$name],'Process')}
    # Cleanup only the verified UUID fixture; unlink junctions before recursion.
    if(Test-Path -LiteralPath $fixture) {
        $absolute=[IO.Path]::GetFullPath($fixture)
        if($absolute.StartsWith([IO.Path]::GetTempPath(),[StringComparison]::OrdinalIgnoreCase) -and (Split-Path -Leaf $absolute) -match '^pinset-uninstall-test-[a-f0-9]{32}$') {
            foreach($link in @((Join-Path $v3 'installs\external-link'),(Join-Path $fixture 'ancestor-link'))) {if(Test-Path -LiteralPath $link){[IO.Directory]::Delete($link)}}
            Remove-Item -LiteralPath $absolute -Recurse -Force
        }
    }
}
