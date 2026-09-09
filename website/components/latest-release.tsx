"use client";

import { createContext, useContext, useEffect, useState } from "react";

const latestReleaseApi = "/api/latest-release";
const cacheKey = "pinset.latest-release.v1";
const cacheLifetime = 5 * 60 * 1000;

type CachedRelease = {
  version: string;
  fetchedAt: number;
};

const LatestReleaseContext = createContext<string | null>(null);

function releaseVersion(value: unknown) {
  if (!value || typeof value !== "object" || !("version" in value)) return null;
  const version = (value as { version?: unknown }).version;
  return typeof version === "string" && /^\d+\.\d+\.\d+$/.test(version) ? version : null;
}

function readCachedRelease() {
  try {
    const cached = JSON.parse(sessionStorage.getItem(cacheKey) || "null") as CachedRelease | null;
    if (!cached || Date.now() - cached.fetchedAt >= cacheLifetime) return null;
    return /^\d+\.\d+\.\d+$/.test(cached.version) ? cached.version : null;
  } catch {
    return null;
  }
}

export function LatestReleaseProvider({ children }: { children: React.ReactNode }) {
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    const cached = readCachedRelease();
    if (cached) {
      setVersion(cached);
      return;
    }

    const controller = new AbortController();
    fetch(latestReleaseApi, { signal: controller.signal })
      .then((response) => {
        if (!response.ok) throw new Error(`Latest release request failed with ${response.status}`);
        return response.json();
      })
      .then((release) => {
        const latest = releaseVersion(release);
        if (!latest) return;
        setVersion(latest);
        try {
          sessionStorage.setItem(cacheKey, JSON.stringify({ version: latest, fetchedAt: Date.now() }));
        } catch {
          // Storage can be disabled without preventing the live version from rendering.
        }
      })
      .catch(() => {
        // Keep the version-neutral fallback when GitHub is unavailable or rate limited.
      });

    return () => controller.abort();
  }, []);

  return <LatestReleaseContext value={version}>{children}</LatestReleaseContext>;
}

export function useLatestReleaseVersion() {
  return useContext(LatestReleaseContext);
}

export function LatestReleaseVersion({ fallback, prefix = "" }: { fallback: string; prefix?: string }) {
  const version = useLatestReleaseVersion();
  return <span data-latest-release-version>{version ? `${prefix}${version}` : fallback}</span>;
}
