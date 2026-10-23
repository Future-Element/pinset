import fs from "node:fs";
import path from "node:path";
import type { Locale } from "./site";

export const guideSlugs = ["concepts", "generic-tools", "ci", "agents", "migrate-from-volta"] as const;
export type GuideSlug = typeof guideSlugs[number];
export type GuideDoc = { slug: GuideSlug; title: string; description: string; markdown: string; filename: string };
export type GuideNavigation = Pick<GuideDoc, "slug" | "title" | "description">;

export function getGuide(locale: Locale, slug: string): GuideDoc | undefined {
  if (!guideSlugs.includes(slug as GuideSlug)) return undefined;
  const filename = `${slug}${locale === "en" ? "" : ".zh-CN"}.md`;
  const source = fs.readFileSync(path.resolve(process.cwd(), "..", "docs", filename), "utf8").replace(/\r\n/g, "\n").trim();
  const [heading, ...paragraphs] = source.split(/\n\s*\n/);
  const navigationRow = /^(?:\[[^\]]+\]\([^)]+\)(?:\s*[·|]\s*)?)+$/;
  const purposeIndex = navigationRow.test(paragraphs[0] || "") ? 1 : 0;
  const purpose = paragraphs[purposeIndex];
  const title = heading.match(/^#\s+([^\n]+)$/)?.[1];
  if (!title || !purpose || /^(?:#|```|\||[-*>]\s)/.test(purpose)) throw new Error(`${filename}: start the guide with a title and purpose paragraph`);
  return {
    slug: slug as GuideSlug,
    title: title.trim(),
    description: purpose.replace(/\s+/g, " ").replace(/[`*_~]/g, "").replace(/\[(.*?)\]\(.*?\)/g, "$1").trim(),
    markdown: paragraphs.filter((_, index) => index !== purposeIndex).join("\n\n").trim(),
    filename,
  };
}

export function getGuides(locale: Locale) {
  return guideSlugs.map((slug) => getGuide(locale, slug)!);
}

export function guideNavigation(locale: Locale): GuideNavigation[] {
  return getGuides(locale).map(({ slug, title, description }) => ({ slug, title, description }));
}
