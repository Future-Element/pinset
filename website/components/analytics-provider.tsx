"use client";

import { usePathname } from "next/navigation";
import { createContext, useCallback, useContext, useEffect, useRef } from "react";
import { captureEvent, type AnalyticsEvent } from "@/lib/analytics";
import { siteConfig, type Locale } from "@/lib/site";
import { useLatestReleaseVersion } from "./latest-release";

type Capture = (...event: AnalyticsEvent) => void;
const AnalyticsContext = createContext<Capture>(() => {});

export function AnalyticsProvider({ children, locale, landing }: { children: React.ReactNode; locale: Locale; landing: boolean }) {
  const path = usePathname();
  const latestRelease = useLatestReleaseVersion();
  const releaseShown = landing ? (latestRelease ?? siteConfig.docsVersion) : siteConfig.docsVersion;
  const viewed = useRef(false);
  const capture = useCallback<Capture>((...event) => {
    captureEvent({ locale, path, release_shown: releaseShown }, ...event);
  }, [locale, path, releaseShown]);

  useEffect(() => {
    if (!landing || viewed.current) return;
    // React Strict Mode replays effects; a release update must not log another view.
    viewed.current = true;
    capture("landing_viewed");
  }, [capture, landing]);

  return <AnalyticsContext value={capture}>{children}</AnalyticsContext>;
}

export function useAnalytics() {
  return useContext(AnalyticsContext);
}
