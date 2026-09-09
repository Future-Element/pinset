export const siteUrl = (process.env.NEXT_PUBLIC_SITE_URL || "https://pinset.future-element.com").replace(/\/$/, "");

export const siteConfig = {
  name: "Pinset",
  alternateName: "Pinset Runtime Manager",
  repository: "https://github.com/Future-Element/pinset",
  organization: {
    name: "Future Element",
    url: "https://future-element.com",
    sameAs: "https://github.com/Future-Element",
  },
  titleZh: "Pinset — 多语言运行时版本管理器",
  titleEn: "Pinset — Polyglot Runtime Version Manager",
  descriptionZh: "Pinset 是免费开源的多语言运行时版本管理器。一份配置与精确锁文件管理 Node.js、Python、Rust、Go、Bun 等工具，统一团队与 CI 的开发环境，支持 Windows、macOS 和 Linux。",
  descriptionEn: "Pinset is a free, open-source runtime version manager. Lock Node.js, Python, Rust, Go, Bun, and more in one project. Reproducible toolchains for your team and CI.",
  contentUpdatedAt: "2026-09-07T00:00:00.000Z",
  homepageUpdatedAt: "2026-09-08T00:00:00.000Z",
};

export type Locale = "zh-CN" | "en";

export function localePrefix(locale: Locale) {
  return locale === "en" ? "/en" : "";
}

export function languageAlternates(zhPath: string, enPath: string) {
  return { "zh-CN": zhPath, en: enPath, "x-default": enPath };
}
