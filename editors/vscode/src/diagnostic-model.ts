export type DiagnosticDocument = "config" | "lock";

export function diagnosticDocument(category: string): DiagnosticDocument | undefined {
  if (category === "configuration") return "config";
  if (category === "lock" || category === "platform_artifact" || category === "provenance") {
    return "lock";
  }
  return undefined;
}
