export const SUPPORTED_PROTOCOL_SCHEMA = 2;

export type Readiness = "pass" | "fail" | "unknown" | "not_applicable";
export interface EnvironmentCheck { id: string; state: Readiness; reason: string; next_step?: string | null }
export interface RuntimeDescriptor {
  tool: string; requested: string; locked_version?: string | null; installation_identity?: string | null;
  selection_source: string; target: string; commands: string[]; executable?: string | null; checks: EnvironmentCheck[];
}
export interface EnvironmentDescriptor {
  schema: number; cli_version: string; project_id?: string | null; project_root?: string | null;
  target: string; host: string; profile?: string | null; profile_source: string; context_fingerprint?: string | null;
  runtimes: RuntimeDescriptor[]; checks: EnvironmentCheck[];
  evidence: { entry: string; tool: string; state: Readiness; reason: string; observed_version?: string | null;
    expected_executable?: string | null; observed_executable?: string | null; observed_unix_ms?: number; context_fingerprint?: string | null }[];
  environment_ready: boolean; execution_verified: boolean;
}

export interface PinsetTaskContext {
  name: string;
  description?: string;
  depends_on: string[];
  profile?: string;
  cwd?: string;
  python_environment?: string;
}

export interface PinsetFinding {
  code: string;
  severity: "error" | "warning" | "info" | string;
  category: string;
  subject: string;
}

export interface PinsetDiagnosticTool {
  name: string;
  requested: string;
  locked_version?: string | null;
  provider?: string | null;
  current_target_artifact: boolean;
}

export interface PinsetContext {
  descriptor?: EnvironmentDescriptor;
  protocol_schema: number;
  minimum_extension_version: string;
  cli_version: string;
  requires_workspace_trust: boolean;
  folder: string;
  project_root?: string | null;
  config?: string | null;
  workspace_members: string[];
  environment: {
    profiles: string[];
    selected?: string;
    source: string;
  };
  tasks: PinsetTaskContext[];
  diagnostics: {
    project: {
      configured: boolean;
      config_schema?: number | null;
      lock_schema?: number | null;
      tasks: number;
    };
    tools: PinsetDiagnosticTool[];
    summary: { passed: boolean; errors: number; warnings: number; info: number };
    findings: PinsetFinding[];
  };
}

interface Envelope {
  schema: number;
  command: string;
  ok: boolean;
  data?: PinsetContext;
  error?: { code?: string; message?: string };
}

export function parseContext(output: string, extensionVersion: string): PinsetContext {
  let envelope: Envelope;
  try {
    envelope = JSON.parse(output) as Envelope;
  } catch (error) {
    throw new Error(`Pinset returned invalid JSON: ${String(error)}`);
  }
  if (envelope.schema !== 1 || envelope.command !== "editor.context") {
    throw new Error(
      `Unsupported Pinset editor envelope: schema=${String(envelope.schema)} command=${String(envelope.command)}`,
    );
  }
  if (!envelope.ok || !envelope.data) {
    throw new Error(envelope.error?.message ?? "Pinset could not create editor context");
  }
  if (![1, SUPPORTED_PROTOCOL_SCHEMA].includes(envelope.data.protocol_schema)) {
    throw new Error(
      `Pinset editor protocol ${envelope.data.protocol_schema} is not supported by this extension (supports ${SUPPORTED_PROTOCOL_SCHEMA})`,
    );
  }
  if (compareVersions(extensionVersion, envelope.data.minimum_extension_version) < 0) {
    throw new Error(
      `Pinset requires extension ${envelope.data.minimum_extension_version} or newer; installed ${extensionVersion}`,
    );
  }
  validateContext(envelope.data);
  if (envelope.data.protocol_schema === 2) validateDescriptor(envelope.data.descriptor);
  return envelope.data;
}

export function validateDescriptor(value: unknown): asserts value is EnvironmentDescriptor {
  const descriptor = value as EnvironmentDescriptor | undefined;
  const states = ["pass", "fail", "unknown", "not_applicable"];
  if (!descriptor || descriptor.schema !== 2 || typeof descriptor.target !== "string" || typeof descriptor.host !== "string"
    || typeof descriptor.environment_ready !== "boolean" || typeof descriptor.execution_verified !== "boolean"
    || !Array.isArray(descriptor.runtimes) || !Array.isArray(descriptor.checks) || !Array.isArray(descriptor.evidence)) {
    throw new Error("Pinset returned an invalid environment descriptor");
  }
  for (const runtime of descriptor.runtimes) {
    if (typeof runtime.tool !== "string" || typeof runtime.requested !== "string" || typeof runtime.selection_source !== "string"
      || !Array.isArray(runtime.commands) || !Array.isArray(runtime.checks)
      || (runtime.executable != null && typeof runtime.executable !== "string")) throw new Error("Invalid runtime descriptor");
  }
  for (const check of [...descriptor.checks, ...descriptor.runtimes.flatMap(runtime => runtime.checks)]) {
    if (!states.includes(check.state) || typeof check.reason !== "string" || typeof check.id !== "string") throw new Error("Invalid environment check");
  }
  for (const evidence of descriptor.evidence) {
    if (!states.includes(evidence.state) || typeof evidence.entry !== "string" || typeof evidence.tool !== "string") throw new Error("Invalid execution evidence");
  }
}

function validateContext(context: PinsetContext): void {
  if (
    typeof context.cli_version !== "string" ||
    typeof context.folder !== "string" ||
    context.requires_workspace_trust !== true ||
    !Array.isArray(context.workspace_members) ||
    !Array.isArray(context.environment?.profiles) ||
    typeof context.environment?.source !== "string" ||
    !Array.isArray(context.tasks) ||
    typeof context.diagnostics?.project?.configured !== "boolean" ||
    !Array.isArray(context.diagnostics?.tools) ||
    !Array.isArray(context.diagnostics?.findings) ||
    typeof context.diagnostics?.summary?.passed !== "boolean"
  ) {
    throw new Error("Pinset returned an invalid editor context payload");
  }
  for (const task of context.tasks) {
    if (typeof task?.name !== "string" || !Array.isArray(task.depends_on)) {
      throw new Error("Pinset returned an invalid editor task payload");
    }
  }
  for (const tool of context.diagnostics.tools) {
    if (
      typeof tool?.name !== "string" ||
      typeof tool.requested !== "string" ||
      typeof tool.current_target_artifact !== "boolean"
    ) {
      throw new Error("Pinset returned an invalid editor tool payload");
    }
  }
}

function compareVersions(left: string, right: string): number {
  const parse = (value: string): number[] => value.split(".").map((part) => Number.parseInt(part, 10) || 0);
  const a = parse(left);
  const b = parse(right);
  for (let index = 0; index < Math.max(a.length, b.length); index += 1) {
    const difference = (a[index] ?? 0) - (b[index] ?? 0);
    if (difference !== 0) return difference;
  }
  return 0;
}
