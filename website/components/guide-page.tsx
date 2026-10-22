import type { Metadata } from "next";
import Link from "next/link";
import { commandNavigation, getCommandGroups } from "@/lib/commands";
import { getGuide, guideNavigation, type GuideDoc } from "@/lib/guides";
import { openGraphImage, twitterImage } from "@/lib/metadata";
import { languageAlternates, localePrefix, siteConfig, siteUrl, type Locale } from "@/lib/site";
import { DocsShell } from "./docs-shell";
import { Markdown } from "./markdown";

export function guideMetadata(locale: Locale, slug: string): Metadata {
  const guide = getGuide(locale, slug);
  if (!guide) return {};
  const canonical = `${localePrefix(locale)}/docs/${slug}`;
  return {
    title: guide.title,
    description: guide.description,
    alternates: { canonical, languages: languageAlternates(`/docs/${slug}`, `/en/docs/${slug}`) },
    openGraph: { type: "article", siteName: siteConfig.name, title: guide.title, description: guide.description, url: canonical, images: [openGraphImage] },
    twitter: { card: "summary_large_image", title: guide.title, description: guide.description, images: [twitterImage] },
  };
}

export function GuidePage({ locale, guide }: { locale: Locale; guide: GuideDoc }) {
  const zh = locale === "zh-CN";
  const prefix = localePrefix(locale);
  const path = `${prefix}/docs/${guide.slug}`;
  const jsonLd = {
    "@context": "https://schema.org", "@type": "TechArticle", "@id": `${siteUrl}${path}#article`, url: `${siteUrl}${path}`,
    headline: guide.title, description: guide.description, inLanguage: locale,
    isPartOf: { "@id": `${siteUrl}/#website` }, about: { "@id": `${siteUrl}/#software` },
    author: { "@type": "Organization", name: siteConfig.organization.name, url: siteConfig.organization.url }, dateModified: siteConfig.contentUpdatedAt,
  };
  const breadcrumb = {
    "@context": "https://schema.org", "@type": "BreadcrumbList",
    itemListElement: [
      { "@type": "ListItem", position: 1, name: "Pinset", item: `${siteUrl}${prefix || "/"}` },
      { "@type": "ListItem", position: 2, name: zh ? "命令参考" : "Command reference", item: `${siteUrl}${prefix}/docs/commands` },
      { "@type": "ListItem", position: 3, name: guide.title, item: `${siteUrl}${path}` },
    ],
  };
  return (
    <DocsShell groups={commandNavigation(getCommandGroups(locale))} guides={guideNavigation(locale)} locale={locale}>
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd).replace(/</g, "\\u003c") }} />
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(breadcrumb).replace(/</g, "\\u003c") }} />
      <article className="docPage guidePage">
        <header className="docHeader compactHeader">
          <div className="breadcrumbs"><Link href={`${prefix}/docs/commands`}>{zh ? "文档" : "Documentation"}</Link><span>/</span>{guide.title}</div>
          <h1>{guide.title}</h1>
          <p>{guide.description}</p>
          <a className="editLink" href={`${siteConfig.repository}/blob/main/docs/${guide.filename}`} target="_blank" rel="noreferrer">{zh ? "在 GitHub 查看源文档" : "View source on GitHub"} ↗</a>
        </header>
        <Markdown source={guide.markdown} />
        <footer className="pageFooter"><span>{zh ? "适用版本" : "Documented version"} · Pinset {siteConfig.docsVersion}</span><Link href={`${prefix}/docs/commands`}>{zh ? "查看全部命令" : "Browse commands"} →</Link></footer>
      </article>
    </DocsShell>
  );
}
