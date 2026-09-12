import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import * as path from "node:path";

import * as vscode from "vscode";

import { diagnosticDocument } from "./diagnostic-model";
import { EnvironmentPanel, bindEnvironment, restoreBindings } from "./environment-panel";
import { INSTALL_GUIDE_URL, installerPlan } from "./install";
import { isMissingExecutableError, PinsetProcessError, processStartError } from "./process-error";
import { parseContext, validateDescriptor, type PinsetContext, type PinsetFinding } from "./protocol";
import { terminateProcessTree, terminationPlan } from "./process-tree";
import { statusModel, type StatusItemModel } from "./status-model";

export { parseContext } from "./protocol";
export { terminateProcessTree, terminationPlan } from "./process-tree";

const MAX_CAPTURE_BYTES = 4 * 1024 * 1024;

interface CapturedProcess {
  stdout: string;
  stderr: string;
}

function executable(): string {
  return vscode.workspace.getConfiguration("pinset").get<string>("executablePath", "pinset");
}

async function runCli(
  folder: vscode.WorkspaceFolder,
  args: readonly string[],
  token?: vscode.CancellationToken,
  timeoutMs = 30_000,
): Promise<CapturedProcess> {
  return new Promise<CapturedProcess>((resolve, reject) => {
    const child = spawn(executable(), [...args], {
      cwd: folder.uri.fsPath,
      detached: process.platform !== "win32",
      env: { ...process.env, PINSET_EDITOR: "1" },
      windowsHide: true,
    });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    let capturedBytes = 0;
    let overflow = false;
    let cancelled = false;
    let timedOut = false;
    const timeout = setTimeout(() => { timedOut = true; void terminate(child); }, timeoutMs);

    const capture = (destination: Buffer[], chunk: Buffer): void => {
      capturedBytes += chunk.byteLength;
      if (capturedBytes > MAX_CAPTURE_BYTES) {
        overflow = true;
        void terminate(child);
        return;
      }
      destination.push(chunk);
    };
    child.stdout.on("data", (chunk: Buffer) => capture(stdout, chunk));
    child.stderr.on("data", (chunk: Buffer) => capture(stderr, chunk));
    const cancellation = token?.onCancellationRequested(() => {
      cancelled = true;
      void terminate(child);
    });
    child.once("error", (error: NodeJS.ErrnoException) => {
      clearTimeout(timeout);
      cancellation?.dispose();
      reject(processStartError(error));
    });
    child.once("close", (code, signal) => {
      clearTimeout(timeout);
      cancellation?.dispose();
      const standardOutput = Buffer.concat(stdout).toString("utf8");
      const standardError = Buffer.concat(stderr).toString("utf8").trim();
      if (timedOut) {
        reject(new PinsetProcessError("Pinset timed out; the process tree was stopped", ""));
      } else if (overflow) {
        reject(new PinsetProcessError("Pinset output exceeded the 4 MiB editor limit", standardError));
      } else if (cancelled) {
        reject(new vscode.CancellationError());
      } else if (code !== 0) {
        reject(
          new PinsetProcessError(
            standardError || `Pinset exited with ${code ?? signal ?? "an unknown status"}`,
            standardError,
          ),
        );
      } else {
        resolve({ stdout: standardOutput, stderr: standardError });
      }
    });
  });
}

async function terminate(child: ChildProcessWithoutNullStreams): Promise<void> {
  if (child.pid !== undefined) await terminateProcessTree(child.pid);
}

class ContextStore {
  private readonly contexts = new Map<string, PinsetContext>();
  private readonly errors = new Map<string, string>();
  private readonly generations = new Map<string, number>();

  constructor(private readonly extensionVersion: string) {}

  get(folder: vscode.WorkspaceFolder): PinsetContext | undefined {
    return this.contexts.get(folder.uri.toString());
  }

  error(folder: vscode.WorkspaceFolder): string | undefined {
    return this.errors.get(folder.uri.toString());
  }

  values(): Iterable<PinsetContext> {
    return this.contexts.values();
  }

  clear(): void {
    this.contexts.clear();
    this.errors.clear();
    for (const [key, generation] of this.generations) this.generations.set(key, generation + 1);
  }

