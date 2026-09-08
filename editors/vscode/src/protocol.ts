export const SUPPORTED_PROTOCOL_SCHEMA = 1;

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

export interface PinsetContext {
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
  if (envelope.data.protocol_schema !== SUPPORTED_PROTOCOL_SCHEMA) {
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
  return envelope.data;
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
