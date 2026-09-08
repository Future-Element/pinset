import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import * as path from "node:path";

import * as vscode from "vscode";

import { parseContext, type PinsetContext, type PinsetFinding } from "./protocol";
import { terminateProcessTree, terminationPlan } from "./process-tree";

export { parseContext } from "./protocol";
export { terminateProcessTree, terminationPlan } from "./process-tree";

const MAX_CAPTURE_BYTES = 4 * 1024 * 1024;

interface CapturedProcess {
  stdout: string;
  stderr: string;
}

class PinsetProcessError extends Error {
  constructor(
    message: string,
    readonly stderr: string,
  ) {
    super(message);
  }
}

function executable(): string {
  return vscode.workspace.getConfiguration("pinset").get<string>("executablePath", "pinset");
}

async function runCli(
  folder: vscode.WorkspaceFolder,
  args: readonly string[],
  token?: vscode.CancellationToken,
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
    child.once("error", (error) => {
      cancellation?.dispose();
      reject(new PinsetProcessError(`Could not start Pinset: ${error.message}`, ""));
    });
    child.once("close", (code, signal) => {
      cancellation?.dispose();
      const standardOutput = Buffer.concat(stdout).toString("utf8");
      const standardError = Buffer.concat(stderr).toString("utf8").trim();
      if (overflow) {
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

  constructor(private readonly extensionVersion: string) {}

  get(folder: vscode.WorkspaceFolder): PinsetContext | undefined {
    return this.contexts.get(folder.uri.toString());
  }

  error(folder: vscode.WorkspaceFolder): string | undefined {
    return this.errors.get(folder.uri.toString());
  }

  clear(): void {
    this.contexts.clear();
    this.errors.clear();
  }

  async refresh(folder: vscode.WorkspaceFolder, token?: vscode.CancellationToken): Promise<PinsetContext> {
    const key = folder.uri.toString();
    try {
      const result = await runCli(folder, ["editor", "context", "--cwd", folder.uri.fsPath, "--json"], token);
      const context = parseContext(result.stdout, this.extensionVersion);
      this.contexts.set(key, context);
      this.errors.delete(key);
      return context;
    } catch (error) {
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

export async function activate(extensionContext: vscode.ExtensionContext): Promise<void> {
  const extensionVersion = String(extensionContext.extension.packageJSON.version ?? "0.0.0");
  const store = new ContextStore(extensionVersion);
  const diagnostics = vscode.languages.createDiagnosticCollection("pinset");
  const output = vscode.window.createOutputChannel("Pinset");
  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 50);
  const taskProvider = new PinsetTaskProvider(store);
  status.command = "pinset.checkDiagnostics";
  status.show();

  const refreshFolder = async (folder: vscode.WorkspaceFolder, token?: vscode.CancellationToken): Promise<void> => {
    if (!vscode.workspace.isTrusted) {
      setUntrustedStatus(status);
      diagnostics.clear();
      return;
    }
    status.text = "$(sync~spin) Pinset";
    try {
      const context = await store.refresh(folder, token);
      publishDiagnostics(diagnostics, folder, context);
      taskProvider.changed();
      if (folder === displayedFolder()) updateStatus(status, folder, context);
    } catch (error) {
      if (error instanceof vscode.CancellationError) return;
      output.appendLine(`[${folder.name}] ${errorMessage(error)}`);
      if (folder === displayedFolder()) {
        status.text = "$(error) Pinset";
        status.tooltip = `Pinset: ${errorMessage(error)}`;
      }
    }
  };

  const refreshAll = async (): Promise<void> => {
    if (!vscode.workspace.isTrusted) {
      setUntrustedStatus(status);
      diagnostics.clear();
      return;
    }
    await Promise.all((vscode.workspace.workspaceFolders ?? []).map((folder) => refreshFolder(folder)));
  };

  extensionContext.subscriptions.push(
    status,
    diagnostics,
    output,
    taskProvider,
    vscode.tasks.registerTaskProvider("pinset", taskProvider),
    vscode.commands.registerCommand("pinset.refresh", refreshAll),
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
      if (!vscode.workspace.isTrusted) setUntrustedStatus(status);
      else if (folder && store.get(folder)) updateStatus(status, folder, store.get(folder)!);
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
  else setUntrustedStatus(status);
}

export function deactivate(): void {}

function publishDiagnostics(
  collection: vscode.DiagnosticCollection,
  folder: vscode.WorkspaceFolder,
  context: PinsetContext,
): void {
  const uri = context.config
    ? vscode.Uri.joinPath(
        folder.uri,
        ...path.relative(folder.uri.fsPath, context.config).split(path.sep),
      )
    : folder.uri;
  const values = context.diagnostics.findings.map((finding) => {
    const diagnostic = new vscode.Diagnostic(
      new vscode.Range(0, 0, 0, 1),
      `${finding.code}: ${finding.subject}`,
      severity(finding),
    );
    diagnostic.source = "Pinset";
    diagnostic.code = finding.code;
    return diagnostic;
  });
  collection.set(uri, values);
}

function severity(finding: PinsetFinding): vscode.DiagnosticSeverity {
  if (finding.severity === "error") return vscode.DiagnosticSeverity.Error;
  if (finding.severity === "warning") return vscode.DiagnosticSeverity.Warning;
  return vscode.DiagnosticSeverity.Information;
}

function updateStatus(status: vscode.StatusBarItem, folder: vscode.WorkspaceFolder, context: PinsetContext): void {
  const summary = context.diagnostics.summary;
  const icon = summary.errors > 0 ? "error" : summary.warnings > 0 ? "warning" : "check";
  const profile = context.environment.selected ?? "default";
  status.text = `$(${icon}) Pinset: ${profile}`;
  status.tooltip = `${folder.name}: ${summary.errors} error(s), ${summary.warnings} warning(s)`;
}

function setUntrustedStatus(status: vscode.StatusBarItem): void {
  status.text = "$(lock) Pinset: Workspace not trusted";
  status.tooltip = "Trust this workspace before Pinset reads project configuration or starts tasks.";
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
