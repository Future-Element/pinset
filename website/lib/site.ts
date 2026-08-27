import packageJson from "../package.json";

export const siteUrl = (process.env.NEXT_PUBLIC_SITE_URL || "https://pinset.future-element.com").replace(/\/$/, "");

export const siteConfig = {
  name: "Pinset",
  version: packageJson.version,
  repository: "https://github.com/Future-Element/pinset",
  organization: {
    name: "Future Element",
    url: "https://future-element.com",
    sameAs: "https://github.com/Future-Element",
  },
  titleZh: "Pinset — 多语言运行时版本管理器",
  titleEn: "Pinset — Polyglot Runtime Version Manager",
  descriptionZh: "Pinset 是面向多语言项目的运行时版本管理器，用一份配置与锁文件管理 Node.js、Python、Rust、Go、Java、.NET、Flutter 等工具链。",
  descriptionEn: "Pinset is a runtime version manager for polyglot projects, using one configuration and lockfile for Node.js, Python, Rust, Go, Java, .NET, Flutter, and more.",
  contentUpdatedAt: "2026-08-25T00:00:00.000Z",
};

export type Locale = "zh-CN" | "en";

export function localePrefix(locale: Locale) {
  return locale === "en" ? "/en" : "";
}

export function languageAlternates(zhPath: string, enPath: string) {
  return { "zh-CN": zhPath, en: enPath, "x-default": enPath };
}
