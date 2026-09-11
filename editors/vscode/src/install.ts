import * as path from "node:path";

export const INSTALL_GUIDE_URL = "https://pinset.future-element.com/#install";
const INSTALL_SCRIPT_URL = "https://raw.githubusercontent.com/Future-Element/pinset/main/install.ps1";
const INSTALL_SHELL_URL = "https://raw.githubusercontent.com/Future-Element/pinset/main/install.sh";

export interface InstallerPlan {
  command: string;
  executablePath?: string;
  shellPath?: string;
}

export function installerPlan(
  platform: NodeJS.Platform,
  environment: NodeJS.ProcessEnv,
): InstallerPlan | undefined {
  if (platform === "win32") {
    const localAppData = environment.LOCALAPPDATA;
    if (!localAppData) return undefined;
    const executablePath = path.win32.join(localAppData, "Pinset", "bin", "pinset.exe");
    return {
      executablePath,
      shellPath: "powershell.exe",
      command: [
        "$pinsetInstaller = Join-Path ([IO.Path]::GetTempPath()) ('pinset-install-' + [guid]::NewGuid().ToString('N') + '.ps1')",
        `Invoke-WebRequest -Uri '${INSTALL_SCRIPT_URL}' -OutFile $pinsetInstaller`,
        "try { & $pinsetInstaller } finally { Remove-Item -LiteralPath $pinsetInstaller -Force -ErrorAction SilentlyContinue }",
      ].join("; "),
    };
  }

  const home = environment.HOME;
  if (!home) return undefined;
  return {
    executablePath: path.posix.join(home, ".local", "bin", "pinset"),
    command: `curl -fsSL ${INSTALL_SHELL_URL} | sh`,
  };
}
