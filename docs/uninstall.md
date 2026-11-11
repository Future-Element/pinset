# Completely uninstall Pinset

[简体中文](uninstall.zh-CN.md)

The standalone root scripts support macOS/Linux (`uninstall.sh`) and Windows PowerShell (`uninstall.ps1`). They do not require a working Pinset CLI. Remote commands below fetch `main`.

## macOS / Linux

Preview, then remove:

```sh
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/uninstall.sh | sh -s -- --plan
curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/uninstall.sh | sh -s -- --yes
```

Alternatively, download first to inspect the script and ensure the download completes before execution:

```sh
script=$(mktemp) && curl -fL https://raw.githubusercontent.com/Future-Element/pinset/main/uninstall.sh -o "$script" && sh "$script" --yes
```

Additional locations can be supplied repeatedly:

```sh
sh uninstall.sh --plan --pinset-home /opt/my-pinset --install-dir "$HOME/.local/bin" --project /path/to/project
sh uninstall.sh --yes --pinset-home /opt/my-pinset --install-dir "$HOME/.local/bin" --project /path/to/project
```

## Windows

Run in PowerShell. Use `curl.exe` to avoid the Windows PowerShell alias:

```powershell
$script = Join-Path ([IO.Path]::GetTempPath()) 'pinset-uninstall.ps1'
curl.exe -fL https://raw.githubusercontent.com/Future-Element/pinset/main/uninstall.ps1 -o $script
if ($LASTEXITCODE -ne 0) { throw 'Uninstaller download failed' }
& $script -Plan
& $script -Yes
```

To download into memory and execute directly, check curl's exit code before running the downloaded source:

```powershell
$pinsetUninstall = curl.exe -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/uninstall.ps1 | Out-String; if ($LASTEXITCODE -ne 0) { throw 'Download failed' }; & ([scriptblock]::Create($pinsetUninstall)) -Yes
```

If execution policy prevents running the downloaded file, use `powershell.exe -NoProfile -ExecutionPolicy Bypass -File $script -Yes`. This changes the policy for that child process only.

Additional locations accept arrays:

```powershell
& .\uninstall.ps1 -Plan -PinsetHome 'D:\Pinset' -InstallDir 'D:\Tools\bin' -Project 'D:\Code\my-project'
```

Remove the temporary downloaded script after running it if desired.

## Scope and verification

The scripts remove identified Pinset CLI/router/shims, managed SDKs and generic tools, v3/v2 state, caches, global configuration, migration backups, Pinset environment variables and PATH entries, supported shell initialization/completion entries, and Pinset identities from the native credential store. Registered or explicitly supplied projects also have their Pinset configuration, encrypted profiles, local state and owned virtual environments removed. Removing identities can make encrypted profiles kept elsewhere impossible to decrypt.

`--plan` / `-Plan` makes no changes, including to credentials and environment variables. Without `--yes` / `-Yes`, an interactive confirmation is required. Unsafe paths, unknown ownership, unsupported configuration, permission failures and busy files are reported as incomplete work.

Discovery covers the current user and known locations/registered projects. Supply any undiscoverable custom homes, command directories and unregistered projects explicitly; the scripts do not scan every disk or other users' homes. Shared caches such as `~/.cargo`, `~/.npm` and `~/.pub-cache`, independent SDKs, build outputs and application dependencies are not exclusively owned by Pinset and are preserved.

Homebrew, WinGet and Scoop package removal is supported. Remove aqua project declarations separately; aqua-managed files are reported for attention. Linux native identity cleanup uses `secret-tool`; if identity metadata exists without that utility, the script stops before deleting data. Ambiguous shell commands are retained and reported with a nonzero exit status.

Separately installed editor extensions, IDE settings and CI configuration need cleanup in their own configuration. For VS Code, use `code --uninstall-extension FutureElement.pinset-vscode` and check `.vscode/settings.json` for Python, Flutter, Rust and Java paths pointing to Pinset. Arbitrarily named shell fragments may also require review.

Close and reopen terminals after removal. A script run through a pipe or child process cannot rewrite its parent process or other open terminals' environments. System installations and machine environment variables require the corresponding permissions; resolve any reported failures and rerun.

Validation used isolated Linux (WSL), Windows PowerShell 5.1 and PowerShell 7 fixtures. Tests replace native credential/package calls and mutate temporary fixture directories only. Native macOS execution, actual package removal and real credential deletion remain unverified. No actual uninstall was performed on the current machine.
