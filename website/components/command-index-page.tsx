import Link from "next/link";
import type { CommandGroup } from "@/lib/commands";
import type { Locale } from "@/lib/site";
import { localePrefix, siteConfig } from "@/lib/site";

export function CommandIndexPage({ locale, groups }: { locale: Locale; groups: CommandGroup[] }) {
  const zh = locale === "zh-CN";
  const prefix = localePrefix(locale);
  const count = groups.reduce((total, group) => total + group.commands.length, 0);
  const flows = zh ? [
    { title: "新项目开始使用", detail: "创建配置，再选择工具版本。", commands: ["init", "use"] },
    { title: "迁移已有版本配置", detail: "确认迁移计划，再安装导入的工具。", commands: ["import", "install"] },
    { title: "克隆已有 Pinset 项目", detail: "按锁文件还原，再检查工具链。", commands: ["install", "check"] },
  ] : [
    { title: "Start a new project", detail: "Create the configuration, then choose tool versions.", commands: ["init", "use"] },
    { title: "Migrate existing versions", detail: "Review the import plan, then install the imported tools.", commands: ["import", "install"] },
    { title: "Clone a Pinset project", detail: "Restore from the lockfile, then check the toolchain.", commands: ["install", "check"] },
  ];

  return (
    <>
      <article className="docPage commandIndexPage">
        <header className="docHeader compactHeader">
          <div className="breadcrumbs"><Link href={prefix || "/"}>{zh ? "文档" : "Documentation"}</Link><span>/</span>{zh ? "命令" : "Commands"}</div>
          <h1>{zh ? "命令参考" : "Command reference"}</h1>
          <p>{zh ? <>Pinset {siteConfig.docsVersion} 的 {count} 个命令。按要完成的事找到命令，查看示例、执行结果和参数。</> : <>{count} commands documented for Pinset {siteConfig.docsVersion}. Find the command for your task, then see examples, results, and options.</>}</p>
        </header>
        <section className="starterSection" aria-labelledby="starter-heading">
          <h2 id="starter-heading">{zh ? "从这里开始" : "Start here"}</h2>
          <div className="starterFlows">{flows.map((flow) => <article key={flow.title}>
            <h3>{flow.title}</h3><p>{flow.detail}</p>
            <div>{flow.commands.map((command, index) => <span key={command}>{index > 0 && <span className="flowArrow" aria-hidden="true">→</span>}<Link href={`${prefix}/docs/commands/${command}`}><code>pinset {command}</code></Link></span>)}</div>
          </article>)}</div>
        </section>
        <nav className="guideLinks" aria-label={zh ? "相关指南" : "Related guides"}>
          <span>{zh ? "先了解：" : "Read about:"}</span>
          <Link href={`${prefix}/docs/concepts`}>{zh ? "基础概念" : "Core concepts"}</Link>
          <Link href={`${prefix}/docs/generic-tools`}>{zh ? "自定义 CLI" : "Custom CLI tools"}</Link>
          <Link href={`${prefix}/docs/ci`}>{zh ? "CI 与 GitHub Action" : "CI and GitHub Action"}</Link>
        </nav>
        <div className="commandDirectory">
          {groups.map((group) => (
            <section key={group.title} id={group.commands[0]?.slug}>
              <div className="directoryHeading"><h2>{group.title}</h2><span>{group.commands.length}</span></div>
              <div className="directoryList">
                {group.commands.map((command) => <Link href={`${prefix}/docs/commands/${command.slug}`} key={command.slug} scroll={false}><code>pinset {command.title}</code><span>{command.description}</span><b>→</b></Link>)}
              </div>
            </section>
          ))}
        </div>
        <footer className="pageFooter"><span>Pinset {siteConfig.docsVersion} CLI</span><Link href={prefix || "/"}>{zh ? "返回首页" : "Back to home"} →</Link></footer>
      </article>
    </>
  );
}
