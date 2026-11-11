export const releaseCacheLifetime = 5 * 60 * 1000;

export function releaseVersion(value: unknown): string | null {
  if (!value || typeof value !== "object" || !("version" in value)) return null;
  const version = (value as { version?: unknown }).version;
  return typeof version === "string" && /^3\.\d+\.\d+$/.test(version) ? version : null;
}

export function cachedRelease(value: unknown, now: number): { version: string; fetchedAt: number } | null {
  const version = releaseVersion(value);
  if (!version || !value || typeof value !== "object" || !("fetchedAt" in value)) return null;
  const fetchedAt = (value as { fetchedAt?: unknown }).fetchedAt;
  if (typeof fetchedAt !== "number" || !Number.isFinite(fetchedAt) || fetchedAt > now || now - fetchedAt >= releaseCacheLifetime) return null;
  return { version, fetchedAt };
}

export function installerCommand(method: "unix" | "windows", release: string | null): string {
  const version = releaseVersion({ version: release });
  if (method === "unix") {
    return "curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/install.sh | sh" + (version ? ` -s -- --version ${version}` : "");
  }
  return "Invoke-WebRequest https://raw.githubusercontent.com/Future-Element/pinset/main/install.ps1 -OutFile install.ps1\n.\\install.ps1" + (version ? ` -Version ${version}` : "");
}
