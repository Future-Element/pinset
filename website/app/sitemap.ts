import type { MetadataRoute } from "next";
import { getCommandDocs } from "@/lib/commands";
import { languageAlternates, siteConfig, siteUrl } from "@/lib/site";

export const dynamic = "force-static";

export default function sitemap(): MetadataRoute.Sitemap {
  const updated = new Date(siteConfig.contentUpdatedAt);
  const roots: MetadataRoute.Sitemap = [
    { url: siteUrl, lastModified: updated, changeFrequency: "weekly", priority: 1, alternates: { languages: languageAlternates(siteUrl, `${siteUrl}/en`) } },
    { url: `${siteUrl}/en`, lastModified: updated, changeFrequency: "weekly", priority: 0.9, alternates: { languages: languageAlternates(siteUrl, `${siteUrl}/en`) } },
    { url: `${siteUrl}/docs/commands`, lastModified: updated, changeFrequency: "weekly", priority: 0.9, alternates: { languages: languageAlternates(`${siteUrl}/docs/commands`, `${siteUrl}/en/docs/commands`) } },
    { url: `${siteUrl}/en/docs/commands`, lastModified: updated, changeFrequency: "weekly", priority: 0.8, alternates: { languages: languageAlternates(`${siteUrl}/docs/commands`, `${siteUrl}/en/docs/commands`) } },
  ];

  const commands = getCommandDocs("zh-CN").flatMap(({ slug }) => [
    { url: `${siteUrl}/docs/commands/${slug}`, lastModified: updated, changeFrequency: "weekly" as const, priority: 0.75, alternates: { languages: languageAlternates(`${siteUrl}/docs/commands/${slug}`, `${siteUrl}/en/docs/commands/${slug}`) } },
    { url: `${siteUrl}/en/docs/commands/${slug}`, lastModified: updated, changeFrequency: "weekly" as const, priority: 0.7, alternates: { languages: languageAlternates(`${siteUrl}/docs/commands/${slug}`, `${siteUrl}/en/docs/commands/${slug}`) } },
  ]);

  return [...roots, ...commands];
}