  async refresh(folder: vscode.WorkspaceFolder, token?: vscode.CancellationToken): Promise<PinsetContext> {
    const key = folder.uri.toString();
    const generation = (this.generations.get(key) ?? 0) + 1;
    this.generations.set(key, generation);
    try {
      const result = await runCli(folder, ["editor", "context", "--cwd", folder.uri.fsPath, "--json"], token);
      let context = parseContext(result.stdout, this.extensionVersion);
      const [major = 0, minor = 0] = context.cli_version.split(".").map(Number);
      if (major > 2 || (major === 2 && minor >= 13)) {
        const upgraded = await runCli(folder, ["editor", "context", "--protocol", "2", "--cwd", folder.uri.fsPath, "--json"], token);
        context = parseContext(upgraded.stdout, this.extensionVersion);
      }
      if (this.generations.get(key) !== generation) throw new vscode.CancellationError();
      this.contexts.set(key, context);
      this.errors.delete(key);
      return context;
    } catch (error) {
      if (this.generations.get(key) !== generation) throw new vscode.CancellationError();
      this.contexts.delete(key);
      const message = errorMessage(error);
      this.errors.set(key, message);
      throw error;
    }
  }
}

class PinsetTerminal implements vscode.Pseudoterminal {
  private readonly writeEmitter = new vscode.EventEmitter<string>();
  private readonly closeEmitter = new vscode.EventEmitter<number | void>();
  private child: ChildProcessWithoutNullStreams | undefined;
  private closed = false;

  readonly onDidWrite = this.writeEmitter.event;
  readonly onDidClose = this.closeEmitter.event;

  constructor(
    private readonly folder: vscode.WorkspaceFolder,
    private readonly task: string,
  ) {}

  open(): void {
    if (!vscode.workspace.isTrusted) {
      this.writeEmitter.fire("Pinset tasks require a trusted workspace.\r\n");
      this.finish(1);
      return;
    }
    this.child = spawn(executable(), ["-C", this.folder.uri.fsPath, "run", this.task], {
      cwd: this.folder.uri.fsPath,
      detached: process.platform !== "win32",
      env: { ...process.env, PINSET_EDITOR: "1" },
      windowsHide: true,
    });
    this.child.stdout.on("data", (chunk: Buffer) => this.writeEmitter.fire(terminalText(chunk)));
    this.child.stderr.on("data", (chunk: Buffer) => this.writeEmitter.fire(terminalText(chunk)));
    this.child.once("error", (error) => {
      this.writeEmitter.fire(`Could not start Pinset: ${error.message}\r\n`);
      this.finish(1);
    });
    this.child.once("close", (code, signal) => {
      if (signal) this.writeEmitter.fire(`Pinset task stopped by ${signal}.\r\n`);
      this.finish(code ?? (signal ? 1 : 0));
    });
  }

  close(): void {
    const child = this.child;
    if (child?.pid !== undefined) void terminateProcessTree(child.pid);
  }

  private finish(code: number): void {
    if (this.closed) return;
    this.closed = true;
    this.closeEmitter.fire(code);
    this.writeEmitter.dispose();
    this.closeEmitter.dispose();
  }
}

class PinsetTaskProvider implements vscode.TaskProvider, vscode.Disposable {
  private readonly changeEmitter = new vscode.EventEmitter<void>();
  readonly onDidChangeTasks = this.changeEmitter.event;

  constructor(private readonly store: ContextStore) {}

  changed(): void {
    this.changeEmitter.fire();
  }

  dispose(): void {
    this.changeEmitter.dispose();
  }

  provideTasks(): vscode.Task[] {
    if (!vscode.workspace.isTrusted) return [];
    return (vscode.workspace.workspaceFolders ?? []).flatMap((folder) => this.tasksFor(folder));
  }

  resolveTask(task: vscode.Task): vscode.Task | undefined {
    if (!vscode.workspace.isTrusted) return undefined;
    const name = typeof task.definition.task === "string" ? task.definition.task : undefined;
    const folder = this.folderForDefinition(task.definition.folder, task.scope);
    if (!name || !folder || !this.store.get(folder)?.tasks.some((item) => item.name === name)) return undefined;
    return this.task(folder, name);
  }

  private tasksFor(folder: vscode.WorkspaceFolder): vscode.Task[] {
    return (this.store.get(folder)?.tasks ?? []).map((task) => this.task(folder, task.name));
  }

