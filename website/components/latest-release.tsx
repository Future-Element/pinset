"use client";

import { createContext, useContext, useEffect, useState } from "react";
import { cachedRelease, releaseCacheLifetime, releaseVersion } from "@/lib/latest-release";

const latestReleaseApi = "/api/latest-release";
const cacheKey = "pinset.latest-release.v3";

const LatestReleaseContext = createContext<string | null>(null);

function readCachedRelease() {
  try {
    return cachedRelease(JSON.parse(sessionStorage.getItem(cacheKey) || "null"), Date.now());
  } catch {
    return null;
  }
}

export function LatestReleaseProvider({ children }: { children: React.ReactNode }) {
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    let pending = false;
    let freshUntil = 0;
    let timer: number | undefined;
    function scheduleRefresh(delay: number) {
      window.clearTimeout(timer);
      timer = window.setTimeout(refresh, Math.max(1, delay));
    }
    function refresh() {
      if (pending || controller.signal.aborted) return;
      const cached = readCachedRelease();
      if (cached) {
        setVersion(cached.version);
        freshUntil = cached.fetchedAt + releaseCacheLifetime;
        scheduleRefresh(freshUntil - Date.now());
        return;
      }
      if (Date.now() < freshUntil) {
        scheduleRefresh(freshUntil - Date.now());
        return;
      }
      setVersion(null);
      pending = true;
      fetch(latestReleaseApi, { signal: controller.signal })
        .then((response) => {
          if (!response.ok) throw new Error(`Latest release request failed with ${response.status}`);
          return response.json();
        })
        .then((release) => {
          const latest = releaseVersion(release);
          if (!latest || controller.signal.aborted) return;
          const fetchedAt = Date.now();
          freshUntil = fetchedAt + releaseCacheLifetime;
          setVersion(latest);
          try {
            sessionStorage.setItem(cacheKey, JSON.stringify({ version: latest, fetchedAt }));
          } catch {
            // Storage can be disabled without preventing the live version from rendering.
          }
        })
        .catch(() => {
          // Keep the version-neutral fallback when GitHub is unavailable or rate limited.
        })
        .finally(() => {
          pending = false;
          if (!controller.signal.aborted) scheduleRefresh(freshUntil > Date.now() ? freshUntil - Date.now() : releaseCacheLifetime);
        });
    }
    refresh();
    window.addEventListener("focus", refresh);

    return () => {
      controller.abort();
      window.clearTimeout(timer);
      window.removeEventListener("focus", refresh);
    };
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
