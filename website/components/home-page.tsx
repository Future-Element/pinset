import Link from "next/link";
import type { CommandGroup } from "@/lib/commands";
import type { Locale } from "@/lib/site";
import { localePrefix, siteConfig, siteUrl } from "@/lib/site";
import { CodeBlock } from "./code-block";
import { DocsShell } from "./docs-shell";

const providers = [
  ["Node.js", "node · npm · npx"], ["pnpm", "pnpm"], ["Bun", "bun · bunx"],
  ["Go", "go · gofmt"], ["Python", "python · pip"], ["Java", "Temurin JDK"],
  ["Rust", "cargo · rustc"], [".NET", "dotnet"], ["Flutter", "flutter · dart"],
];

const quickstart = [
  "pinset init",
  "pinset use node@24 pnpm@11 python@3.14 --no-install",
  "pinset install --locked",
  "node --version",
].join("\n");

export function HomePage({ locale, groups }: { locale: Locale; groups: CommandGroup[] }) {
  const zh = locale === "zh-CN";
  const prefix = localePrefix(locale);
  const commandCount = groups.reduce((count, group) => count + group.commands.length, 0);
  const organizationId = `${siteConfig.organization.url}/#organization`;
  const websiteId = `${siteUrl}/#website`;
  const softwareId = `${siteUrl}/#software`;
  const organization = {
    "@type": "Organization",
    "@id": organizationId,
    name: siteConfig.organization.name,
    url: siteConfig.organization.url,
    sameAs: [siteConfig.organization.sameAs],
  };
  const software = {
    "@type": "SoftwareApplication",
    "@id": softwareId,
    name: siteConfig.name,
    alternateName: siteConfig.alternateName,
    url: siteUrl,
    applicationCategory: "DeveloperApplication",
    operatingSystem: "Windows, Linux, macOS",
    softwareVersion: siteConfig.version,
    license: "https://opensource.org/license/mit",
    codeRepository: siteConfig.repository,
    description: zh ? siteConfig.descriptionZh : siteConfig.descriptionEn,
    author: { "@id": organizationId },
    publisher: { "@id": organizationId },
  };
  const website = {
    "@type": "WebSite",
    "@id": websiteId,
    name: siteConfig.name,
    alternateName: siteConfig.alternateName,
    url: `${siteUrl}/`,
    inLanguage: ["zh-CN", "en"],
    publisher: { "@id": organizationId },
  };
  const jsonLd = {
    "@context": "https://schema.org",
    "@graph": zh
      ? [
          website,
          organization,
          software,
        ]
      : [
          organization,
          software,
          {
            "@type": "WebPage",
            "@id": `${siteUrl}/en#webpage`,
            name: siteConfig.titleEn,
            url: `${siteUrl}/en`,
            inLanguage: "en",
            isPartOf: { "@id": websiteId },
            about: { "@id": softwareId },
          },
        ],
  };

  return (
    <DocsShell groups={groups} locale={locale}>
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd).replace(/</g, "\\u003c") }} />
      <article className="docPage homePage">
        <header className="docHeader">
          <div className="versionPill"><span /> Pinset 2.1</div>
          <h1>
            <span>Pinset</span>
            <small>{zh ? "多语言运行时版本管理器" : "Polyglot Runtime Version Manager"}</small>
          </h1>
          <p>{zh ? "一个行为可预测、理解项目边界的多语言运行时版本管理器。" : "A predictable polyglot runtime version manager that understands project boundaries."}</p>
          <div className="headerLinks">
            <Link className="primaryLink" href={`${prefix}/docs/commands`}>{zh ? `浏览全部 ${commandCount} 个命令` : `Browse all ${commandCount} commands`} <span>→</span></Link>
            <a href={siteConfig.repository} target="_blank" rel="noreferrer">GitHub</a>
          </div>
        </header>

        <section className="contentSection" id="getting-started">
          <div className="sectionLabel">{zh ? "开始使用" : "Getting started"}</div>
          <h2>{zh ? "用一份锁文件固定整个项目。" : "Pin the entire project with one lockfile."}</h2>
          <p>{zh ? "完成一次 Shell 初始化后，项目里的 node、python、cargo、flutter 等命令会直接路由到锁定版本。" : "After one-time shell activation, node, python, cargo, flutter, and other commands route directly to their locked versions."}</p>
          <CodeBlock label={zh ? "终端" : "Terminal"}>{quickstart}</CodeBlock>
        </section>

        <section className="contentSection" id="model">
          <div className="sectionLabel">{zh ? "项目模型" : "Project model"}</div>
          <h2>{zh ? "意图与精确结果，分别记录。" : "Intent and exact results, recorded separately."}</h2>
          <div className="fileModel">
            <div><code>pinset.toml</code><strong>{zh ? "用户意图与项目策略" : "User intent and project policy"}</strong><span>node = &quot;24&quot; · boundary = &quot;git&quot;</span></div>
            <i>→</i>
            <div><code>pinset.lock</code><strong>{zh ? "精确版本与制品信息" : "Exact versions and artifacts"}</strong><span>version · URL · checksum · platform</span></div>
            <i>→</i>
            <div><code>shim</code><strong>{zh ? "直接命令路由" : "Direct command routing"}</strong><span>node · python · cargo · flutter</span></div>
          </div>
        </section>

        <section className="contentSection" id="providers">
          <div className="sectionLabel">Provider</div>
          <h2>{zh ? "九类工具，同一种工作方式。" : "Nine tool families. One workflow."}</h2>
          <div className="providerList">
            {providers.map(([name, commands]) => <div key={name}><span className="providerDot" /><strong>{name}</strong><code>{commands}</code></div>)}
          </div>
        </section>

        <section className="contentSection" id="boundaries">
          <div className="sectionLabel">{zh ? "严格项目边界" : "Strict boundaries"}</div>
          <h2>{zh ? "没有声明，就不静默猜测。" : "If it is not declared, Pinset does not guess."}</h2>
          <p>{zh ? "项目默认不继承全局版本、不回退系统 PATH。联网安装、传统版本文件导入与系统回退都必须显式发生。" : "Projects do not inherit global versions or fall back to system PATH by default. Network installs, legacy file imports, and fallback must be explicit."}</p>
          <div className="callout"><strong>{zh ? "可解释" : "Explainable"}</strong><span><code>current --explain</code>、<code>which --explain</code>、<code>doctor</code> {zh ? "会说明最终选择和失败位置。" : "show the final selection and where resolution failed."}</span></div>
        </section>

        <section className="contentSection" id="commands">
          <div className="sectionLabel">CLI</div>
          <h2>{zh ? "每一个命令，都有完整说明。" : "Every command, fully documented."}</h2>
          <p>{zh ? "命令页包含用途、语法、状态修改、示例、JSON 支持、退出码和关键错误。" : "Command pages include purpose, syntax, state changes, examples, JSON support, exit codes, and key errors."}</p>
          <div className="groupPreview">
            {groups.map((group) => <Link href={`${prefix}/docs/commands#${group.commands[0]?.slug}`} key={group.title}><strong>{group.title}</strong><span>{group.commands.length} {zh ? "个命令" : "commands"}</span><b>→</b></Link>)}
          </div>
        </section>

        <footer className="pageFooter"><span>Pinset · MIT License</span><a href={siteConfig.repository}>GitHub →</a></footer>
      </article>
    </DocsShell>
  );
}