  task(folder: vscode.WorkspaceFolder, name: string): vscode.Task {
    const definition: vscode.TaskDefinition = { type: "pinset", task: name, folder: folder.uri.toString() };
    const execution = new vscode.CustomExecution(async () => new PinsetTerminal(folder, name));
    return new vscode.Task(definition, folder, name, "pinset", execution, []);
  }

  private folderForDefinition(
    value: unknown,
    scope: vscode.TaskScope | vscode.WorkspaceFolder | undefined,
  ): vscode.WorkspaceFolder | undefined {
    if (typeof value === "string") {
      return vscode.workspace.workspaceFolders?.find((folder) => folder.uri.toString() === value);
    }
    return typeof scope === "object" && "uri" in scope ? scope : undefined;
  }
}

class PinsetStatus implements vscode.Disposable {
  private readonly primary = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 52);
  private readonly tools = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 51);
  private readonly environment = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 50);

  loading(): void {
    this.set(this.primary, {
      text: "$(sync~spin) Pinset",
      tooltip: "Refreshing Pinset project status…",
      command: "pinset.refresh",
    });
    this.tools.hide();
    this.environment.hide();
  }

  update(context: PinsetContext): void {
    const model = statusModel(context);
    this.set(this.primary, model.primary);
    this.setOptional(this.tools, model.tools);
    this.setOptional(this.environment, model.environment);
  }

  error(message: string): void {
    this.set(this.primary, {
      text: "$(error) Pinset unavailable",
      tooltip: message,
      command: "pinset.checkDiagnostics",
    });
    this.tools.hide();
    this.environment.hide();
  }

  missingCli(message: string): void {
    this.set(this.primary, {
      text: "$(cloud-download) Pinset: Install CLI",
      tooltip: `${message}\nInstall Pinset in this local or remote extension host.`,
      command: "pinset.install",
    });
    this.tools.hide();
    this.environment.hide();
  }

  untrusted(): void {
    this.set(this.primary, {
      text: "$(lock) Pinset: Workspace not trusted",
      tooltip: "Trust this workspace before Pinset reads project configuration or starts tasks.",
      command: "pinset.checkDiagnostics",
    });
    this.tools.hide();
    this.environment.hide();
  }

  hide(): void {
    this.primary.hide();
    this.tools.hide();
    this.environment.hide();
  }

  dispose(): void {
    this.primary.dispose();
    this.tools.dispose();
    this.environment.dispose();
  }

  private setOptional(item: vscode.StatusBarItem, model: StatusItemModel | undefined): void {
    if (model) this.set(item, model);
    else item.hide();
  }

  private set(item: vscode.StatusBarItem, model: StatusItemModel): void {
    item.text = model.text;
    item.tooltip = model.tooltip;
    item.command = model.command;
    item.show();
  }
}

