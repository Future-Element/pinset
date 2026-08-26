import Link from "next/link";
import type { CommandDoc, CommandGroup } from "@/lib/commands";
import type { Locale } from "@/lib/site";
import { localePrefix, siteConfig } from "@/lib/site";
import { DocsShell } from "./docs-shell";
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
    headline: `pinset ${command.title}`,
    description: command.description,
    inLanguage: locale,
    isPartOf: { "@type": "WebSite", name: siteConfig.name },
    about: { "@type": "SoftwareApplication", name: siteConfig.name, softwareVersion: "2.1.1" },
  };

  return (
    <DocsShell groups={groups} locale={locale}>
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd).replace(/</g, "\\u003c") }} />
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
        <div className="pageMeta"><span>{path}</span><span>Pinset 2.1</span></div>
      </article>
    </DocsShell>
  );
}
