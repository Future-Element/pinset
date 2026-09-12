import * as path from "node:path";
import * as vscode from "vscode";
import type { EnvironmentDescriptor, PinsetContext, RuntimeDescriptor } from "./protocol";

interface PythonApi {
  ready: Promise<void>;
  environments: {
    getActiveEnvironmentPath(resource: vscode.Uri): { path: string };
    updateActiveEnvironmentPath(value: string, resource: vscode.Uri): Promise<void>;
  };
}
interface Binding { section: string; key: string; before: unknown; after: unknown; host: string; }
const host = (): string => `${vscode.env.remoteName ?? "local"}:${process.platform}:${process.arch}`;
const bindingKey = (folder: vscode.WorkspaceFolder): string => `bindings:${host()}:${folder.uri.toString()}`;
export const sameValue = (left: unknown, right: unknown): boolean => JSON.stringify(left ?? null) === JSON.stringify(right ?? null);

class Row extends vscode.TreeItem {
  constructor(label: string, readonly children: Row[] = [], description?: string, command?: string) {
    super(label, children.length ? vscode.TreeItemCollapsibleState.Expanded : vscode.TreeItemCollapsibleState.None);
    this.description = description;
    if (command) this.command = { command, title: label };
  }
}

export class EnvironmentPanel implements vscode.TreeDataProvider<Row>, vscode.Disposable {
  private readonly emitter = new vscode.EventEmitter<void>();
  readonly onDidChangeTreeData = this.emitter.event;
  constructor(private readonly context: (folder: vscode.WorkspaceFolder) => PinsetContext | undefined) {}
  changed(): void { this.emitter.fire(); }
  dispose(): void { this.emitter.dispose(); }
  getTreeItem(row: Row): vscode.TreeItem { return row; }
  getChildren(row?: Row): Row[] {
    if (row) return row.children;
    if (!vscode.workspace.isTrusted) return [new Row("Workspace Trust required")];
    return (vscode.workspace.workspaceFolders ?? []).map(folder => {
      const context = this.context(folder);
      const report = context?.descriptor;
      if (!report) return new Row(folder.name, [new Row(context ? "Environment panel requires Pinset 2.13+" : "Refresh Pinset status", [], undefined, "pinset.refresh")]);
      const children = [
        new Row("Host", [], `${host()} · CLI ${report.host} · ${report.target}`),
        new Row("Environment", [], report.environment_ready ? "Ready" : "Needs attention"),
        new Row("Execution", [], report.execution_verified ? "Requested probes verified" : "Not verified"),
        new Row("Profile", [], `${report.profile ?? "none"} (${report.profile_source})`),
        ...report.runtimes.map(runtime => new Row(runtime.tool, [
          new Row("Requested", [], runtime.requested), new Row("Locked", [], runtime.locked_version ?? "missing"),
          new Row("Executable", [], runtime.executable ?? "unavailable"),
          ...runtime.checks.map(check => new Row(check.id, [], `${check.state}: ${check.reason}`)),
        ], runtime.selection_source)),
        ...report.checks.filter(check => check.state !== "pass" && check.state !== "not_applicable")
          .map(check => new Row(check.id, [], `${check.state}: ${check.reason}${check.next_step ? `; ${check.next_step}` : ""}`)),
        ...report.evidence.map(evidence => new Row(`${evidence.tool} / ${evidence.entry}`, [], `${evidence.state}: ${evidence.reason}`)),
        new Row("Other debug / test / existing terminals", [], "Verify each entry separately"),
      ];
      return new Row(folder.name, children);
    });
  }
}

async function pythonApi(): Promise<PythonApi> {
  const extension = vscode.extensions.getExtension<PythonApi>("ms-python.python");
  if (!extension) throw new Error("Install or enable the Microsoft Python extension in this host first.");
  const api = extension.isActive ? extension.exports : await extension.activate();
  await api.ready;
  if (!api.environments?.updateActiveEnvironmentPath) throw new Error("Python extension environment API is unavailable.");
  return api;
}