export async function activate(extensionContext: vscode.ExtensionContext): Promise<void> {
  const extensionVersion = String(extensionContext.extension.packageJSON.version ?? "0.0.0");
  const store = new ContextStore(extensionVersion);
  const diagnostics = vscode.languages.createDiagnosticCollection("pinset");
  const output = vscode.window.createOutputChannel("Pinset");
  const status = new PinsetStatus();
  const taskProvider = new PinsetTaskProvider(store);
  const panel = new EnvironmentPanel(folder => store.get(folder));

  const refreshFolder = async (folder: vscode.WorkspaceFolder, token?: vscode.CancellationToken): Promise<void> => {
    if (!vscode.workspace.isTrusted) {
      status.untrusted();
      diagnostics.clear();
      return;
    }
    if (folder === displayedFolder()) status.loading();
    try {
      const context = await store.refresh(folder, token);
      publishDiagnostics(diagnostics, store.values());
      taskProvider.changed();
      panel.changed();
      if (folder === displayedFolder()) status.update(context);
    } catch (error) {
      if (error instanceof vscode.CancellationError) return;
      output.appendLine(`[${folder.name}] ${errorMessage(error)}`);
      panel.changed();
      publishDiagnostics(diagnostics, store.values());
      if (folder === displayedFolder()) {
        if (isMissingExecutableError(error)) status.missingCli(errorMessage(error));
        else status.error(errorMessage(error));
      }
    }
  };

  const refreshAll = async (): Promise<void> => {
    if (!vscode.workspace.isTrusted) {
      status.untrusted();
      diagnostics.clear();
      return;
    }
    const folders = vscode.workspace.workspaceFolders ?? [];
    if (folders.length === 0) {
      status.hide();
      return;
    }
    await Promise.all(folders.map((folder) => refreshFolder(folder)));
  };

  extensionContext.subscriptions.push(
    status,
    diagnostics,
    output,
    taskProvider,
    panel,
    vscode.window.registerTreeDataProvider("pinset.environments", panel),
    vscode.commands.registerCommand("pinset.prepareEnvironment", async () => {
      const folder = await trustedFolder();
      if (!folder) return;
      try {
        const preview = await runCli(folder, ["setup", "--plan", "--json"]);
        const plan = JSON.parse(preview.stdout).data;
        const answer = await vscode.window.showWarningMessage(`Prepare ${folder.name}?`, { modal: true,
          detail: `Steps: ${plan.steps.map((step: { id: string }) => step.id).join(", ")}\nDeclared tasks require a separate explicit action.` }, "Prepare");
        if (answer !== "Prepare") return;
        await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification, title: "Preparing Pinset environment", cancellable: true },
          async (_progress, token) => { const result = await runCli(folder, ["setup", "--yes", "--json"], token, 20 * 60_000); output.appendLine(result.stdout); });
      } catch (error) { void vscode.window.showErrorMessage(errorMessage(error)); }
      await refreshFolder(folder);
    }),
    vscode.commands.registerCommand("pinset.probeEnvironment", async () => {
      const folder = await trustedFolder();
      if (!folder) return;
      try {
        await refreshFolder(folder);
        const context = store.get(folder);
        if (!context?.descriptor) throw new Error("Environment probes require Pinset 2.13 or newer.");
        const fingerprint = context.descriptor.context_fingerprint;
        const result = await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification, title: "Probing selected runtimes", cancellable: true },
          async (_progress, token) => runCli(folder, ["status", "--report-version", "2", "--probe", "--json"], token));
        const report: unknown = JSON.parse(result.stdout).data.report;
        validateDescriptor(report);
        if (store.get(folder) !== context || context.descriptor.context_fingerprint !== fingerprint) throw new Error("Project changed while probing; refresh and retry.");
        context.descriptor.evidence = report.evidence;
        context.descriptor.execution_verified = report.execution_verified;
        context.descriptor.environment_ready = report.environment_ready;
        panel.changed();
      } catch (error) { void vscode.window.showErrorMessage(errorMessage(error)); }
    }),
    vscode.commands.registerCommand("pinset.bindEnvironment", async () => {
      const folder = await trustedFolder();
      if (!folder) return;
      try { const context = await store.refresh(folder); if (!context.descriptor) throw new Error("Binding requires Pinset 2.13 or newer.");
        await bindEnvironment(folder, context.descriptor, extensionContext.workspaceState); await refreshFolder(folder);
      } catch (error) { void vscode.window.showErrorMessage(errorMessage(error)); }
    }),
    vscode.commands.registerCommand("pinset.restoreBindings", async () => {
      const folder = await trustedFolder();
      if (!folder) return;
      try { await restoreBindings(folder, extensionContext.workspaceState); await refreshFolder(folder); }
      catch (error) { void vscode.window.showErrorMessage(errorMessage(error)); }
    }),
    vscode.tasks.registerTaskProvider("pinset", taskProvider),
    vscode.commands.registerCommand("pinset.refresh", refreshAll),
    vscode.commands.registerCommand("pinset.install", async () => {
      const folder = await trustedFolder();
      if (!folder) return;
      const action = await vscode.window.showWarningMessage(
        "Pinset CLI was not found in this extension host. Run the official installer in a new terminal?",
        { modal: true, detail: "The installer downloads a Pinset release from GitHub and verifies its SHA-256 checksum before installing it." },
        "Install in Terminal",
        "Open Install Guide",
      );
      if (action === "Open Install Guide") {
        await vscode.env.openExternal(vscode.Uri.parse(INSTALL_GUIDE_URL));
        return;
      }
      if (action !== "Install in Terminal") return;
      const plan = installerPlan(process.platform, process.env);
      if (!plan) {
        await vscode.env.openExternal(vscode.Uri.parse(INSTALL_GUIDE_URL));
        void vscode.window.showInformationMessage("The automatic install path is unavailable in this host. The Pinset installation guide has been opened.");
        return;
      }
      if (plan.executablePath) {
        await vscode.workspace
          .getConfiguration("pinset")
          .update("executablePath", plan.executablePath, vscode.ConfigurationTarget.Global);
      }
      const terminal = vscode.window.createTerminal({
        name: "Pinset Installer",
        cwd: folder.uri,
        shellPath: plan.shellPath,
      });
      terminal.show();
      terminal.sendText(plan.command, true);
      void vscode.window.showInformationMessage("Pinset installer started. When it finishes, select Pinset: Refresh Status.");
    }),
    vscode.commands.registerCommand("pinset.initializeProject", async () => {
      const folder = await trustedFolder();
      if (!folder) return;
      try {
        const result = await vscode.window.withProgress(
          { location: vscode.ProgressLocation.Notification, title: `Initializing Pinset in ${folder.name}` },
          async () => runCli(folder, ["init"]),
        );
        output.appendLine(`[${folder.name}] ${result.stdout.trim()}`);
        await refreshFolder(folder);
        const open = await vscode.window.showInformationMessage(
          `Pinset initialized in ${folder.name}.`,
          "Open pinset.toml",
        );
        if (open === "Open pinset.toml") {
          await vscode.window.showTextDocument(vscode.Uri.joinPath(folder.uri, "pinset.toml"));
        }
      } catch (error) {
        const message = errorMessage(error);
        output.appendLine(`[${folder.name}] ${message}`);
        void vscode.window.showErrorMessage(`Could not initialize Pinset: ${message}`);
        if (isMissingExecutableError(error)) status.missingCli(message);
        else status.error(message);
      }
    }),
    vscode.commands.registerCommand("pinset.selectEnvironment", async () => {
      const folder = await trustedFolder();
      if (!folder) return;
      const context = store.get(folder) ?? (await store.refresh(folder));
      const choices = [
        ...context.environment.profiles.map((profile) => ({ label: profile, description: profile === context.environment.selected ? "Current" : undefined })),
        { label: "$(discard) Reset local selection", description: "Use task or project defaults" },
      ];
      const selected = await vscode.window.showQuickPick(choices, { placeHolder: `Select Pinset environment for ${folder.name}` });
      if (!selected) return;
      if (selected.label.startsWith("$(discard)")) {
        await runCli(folder, ["env", "reset", "--cwd", folder.uri.fsPath]);
      } else {
        await runCli(folder, ["env", "use", selected.label, "--cwd", folder.uri.fsPath]);
      }
      await refreshFolder(folder);
    }),
    vscode.commands.registerCommand("pinset.runTask", async () => {
      const folder = await trustedFolder();
      if (!folder) return;
      const context = store.get(folder) ?? (await store.refresh(folder));
      const picked = await vscode.window.showQuickPick(
        context.tasks.map((task) => ({ label: task.name, description: task.description, task })),
        { placeHolder: `Run a Pinset task in ${folder.name}` },
      );
      if (!picked) return;
      await vscode.tasks.executeTask(taskProvider.task(folder, picked.task.name));
    }),
    vscode.commands.registerCommand("pinset.checkDiagnostics", async () => {
      const folder = await trustedFolder();
      if (!folder) return;
      await vscode.window.withProgress(
        { location: vscode.ProgressLocation.Notification, title: `Checking Pinset in ${folder.name}`, cancellable: true },
        async (_progress, token) => refreshFolder(folder, token),
      );
      const context = store.get(folder);
      if (!context) {
        void vscode.window.showErrorMessage(store.error(folder) ?? "Pinset diagnostics are unavailable");
        return;
      }
      output.appendLine(
        `[${folder.name}] diagnostics: ${context.diagnostics.summary.errors} error(s), ${context.diagnostics.summary.warnings} warning(s), ${context.diagnostics.summary.info} info`,
      );
      for (const finding of context.diagnostics.findings) {
        output.appendLine(`  ${finding.severity}: ${finding.code} (${finding.subject})`);
      }
      output.show(true);
    }),
    vscode.workspace.onDidGrantWorkspaceTrust(() => {
      store.clear();
      void refreshAll();
    }),
    vscode.workspace.onDidChangeWorkspaceFolders(() => {
      store.clear();
      void refreshAll();
    }),
    vscode.window.onDidChangeActiveTextEditor(() => {
      const folder = activeFolder();
      if (!vscode.workspace.isTrusted) status.untrusted();
      else if (folder && store.get(folder)) status.update(store.get(folder)!);
      else if (!folder && vscode.workspace.workspaceFolders?.length) {
        const first = vscode.workspace.workspaceFolders[0];
        const context = first ? store.get(first) : undefined;
        if (context) status.update(context);
      }
    }),
  );

  const watcher = vscode.workspace.createFileSystemWatcher("**/{pinset.toml,pinset.lock}");
  const refreshChanged = (uri: vscode.Uri): void => {
    const folder = vscode.workspace.getWorkspaceFolder(uri);
    if (folder) void refreshFolder(folder);
  };
  extensionContext.subscriptions.push(
    watcher,
    watcher.onDidCreate(refreshChanged),
    watcher.onDidChange(refreshChanged),
    watcher.onDidDelete(refreshChanged),
  );

  if (vscode.workspace.isTrusted) await refreshAll();
  else status.untrusted();
}

