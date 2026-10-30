"use client";

import posthog from "posthog-js";
import type { Locale } from "./site";

export type InstallMethod = "unix" | "windows" | "homebrew" | "scoop";

export type AnalyticsEvent =
  | [event: "landing_viewed" | "shell_setup_clicked" | "ci_action_clicked" | "cta_install_clicked" | "release_link_clicked"]
  | [event: "install_method_selected" | "install_command_copied" | "install_source_clicked", properties: { method: InstallMethod }]
  | [event: "docs_nav_clicked", properties: { target: string }]
  | [event: "faq_toggled", properties: { question_id: string }];

export type AnalyticsProperties = {
  locale: Locale;
  path: string;
  release_shown: string | null;
};

let initialized = false;

export function captureEvent(context: AnalyticsProperties, ...[event, properties]: AnalyticsEvent) {
  const key = process.env.NEXT_PUBLIC_POSTHOG_KEY?.trim();
  if (typeof window === "undefined" || !key) return;

  try {
    if (!initialized) {
      posthog.init(key, {
        api_host: process.env.NEXT_PUBLIC_POSTHOG_HOST?.trim() || "https://us.i.posthog.com",
        autocapture: false,
        capture_pageview: false,
        capture_pageleave: false,
        capture_dead_clicks: false,
        capture_exceptions: false,
        capture_heatmaps: false,
        capture_performance: false,
        rageclick: false,
        disable_session_recording: true,
        disable_surveys: true,
        advanced_disable_flags: true,
        cross_subdomain_cookie: false,
      });
      initialized = true;
    }

    // Include current page context explicitly so navigation and release updates
    // cannot reuse properties persisted by a previous page.
    posthog.capture(event, { ...properties, ...context });
  } catch {
    // Analytics must not interfere with installation, copying, or navigation.
  }
}