async function current(folder: vscode.WorkspaceFolder, binding: Pick<Binding, "section" | "key">): Promise<unknown> {
  if (binding.section === "python-api") return (await pythonApi()).environments.getActiveEnvironmentPath(folder.uri).path;
  return vscode.workspace.getConfiguration(binding.section, folder.uri).inspect(binding.key)?.workspaceFolderValue;
}
async function write(folder: vscode.WorkspaceFolder, binding: Binding, value: unknown): Promise<void> {
  if (binding.section === "python-api") await (await pythonApi()).environments.updateActiveEnvironmentPath(String(value ?? ""), folder.uri);
  else await vscode.workspace.getConfiguration(binding.section, folder.uri).update(binding.key, value ?? undefined, vscode.ConfigurationTarget.WorkspaceFolder);
}

async function proposal(folder: vscode.WorkspaceFolder, runtime: RuntimeDescriptor): Promise<Binding> {
  if (!runtime.executable || !path.isAbsolute(runtime.executable)) throw new Error("Run Pinset setup before binding this runtime.");
  let section: string; let key: string; let after: unknown;
  if (runtime.tool === "python") { section = "python-api"; key = "activeEnvironment"; after = runtime.executable; }
  else if (runtime.tool === "flutter") {
    if (!vscode.extensions.getExtension("Dart-Code.flutter")) throw new Error("Install the Flutter extension in this host first.");
    section = "dart"; key = "flutterSdkPath"; after = path.dirname(path.dirname(runtime.executable));
  } else {
    section = "launch"; key = "configurations";
    const configurations = vscode.workspace.getConfiguration(section, folder.uri).get<vscode.DebugConfiguration[]>(key, []);
    if (configurations.some(config => config.name === "Pinset: Node")) throw new Error("A Pinset: Node launch entry already exists. Restore or rename it before binding again.");
    after = [...configurations, { name: "Pinset: Node", type: "node", request: "launch", runtimeExecutable: runtime.executable, program: "${file}", cwd: "${workspaceFolder}", console: "integratedTerminal" }];
  }
  return { section, key, before: (await current(folder, { section, key })) ?? null, after, host: host() };
}

export async function bindEnvironment(folder: vscode.WorkspaceFolder, report: EnvironmentDescriptor, state: vscode.Memento, validateContext: () => Promise<void>): Promise<void> {
  const selected = await vscode.window.showQuickPick(report.runtimes.filter(runtime => ["node", "python", "flutter"].includes(runtime.tool))
    .map(runtime => ({ label: runtime.tool, description: runtime.executable ?? "not installed", runtime })), { placeHolder: `Bind a runtime in ${folder.name}` });
  if (!selected) return;
  const bindings = state.get<Binding[]>(bindingKey(folder), []);
  const binding = await proposal(folder, selected.runtime);
  if (bindings.some(old => old.section === binding.section && old.key === binding.key)) throw new Error("Restore the previous Pinset binding before replacing it.");
  const answer = await vscode.window.showWarningMessage(`Update ${binding.section}.${binding.key} for ${folder.name}?`,
    { modal: true, detail: `Before: ${JSON.stringify(binding.before)}\nAfter: ${JSON.stringify(binding.after)}\nExisting terminals must be reopened. Debug/test execution remains unverified.` }, "Apply Binding");
  if (answer !== "Apply Binding") return;
  await validateContext();
  if (!sameValue(await current(folder, binding), binding.before)) throw new Error("Settings changed while previewing. Review the binding again.");
  // Record before writing so interruption cannot erase the ownership boundary.
  await state.update(bindingKey(folder), [...bindings, binding]);
  await write(folder, binding, binding.after);
  if (!sameValue(await current(folder, binding), binding.after)) throw new Error("The language extension did not retain the requested binding.");
  void vscode.window.showInformationMessage("Binding saved. Open a new terminal and verify native debug/test entries separately.");
}

export async function restoreBindings(folder: vscode.WorkspaceFolder, state: vscode.Memento): Promise<void> {
  const key = bindingKey(folder);
  const bindings = state.get<Binding[]>(key, []);
  const retained: Binding[] = [];
  for (const binding of bindings) {
    const value = await current(folder, binding);
    if (sameValue(value, binding.before)) continue;
    if (binding.host !== host() || !sameValue(value, binding.after)) { retained.push(binding); continue; }
    await write(folder, binding, binding.before);
    // Commit progress after every field, allowing partial restoration to resume safely.
    await state.update(key, [...retained, ...bindings.slice(bindings.indexOf(binding) + 1)]);
  }
  await state.update(key, retained);
  void vscode.window.showInformationMessage(retained.length ? "Some bindings were changed outside Pinset; those values were preserved." : "Pinset bindings restored.");
}