export function deactivate(): void {}

function publishDiagnostics(
  collection: vscode.DiagnosticCollection,
  contexts: Iterable<PinsetContext>,
): void {
  collection.clear();
  const grouped = new Map<string, { uri: vscode.Uri; values: vscode.Diagnostic[] }>();
  for (const context of contexts) {
    if (!context.config) continue;
    const configPath = context.config;
    const lockPath = path.join(path.dirname(configPath), "pinset.lock");
    for (const finding of context.diagnostics.findings) {
      const document = diagnosticDocument(finding.category);
      if (!document) continue;
      const uri = vscode.Uri.file(document === "config" ? configPath : lockPath);
      const key = uri.toString();
      const entry = grouped.get(key) ?? { uri, values: [] };
      const diagnostic = new vscode.Diagnostic(
        new vscode.Range(0, 0, 0, 1),
        `${finding.code}: ${finding.subject}`,
        severity(finding),
      );
      diagnostic.source = "Pinset";
      diagnostic.code = finding.code;
      entry.values.push(diagnostic);
      grouped.set(key, entry);
    }
  }
  for (const { uri, values } of grouped.values()) collection.set(uri, values);
}

function severity(finding: PinsetFinding): vscode.DiagnosticSeverity {
  if (finding.severity === "error") return vscode.DiagnosticSeverity.Error;
  if (finding.severity === "warning") return vscode.DiagnosticSeverity.Warning;
  return vscode.DiagnosticSeverity.Information;
}

