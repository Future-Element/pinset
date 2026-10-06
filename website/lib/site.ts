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
  titleZh: "Pinset — 项目工具链与升级验证",
  titleEn: "Pinset — Project Toolchains and Verified Upgrades",
  descriptionZh: "Pinset 锁定八种项目工具链，包括完整 OpenJDK、Python venv、Rust 和 Flutter，解释执行入口并在独立副本中验证升级。",
  descriptionEn: "Lock eight project toolchains, including complete OpenJDK, Python venv, Rust and Flutter. Explain execution paths and validate upgrades in isolated project copies.",
  contentUpdatedAt: "2026-10-05T00:00:00.000Z",
  homepageUpdatedAt: "2026-10-05T00:00:00.000Z",
};

export type Locale = "zh-CN" | "en";

export function localePrefix(locale: Locale) {
  return locale === "en" ? "/en" : "";
}

export function languageAlternates(zhPath: string, enPath: string) {
  return { "zh-CN": zhPath, en: enPath, "x-default": enPath };
}
