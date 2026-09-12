import { randomUUID } from "node:crypto";
import { promises as fs } from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";
import type { EnvironmentDescriptor, RuntimeDescriptor } from "./protocol";

export interface DebugEvidence { tool: string; entry: string; state: "pass" | "fail" | "unknown"; reason: string; observed_version?: string; }

/** Observe only this controlled launch, never general user debug sessions or their output. */
export async function debugProbe(folder: vscode.WorkspaceFolder, runtime: RuntimeDescriptor, storage: vscode.Uri,
  token: vscode.CancellationToken): Promise<DebugEvidence> {
  const result = (state: DebugEvidence["state"], reason: string, version?: string): DebugEvidence =>
    ({ tool: runtime.tool, entry: "pinset-debug-probe", state, reason, observed_version: version });
  if (!runtime.executable) return result("unknown", "managed_runtime_unavailable");
  const language = runtime.tool;
  const extensionId = language === "python" ? "ms-python.debugpy" : language === "flutter" ? "Dart-Code.dart-code" : "ms-vscode.js-debug";
  const extension = vscode.extensions.getExtension(extensionId);
  if (!extension) return result("unknown", "native_debug_extension_unavailable");
  await extension.activate();
  const nonce = randomUUID();
  const marker = `PINSET_PROBE_${nonce}:`;
  const directory = vscode.Uri.joinPath(storage, "probes", nonce);
  await vscode.workspace.fs.createDirectory(directory);
  const suffix = language === "python" ? "py" : language === "flutter" ? "dart" : "cjs";
  const file = vscode.Uri.joinPath(directory, `probe.${suffix}`);
  const observationFile = vscode.Uri.joinPath(directory, "observation.json");
  const expected = language === "flutter"
    ? path.join(path.dirname(path.dirname(runtime.executable)), "bin", "cache", "dart-sdk", "bin", process.platform === "win32" ? "dart.exe" : "dart")
    : runtime.executable;
  const source = language === "node" ? `const info={executable:process.execPath,version:process.versions.node};require('fs').writeFileSync(${JSON.stringify(observationFile.fsPath)},JSON.stringify(info));console.log(${JSON.stringify(marker)}+JSON.stringify(info));`
    : language === "python" ? `import sys,json\ninfo=dict(executable=sys.executable,version='.'.join(map(str,sys.version_info[:3])))\nwith open(${JSON.stringify(observationFile.fsPath)},'w') as output: json.dump(info,output)\nprint(${JSON.stringify(marker)}+json.dumps(info))\n`
    : `import 'dart:io'; import 'dart:convert'; void main() { final info = {'executable': Platform.resolvedExecutable, 'version': Platform.version.split(' ').first}; File(${JSON.stringify(observationFile.fsPath)}).writeAsStringSync(jsonEncode(info)); print('${marker}' + jsonEncode(info)); }`;
  await vscode.workspace.fs.writeFile(file, Buffer.from(source));
  const emptyEnvironment = vscode.Uri.joinPath(directory, "probe.env");
  await vscode.workspace.fs.writeFile(emptyEnvironment, Buffer.from(""));
  const name = `Pinset runtime probe ${nonce}`;
  const config: vscode.DebugConfiguration = { name, type: language === "node" ? "node" : language === "python" ? "debugpy" : "dart",
    request: "launch", program: file.fsPath, cwd: folder.uri.fsPath, console: "internalConsole", noDebug: false,
    env: { NODE_OPTIONS: "", NODE_PATH: "", PYTHONPATH: "", PYTHONSTARTUP: "", PYTHONINSPECT: "" }, envFile: emptyEnvironment.fsPath };
  if (language === "node") { config.runtimeExecutable = expected; config.outputCapture = "std"; }
  if (language === "python") { config.python = expected; config.redirectOutput = true; }
  // Dart Code resolves the SDK through the explicitly bound workspace, then reports the process it launched.
  if (language === "flutter") { config.flutterMode = "debug"; }
  let session: vscode.DebugSession | undefined;
  let observed: { executable: string; version: string } | undefined;
  let resolveDone: (() => void) | undefined;
  const done = new Promise<void>(resolve => { resolveDone = resolve; });
  const readObservation = async (): Promise<void> => {
    try {
      const stat = await fs.lstat(observationFile.fsPath);
      if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 64 * 1024) return;
      const value = JSON.parse(await fs.readFile(observationFile.fsPath, "utf8"));
      if (typeof value.executable === "string" && typeof value.version === "string") { observed = value; resolveDone?.(); }
    } catch { /* The controlled process has not committed its observation yet. */ }
  };
  const observationTimer = setInterval(() => void readObservation(), 100);
  let text = "";
  const tracker = vscode.debug.registerDebugAdapterTrackerFactory("*", {
    createDebugAdapterTracker(candidate) {
      if (candidate.name !== name && candidate.configuration.program !== file.fsPath) return undefined;
      session = candidate;
      return { onDidSendMessage(message) {
        if (message.type !== "event" || message.event !== "output" || typeof message.body?.output !== "string") return;
        text = (text + message.body.output).slice(-64 * 1024);
        const line = text.split(/\r?\n/).find(value => value.startsWith(marker));
        if (!line) return;
        try { const value = JSON.parse(line.slice(marker.length));
          if (typeof value.executable === "string" && typeof value.version === "string") { observed = value; resolveDone?.(); }
        } catch { /* The output event may split a line; wait for its remainder. */ }
      }, onError() { resolveDone?.(); }, onExit() { resolveDone?.(); } };
    },
  });
  const start = vscode.debug.onDidStartDebugSession(started => { if (started.name === name || started.configuration.program === file.fsPath) session = started; });
  const termination = vscode.debug.onDidTerminateDebugSession(ended => { if (ended === session) resolveDone?.(); });
  const cancellation = token.onCancellationRequested(() => { resolveDone?.(); if (session) void vscode.debug.stopDebugging(session); });
  const timeout = setTimeout(() => resolveDone?.(), 30_000);
  try {
    if (token.isCancellationRequested) return result("unknown", "probe_cancelled");
    if (!await vscode.debug.startDebugging(folder, config)) return result("unknown", "debug_adapter_declined_launch");
    await done;
    await readObservation();
    if (token.isCancellationRequested) return result("unknown", "probe_cancelled");
    if (!observed) return result("unknown", "debug_process_not_observed");
    const identity = async (value: string): Promise<string> => {
      const result = language === "python" ? path.join(await fs.realpath(path.dirname(value)), path.basename(value)) : await fs.realpath(value);
      return process.platform === "win32" ? result.toLowerCase() : result;
    };
    const equal = await identity(expected).then(async expectedPath => expectedPath === await identity(observed!.executable)).catch(() => false);
    const version = language === "flutter" || runtime.locked_version === observed.version || runtime.locked_version?.startsWith(`${observed.version}+`);
    return result(equal && version ? "pass" : "fail", equal && version ? "native_debug_process_observed_without_project_secrets" : "native_debug_runtime_mismatch", observed.version);
  } finally {
    clearTimeout(timeout); clearInterval(observationTimer); tracker.dispose(); start.dispose(); termination.dispose(); cancellation.dispose();
    if (session) await vscode.debug.stopDebugging(session);
    await vscode.workspace.fs.delete(directory, { recursive: true, useTrash: false });
  }
}

export function appendDebugEvidence(report: EnvironmentDescriptor, evidence: DebugEvidence): void {
  report.evidence = [...report.evidence.filter(old => old.tool !== evidence.tool || old.entry !== evidence.entry), evidence];
  report.execution_verified = report.evidence.length > 0 && report.evidence.every(item => item.state === "pass");
}