async function trustedFolder(): Promise<vscode.WorkspaceFolder | undefined> {
  if (!vscode.workspace.isTrusted) {
    const action = await vscode.window.showWarningMessage(
      "Pinset requires Workspace Trust before it reads project configuration or starts tasks.",
      "Manage Workspace Trust",
    );
    if (action === "Manage Workspace Trust") await vscode.commands.executeCommand("workbench.trust.manage");
    return undefined;
  }
  const folders = vscode.workspace.workspaceFolders ?? [];
  if (folders.length === 0) {
    void vscode.window.showInformationMessage("Open a workspace folder to use Pinset.");
    return undefined;
  }
  const active = activeFolder();
  if (active) return active;
  if (folders.length === 1) return folders[0];
  const picked = await vscode.window.showQuickPick(
    folders.map((folder) => ({ label: folder.name, description: folder.uri.fsPath, folder })),
    { placeHolder: "Select a workspace folder" },
  );
  return picked?.folder;
}

function activeFolder(): vscode.WorkspaceFolder | undefined {
  const uri = vscode.window.activeTextEditor?.document.uri;
  return uri ? vscode.workspace.getWorkspaceFolder(uri) : undefined;
}

function displayedFolder(): vscode.WorkspaceFolder | undefined {
  return activeFolder() ?? vscode.workspace.workspaceFolders?.[0];
}

function terminalText(chunk: Buffer): string {
  return chunk.toString("utf8").replace(/(?<!\r)\n/g, "\r\n");
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
