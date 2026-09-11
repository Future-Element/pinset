export type PinsetProcessErrorKind = "cli" | "missing-executable";

export class PinsetProcessError extends Error {
  override readonly name = "PinsetProcessError";

  constructor(
    message: string,
    readonly stderr: string,
    readonly kind: PinsetProcessErrorKind = "cli",
  ) {
    super(message);
  }
}

export function processStartError(error: NodeJS.ErrnoException): PinsetProcessError {
  const kind = error.code === "ENOENT" ? "missing-executable" : "cli";
  return new PinsetProcessError(`Could not start Pinset: ${error.message}`, "", kind);
}

export function isMissingExecutableError(error: unknown): error is PinsetProcessError {
  return error instanceof PinsetProcessError && error.kind === "missing-executable";
}
