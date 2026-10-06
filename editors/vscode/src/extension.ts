import * as vscode from "vscode";
import { execFile } from "node:child_process";
import * as path from "node:path";
import { applyEdits, modify, parse, type ParseError } from "jsonc-parser";

type Plan = { protocol: string; tool: string; executable: string; sdk: string; version: string; project: string; environment?: Record<string,string> };
type Report = { protocol: string; report: { checks: { tool: string; installed: boolean; bound: boolean; actually_verified: boolean; detail: string }[] } };
let status: vscode.StatusBarItem;
let output: vscode.OutputChannel;

function cwd(): string | undefined {
  const editor = vscode.window.activeTextEditor;
  const folder = editor ? vscode.workspace.getWorkspaceFolder(editor.document.uri) : vscode.workspace.workspaceFolders?.[0];
  if (!folder || folder.uri.scheme !== "file") return undefined;
  return editor?.document.uri.scheme === "file" && editor.document.uri.fsPath.startsWith(folder.uri.fsPath + path.sep)
    ? path.dirname(editor.document.uri.fsPath) : folder.uri.fsPath;
}
function cli(): string { return vscode.workspace.getConfiguration("pinset").get<string>("executable", "pinset"); }
async function invoke<T>(args: string[]): Promise<T> {
  const directory = cwd();
  if (!directory) throw new Error("Open a local project first.");
  if (!vscode.workspace.isTrusted) throw new Error("Workspace Trust is required before starting Pinset.");
  const executable = cli();
  const result = await new Promise<string>((resolve, reject) => {
    const child = execFile(executable, ["-C", directory, "--json", ...args], { cwd: directory, timeout: args[0] === "install" ? 1_800_000 : args.includes("--probe") ? 300_000 : 60_000, maxBuffer: 2 * 1024 * 1024, windowsHide: true }, (error, stdout, stderr) => {
      if (error) reject(new Error(stdout.trim() || stderr.trim() || error.message));
      else resolve(stdout);
    });
    child.on("error", reject);
  });
  const value: unknown = JSON.parse(result);
  if (!value || typeof value !== "object" || !("protocol" in value) || value.protocol !== "pinset/3") throw new Error("Pinset 3 CLI is required.");
  return value as T;
}
async function refresh(): Promise<void> {
  if (!vscode.workspace.isTrusted) { status.text = "$(lock) Pinset"; status.tooltip = "Workspace Trust is required"; return; }
  try {
    const report = await invoke<Report>(["check"]);
    const checks = report.report.checks;
    status.text = `$(tools) Pinset ${checks.length}`;
    status.tooltip = checks.map(c => `${c.tool}: ${c.bound ? "bound" : c.installed ? "installed" : "not installed"}${c.actually_verified ? ", observed" : ""}`).join("\n");
  } catch (error) { status.text = "$(warning) Pinset"; status.tooltip = String(error); }
}
async function updateProjectSetting(resource: vscode.Uri, key: string, value: unknown): Promise<void> {
  const folder = vscode.workspace.getWorkspaceFolder(resource);
  if (!folder) throw new Error("Open a local project first.");
  const directory = vscode.Uri.joinPath(folder.uri, ".vscode");
  const settings = vscode.Uri.joinPath(directory, "settings.json");
  if (vscode.workspace.textDocuments.some(document => document.uri.toString() === settings.toString() && document.isDirty)) {
    throw new Error("Save project settings before binding a toolchain.");
  }
  let text = "{}\n";
  try { text = Buffer.from(await vscode.workspace.fs.readFile(settings)).toString("utf8"); }
  catch (error) { if (!(error instanceof vscode.FileSystemError) || error.code !== "FileNotFound") throw error; }
  const errors: ParseError[] = [];
  const existing: unknown = parse(text, errors, { allowTrailingComma: true });
  if (errors.length || !existing || typeof existing !== "object" || Array.isArray(existing)) throw new Error("Project settings must be a valid JSON object.");
  const updated = applyEdits(text, modify(text, [key], value, { formattingOptions: { insertSpaces: true, tabSize: 2, eol: "\n" } }));
  await vscode.workspace.fs.createDirectory(directory);
  await vscode.workspace.fs.writeFile(settings, Buffer.from(updated, "utf8"));
}
async function bind(): Promise<void> {
  const directory = cwd();
  if (!directory || !vscode.workspace.isTrusted) throw new Error("Open a trusted local project first.");
  const routes = await invoke<{ protocol: string; commands: Plan[] }>(["which"]);
  const plans = routes.commands;
  if (!Array.isArray(plans) || plans.some(p => p.protocol !== "pinset/3")) throw new Error("Invalid Pinset 3 routes");
  const resource = vscode.Uri.file(directory);
  for (const plan of plans) {
    if (plan.tool === "python") await updateProjectSetting(resource, "python.defaultInterpreterPath", plan.executable);
    if (plan.tool === "flutter") await updateProjectSetting(resource, "dart.flutterSdkPath", path.join(plan.project, ".pinset", "local", "flutter-sdk"));
    if (plan.tool === "rust") {
      const rust = vscode.workspace.getConfiguration("rust-analyzer", resource);
      const existing = rust.get<Record<string,string>>("cargo.extraEnv", {});
      await updateProjectSetting(resource, "rust-analyzer.cargo.extraEnv", { ...existing, RUSTC: plan.executable, RUSTUP_TOOLCHAIN: plan.sdk, ...(plan.environment?.PATH ? { PATH: plan.environment.PATH } : {}) });
    }
    if (plan.tool === "java") {
      const major = plan.version.split(".")[0];
      const name = major === "8" ? "JavaSE-1.8" : `JavaSE-${major}`;
      const java = vscode.workspace.getConfiguration("java", resource);
      const existing = java.get<{ name: string; path: string; default?: boolean }[]>("configuration.runtimes", []);
      const runtimes = existing.filter(runtime => runtime.name !== name).map(runtime => ({ ...runtime, default: false }));
      runtimes.push({ name, path: plan.sdk, default: true });
      await updateProjectSetting(resource, "java.configuration.runtimes", runtimes);
      output.appendLine(`Project JDK: ${plan.sdk}. Java language server JDK remains the Java extension's separate setting.`);
    }
  }
  output.show(true);
  await refresh();
}
export function activate(context: vscode.ExtensionContext): void {
  output = vscode.window.createOutputChannel("Pinset");
  status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 20);
  status.command = "pinset.check";
  status.show();
  const command = (name: string, action: () => Promise<unknown>): void => {
    context.subscriptions.push(vscode.commands.registerCommand(name, () => action().catch(error => { void vscode.window.showErrorMessage(String(error)); throw error; })));
  };
  command("pinset.refresh", refresh);
  command("pinset.install", async () => { const result = await invoke(["install"]); output.appendLine(JSON.stringify(result, null, 2)); output.show(true); await refresh(); return result; });
  command("pinset.check", async () => { const result = await invoke(["check"]); output.appendLine(JSON.stringify(result, null, 2)); output.show(true); return result; });
  command("pinset.probe", async () => {
    const result = await invoke(["check", "--probe"]); output.appendLine(JSON.stringify(result, null, 2)); output.show(true); await refresh(); return result;
  });
  command("pinset.bind", bind);
  const watcher = vscode.workspace.createFileSystemWatcher("**/.pinset/{config,lock}.toml");
  context.subscriptions.push(output, status, watcher, watcher.onDidCreate(() => void refresh()), watcher.onDidChange(() => void refresh()), watcher.onDidDelete(() => void refresh()), vscode.workspace.onDidGrantWorkspaceTrust(() => void refresh()), vscode.window.onDidChangeActiveTextEditor(() => void refresh()));
  void refresh();
}
export function deactivate(): void { /* VS Code disposes registered resources. */ }
