import Link from "next/link";
import type { CommandGroup } from "@/lib/commands";
import type { Locale } from "@/lib/site";
import { localePrefix } from "@/lib/site";
import { DocsShell } from "./docs-shell";

export function CommandIndexPage({ locale, groups }: { locale: Locale; groups: CommandGroup[] }) {
  const zh = locale === "zh-CN";
  const prefix = localePrefix(locale);
  const count = groups.reduce((total, group) => total + group.commands.length, 0);

  return (
    <DocsShell groups={groups} locale={locale}>
      <article className="docPage commandIndexPage">
        <header className="docHeader compactHeader">
          <div className="breadcrumbs"><Link href={prefix || "/"}>{zh ? "文档" : "Documentation"}</Link><span>/</span>{zh ? "命令" : "Commands"}</div>
          <h1>{zh ? "命令参考" : "Command reference"}</h1>
          <p>{zh ? `Pinset 2.1 的 ${count} 个公开 CLI 命令。每页包含语法、状态修改、JSON 支持、退出码和关键错误。` : `${count} public CLI commands in Pinset 2.1. Each page covers syntax, state changes, JSON support, exit codes, and key errors.`}</p>
        </header>
        <div className="commandDirectory">
          {groups.map((group) => (
            <section key={group.title} id={group.commands[0]?.slug}>
              <div className="directoryHeading"><h2>{group.title}</h2><span>{group.commands.length}</span></div>
              <div className="directoryList">
                {group.commands.map((command) => <Link href={`${prefix}/docs/commands/${command.slug}`} key={command.slug}><code>pinset {command.title}</code><span>{command.description}</span><b>→</b></Link>)}
              </div>
            </section>
          ))}
        </div>
        <footer className="pageFooter"><span>Pinset 2.1 CLI</span><Link href={prefix || "/"}>{zh ? "返回介绍" : "Back to introduction"} →</Link></footer>
      </article>
    </DocsShell>
  );
}
