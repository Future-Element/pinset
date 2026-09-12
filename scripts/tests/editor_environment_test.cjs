// Runs the packaged source in a real VS Code extension host on disposable CI runners.
const path = require("node:path");
const fs = require("node:fs/promises");
const { downloadAndUnzipVSCode, runTests, runVSCodeCommand } = require(path.join(process.env.PINSET_EDITOR_TEST_MODULES, "@vscode/test-electron"));

async function main() {
  const [cli, project, home] = process.argv.slice(2);
  const extensionDevelopmentPath = path.resolve(__dirname, "../../editors/vscode");
  const userData = path.join(home, "vscode-test-user");
  const extensionsDir = path.join(home, "vscode-test-extensions");
  await fs.mkdir(path.join(userData, "User"), { recursive: true });
  await fs.writeFile(path.join(userData, "User/settings.json"), JSON.stringify({
    "security.workspace.trust.enabled": false, "pinset.executablePath": cli,
    "telemetry.telemetryLevel": "off", "update.mode": "none", "extensions.autoUpdate": false,
  }));
  const version = "stable";
  const download = { version, cachePath: path.join(home, "vscode-download") };
  const vscodeExecutablePath = await downloadAndUnzipVSCode(download);
  await runVSCodeCommand(["--install-extension", "ms-python.python", "--extensions-dir", extensionsDir, "--user-data-dir", userData], download);
  await runVSCodeCommand(["--install-extension", "ms-python.debugpy", "--extensions-dir", extensionsDir, "--user-data-dir", userData], download);
  if (process.env.PINSET_EDITOR_TEST_FLUTTER_SDK) {
    await fs.mkdir(path.join(project, ".vscode"), { recursive: true });
    await fs.writeFile(path.join(project, ".vscode/settings.json"), JSON.stringify({ "dart.flutterSdkPath": process.env.PINSET_EDITOR_TEST_FLUTTER_SDK }));
    await runVSCodeCommand(["--install-extension", "Dart-Code.flutter", "--extensions-dir", extensionsDir, "--user-data-dir", userData], download);
  }
  await runTests({ vscodeExecutablePath, extensionDevelopmentPath,
    extensionTestsPath: path.resolve(__dirname, "editor_environment_suite.cjs"),
    launchArgs: [project, "--no-sandbox", "--disable-gpu", "--skip-welcome", "--skip-release-notes", "--user-data-dir", userData, "--extensions-dir", extensionsDir],
    extensionTestsEnv: { PINSET_HOME: home, PINSET_EDITOR_TEST_TOOLS: process.env.PINSET_EDITOR_TEST_TOOLS || "node,python" },
  });
}
main().catch(error => { console.error(error); process.exitCode = 1; });
