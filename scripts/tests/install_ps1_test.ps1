$ErrorActionPreference = 'Stop'

$repository = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$installer = Join-Path $repository 'install.ps1'
$tokens = $null
$errors = $null
[void][System.Management.Automation.Language.Parser]::ParseFile(
    $installer,
    [ref]$tokens,
    [ref]$errors
)
if ($errors.Count -ne 0) {
    throw "install.ps1 contains parser errors: $($errors | ForEach-Object Message -join '; ')"
}

$content = Get-Content -LiteralPath $installer -Raw
foreach ($required in @(
    'SHA256SUMS',
    'Get-FileHash',
    'pinset.exe',
    'pinset-shim.exe',
    'shim install --all'
)) {
    if (-not $content.Contains($required)) {
        throw "install.ps1 is missing required contract text: $required"
    }
}

Write-Output 'install.ps1 static contract passed'
