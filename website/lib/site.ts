import packageInfo from "../package.json";

export const siteUrl = (process.env.NEXT_PUBLIC_SITE_URL || "https://pinset.future-element.com").replace(/\/$/, "");

export const siteConfig = {
  name: "Pinset",
  docsVersion: packageInfo.version,
  alternateName: "Pinset Runtime Manager",
  repository: "https://github.com/Future-Element/pinset",
  organization: {
    name: "Future Element",
    url: "https://future-element.com",
    sameAs: "https://github.com/Future-Element",
  },
  titleZh: "Pinset — 项目工具链与执行环境",
  titleEn: "Pinset — Project Toolchains and Execution Environments",
  descriptionZh: "Pinset 锁定八种项目工具链和自定义 CLI，导入已有版本声明，通过 GitHub Action 还原并检查环境。版本、下载来源和执行入口清晰可查。",
  descriptionEn: "Lock eight built-in toolchains and custom CLI tools, import existing version files, and restore and check environments with GitHub Actions. Inspect versions, downloads, and command routing.",
  contentUpdatedAt: "2026-10-23T00:00:00.000Z",
  homepageUpdatedAt: "2026-10-23T00:00:00.000Z",
};

export type Locale = "zh-CN" | "en";

export function localePrefix(locale: Locale) {
  return locale === "en" ? "/en" : "";
}

export function languageAlternates(zhPath: string, enPath: string) {
  return { "zh-CN": zhPath, en: enPath, "x-default": enPath };
}
