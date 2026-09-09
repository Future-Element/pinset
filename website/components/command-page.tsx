import Link from "next/link";
import { commandNavigation, type CommandDoc, type CommandGroup } from "@/lib/commands";
import type { Locale } from "@/lib/site";
import { localePrefix, siteConfig, siteUrl } from "@/lib/site";
import { DocsShell } from "./docs-shell";
import { LatestReleaseVersion } from "./latest-release";
import { Markdown } from "./markdown";

export function CommandPage({ locale, groups, command }: { locale: Locale; groups: CommandGroup[]; command: CommandDoc }) {
  const zh = locale === "zh-CN";
  const prefix = localePrefix(locale);
  const commands = groups.flatMap((group) => group.commands);
  const index = commands.findIndex((item) => item.slug === command.slug);
  const previous = commands[index - 1];
  const next = commands[index + 1];
  const path = `${prefix}/docs/commands/${command.slug}`;
  const jsonLd = {
    "@context": "https://schema.org",
    "@type": "TechArticle",
    "@id": `${siteUrl}${path}#article`,
    url: `${siteUrl}${path}`,
    headline: `pinset ${command.title}`,
    description: command.description,
    inLanguage: locale,
    isPartOf: { "@id": `${siteUrl}/#website` },
    about: { "@id": `${siteUrl}/#software` },
    author: { "@type": "Organization", name: siteConfig.organization.name, url: siteConfig.organization.url },
    dateModified: siteConfig.contentUpdatedAt,
  };
  const breadcrumb = {
    "@context": "https://schema.org", "@type": "BreadcrumbList",
    itemListElement: [
      { "@type": "ListItem", position: 1, name: "Pinset", item: `${siteUrl}${prefix || "/"}` },
      { "@type": "ListItem", position: 2, name: zh ? "命令参考" : "Command reference", item: `${siteUrl}${prefix}/docs/commands` },
      { "@type": "ListItem", position: 3, name: `pinset ${command.title}`, item: `${siteUrl}${path}` },
    ],
  };

  return (
    <DocsShell groups={commandNavigation(groups)} locale={locale}>
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd).replace(/</g, "\\u003c") }} />
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(breadcrumb).replace(/</g, "\\u003c") }} />
      <article className="docPage commandPage">
        <header className="docHeader compactHeader">
          <div className="breadcrumbs"><Link href={prefix || "/"}>{zh ? "文档" : "Documentation"}</Link><span>/</span><Link href={`${prefix}/docs/commands`}>{zh ? "命令" : "Commands"}</Link><span>/</span>{command.title}</div>
          <div className="commandGroupLabel">{command.group}</div>
          <h1><code>pinset {command.title}</code></h1>
          <p>{command.description}</p>
          <a className="editLink" href={`${siteConfig.repository}/blob/main/docs/${locale === "en" ? "commands.md" : "commands.zh-CN.md"}`} target="_blank" rel="noreferrer">{zh ? "在 GitHub 查看源文档" : "View source on GitHub"} ↗</a>
        </header>
        <Markdown source={command.markdown} />
        <nav className="pager" aria-label={zh ? "命令分页" : "Command pagination"}>
          {previous ? <Link className="previous" href={`${prefix}/docs/commands/${previous.slug}`}><small>{zh ? "上一个" : "Previous"}</small><code>← pinset {previous.title}</code></Link> : <span />}
          {next ? <Link className="next" href={`${prefix}/docs/commands/${next.slug}`}><small>{zh ? "下一个" : "Next"}</small><code>pinset {next.title} →</code></Link> : <span />}
        </nav>
        <div className="pageMeta"><span>{path}</span><span>Pinset <LatestReleaseVersion fallback={zh ? "最新版本" : "latest"} /></span></div>
      </article>
    </DocsShell>
  );
}
