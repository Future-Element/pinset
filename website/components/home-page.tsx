import Link from "next/link";
import { commandNavigation, type CommandGroup } from "@/lib/commands";
import { localePrefix, siteConfig, siteUrl, type Locale } from "@/lib/site";
import { Brand } from "./brand";
import { CodeBlock } from "./code-block";
import { DocsShell } from "./docs-shell";
import { InstallPanel } from "./install-panel";
import { LatestReleaseVersion } from "./latest-release";
import { ProjectPreview } from "./project-preview";

const providers = [
  ["Node.js", "node · npm · npx"], ["pnpm", "pnpm"], ["Bun", "bun · bunx"],
  ["Go", "go · gofmt"], ["Python", "python · pip"], ["Java", "Temurin JDK"],
  ["Rust", "cargo · rustc"], [".NET", "dotnet"], ["Flutter / Dart", "flutter · dart"],
];

export function HomePage({ locale, groups }: { locale: Locale; groups: CommandGroup[] }) {
  const zh = locale === "zh-CN";
  const prefix = localePrefix(locale);
  const commandCount = groups.reduce((count, group) => count + group.commands.length, 0);
  const pageUrl = zh ? siteUrl + "/" : siteUrl + "/en";
  const organizationId = siteConfig.organization.url + "/#organization";
  const websiteId = siteUrl + "/#website";
  const softwareId = siteUrl + "/#software";
  const faq = zh ? [
    ["Pinset 是什么？", "Pinset 是一个多语言运行时版本管理器。它用 pinset.toml 声明项目需要的工具与策略，用 pinset.lock 保存精确版本、平台制品和完整性信息。"],
    ["安装 Pinset 后，还需要配置 Shell 吗？", "需要。Pinset 不会自动修改 Shell 配置文件。按照 activate 命令文档，为 Bash、Zsh、Fish 或 PowerShell 配置初始化，让项目命令路由到 Pinset 管理的版本。"],
    ["能和团队、CI 使用相同的版本吗？", "将 pinset.toml 和 pinset.lock 提交到仓库，团队成员和 CI 使用 pinset install --locked 安装锁定的工具。项目所需平台必须有对应的可用制品。"],
    ["支持哪些操作系统？", "支持 Windows、macOS 和 Linux，具体架构取决于工具的上游发行包。例如 Flutter 上游没有可用于当前安装模型的 Linux ARM64 SDK，Pinset 会明确提示不支持。"],
    ["Pinset 会自动使用系统里的运行时吗？", "默认不会。项目默认不继承全局版本，也不静默回退到系统 PATH。需要通过项目策略显式允许；current --explain 和 which --explain 可以解释最终选择。"],
  ] : [
    ["What is Pinset?", "Pinset is a polyglot runtime version manager. It uses pinset.toml to declare project tools and policy, and pinset.lock to record exact versions, platform artifacts, and integrity metadata."],
    ["Do I need to set up my shell after installation?", "Yes. Pinset does not edit shell profiles automatically. Follow the activate command reference for Bash, Zsh, Fish, or PowerShell so project commands route to Pinset-managed versions."],
    ["Can my team and CI use the same versions?", "Commit pinset.toml and pinset.lock, then run pinset install --locked on another machine or in CI. Each required platform must have a supported runtime artifact."],
    ["Which operating systems are supported?", "Pinset supports Windows, macOS, and Linux. Architecture support depends on each tool's upstream releases. For example, Flutter does not provide a Linux ARM64 SDK compatible with the current installation model, so Pinset reports an unsupported target."],
    ["Does Pinset fall back to system runtimes?", "Not by default. Projects do not silently inherit global versions or fall back to system PATH. Allow either explicitly through policy; current --explain and which --explain show how a command was resolved."],
  ];
  const jsonLd = {
    "@context": "https://schema.org",
    "@graph": [
      ...(zh ? [{
        "@type": "WebSite", "@id": websiteId, name: siteConfig.name,
        alternateName: siteConfig.alternateName, url: siteUrl + "/",
        inLanguage: ["zh-CN", "en"], publisher: { "@id": organizationId },
      }] : []),
      {
        "@type": "Organization", "@id": organizationId, name: siteConfig.organization.name,
        url: siteConfig.organization.url, sameAs: [siteConfig.organization.sameAs],
      },
      {
        "@type": "SoftwareApplication", "@id": softwareId, name: siteConfig.name,
        alternateName: siteConfig.alternateName, url: siteUrl,
        applicationCategory: "DeveloperApplication", operatingSystem: "Windows, Linux, macOS",
        license: "https://opensource.org/license/mit",
        codeRepository: siteConfig.repository,
        description: zh ? siteConfig.descriptionZh : siteConfig.descriptionEn,
        image: siteUrl + "/opengraph-image", downloadUrl: siteConfig.repository + "/releases",
        isAccessibleForFree: true, offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
        author: { "@id": organizationId }, publisher: { "@id": organizationId },
      },
      {
        "@type": "WebPage", "@id": pageUrl + "#webpage",
        name: zh ? siteConfig.titleZh : siteConfig.titleEn, url: pageUrl,
        inLanguage: locale, isPartOf: { "@id": websiteId }, about: { "@id": softwareId },
        primaryImageOfPage: { "@type": "ImageObject", url: siteUrl + "/opengraph-image", width: 1200, height: 630 },
      },
    ],
  };
  const features = zh ? [
    ["01", "锁定整个工具链", "Node.js、Python、Rust 和更多工具共享一份项目配置。版本选择与精确制品分别记录，团队直接按锁文件安装。", "pinset install --locked", "install"],
    ["02", "每一次选择，都能解释", "项目边界、全局继承、系统回退由策略决定。遇到版本不一致，可以查看来源、路由与诊断结果。", "pinset current --explain", "current"],
    ["03", "环境跟着项目走", "使用 age 加密的项目环境，在本地信任授权后注入命令进程。切换项目时，工具版本和选中的环境一起就位。", "pinset trust status", "trust-status"],
  ] : [
    ["01", "Lock the whole toolchain", "Node.js, Python, Rust, and more share one project configuration. Keep version intent separate from exact artifacts, then install from the lockfile.", "pinset install --locked", "install"],
    ["02", "Know why a version runs", "Project boundaries, global inheritance, and system fallback are explicit policy. Inspect the source, routing, and diagnostics behind every selection.", "pinset current --explain", "current"],
    ["03", "Keep environments in context", "Store project environments with age encryption. After local trust is granted, inject the selected profile into commands as you work.", "pinset trust status", "trust-status"],
  ];

  return (
    <DocsShell groups={commandNavigation(groups)} locale={locale} landing>
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd).replace(/</g, "\\u003c") }} />
      <div className="landingPage">
        <section className="landingHero">
          <div className="heroCopy">
            <a className="releaseLink" href={siteConfig.repository + "/releases/latest"}><span>PINSET <LatestReleaseVersion fallback={zh ? "最新版本" : "LATEST"} /></span>{zh ? "查看版本更新" : "Explore the release"} <span aria-hidden="true">↗</span></a>
            <h1>{zh ? <>多语言开发，<br /><em>一处锁定。</em></> : <>Every runtime.<br /><em>Right in place.</em></>}</h1>
            <p className="heroDescription">{zh ? "Pinset 多语言运行时版本管理器，用一份配置与精确锁文件，让项目、团队和 CI 使用同一组工具版本。" : "Pinset is a polyglot runtime version manager. One configuration and an exact lockfile keep your project, team, and CI on the same toolchain."}</p>
            <InstallPanel locale={locale} />
            <a className="textCta" href="#getting-started">{zh ? "从你的下一个项目开始" : "Start with your next project"} <span>→</span></a>
          </div>
          <ProjectPreview locale={locale} />
        </section>
        <section className="toolStrip" aria-label={zh ? "支持的运行时和工具" : "Supported runtimes and tools"}>
          <p>{zh ? "你熟悉的工具。统一的项目环境。" : "THE TOOLS YOU KNOW. ONE PROJECT ENVIRONMENT."}</p>
          <div>{providers.map(([name]) => <a href="#providers" key={name}>{name}</a>)}</div>
        </section>
        <section className="landingSection" id="features">
          <div className="sectionIntro"><span className="eyebrow">{zh ? "为真实的开发工作准备" : "BUILT FOR EVERYDAY DEVELOPMENT"}</span><h2>{zh ? <>把版本管理好。<br /><span>把注意力留给代码。</span></> : <>Set your toolchain.<br /><span>Get back to building.</span></>}</h2><p>{zh ? "从本地项目到自动化流程，让运行环境可复现、可解释、可检查。" : "From a local checkout to an automated build, make the environment reproducible, explainable, and verifiable."}</p></div>
          <div className="featureGrid">{features.map(([number, title, description, command, slug]) => <article key={number}><span className="featureNumber">{number} /</span><h3>{title}</h3><p>{description}</p><Link href={prefix + "/docs/commands/" + slug}><code>{command}</code><span>↗</span></Link></article>)}</div>
        </section>
        <section className="workflowSection landingSection" id="getting-started">
          <div className="workflowIntro"><span className="eyebrow">{zh ? "快速开始" : "QUICKSTART"}</span><h2>{zh ? <>一次声明，<br />每次都就位。</> : <>Declare it once.<br />Use it everywhere.</>}</h2><p>{zh ? "先安装 Pinset 并完成 Shell 初始化，再在项目目录中选择工具版本。" : "Install Pinset and set up your shell, then choose the tools for your project."}</p><Link className="textCta" href={prefix + "/docs/commands/activate"}>{zh ? "Shell 初始化指南" : "Shell setup guide"} →</Link><div className="workflowSignature"><img src="/brand/pinset-mark.svg" alt="" width="56" height="62" loading="lazy" /><span>pinset.toml<br />+ pinset.lock</span></div></div>
          <div className="workflowSteps">
            <article><span className="stepNumber">01</span><div><h3>{zh ? "创建项目，选择版本" : "Create a project. Choose versions."}</h3><p>{zh ? "在项目目录执行，生成项目配置和精确锁文件。" : "Run inside your project to create its configuration and exact lockfile."}</p><CodeBlock locale={locale} label={zh ? "项目终端" : "Project terminal"}>{"pinset init\npinset use node@24 pnpm@11 python@3.14 --no-install\npinset install --locked"}</CodeBlock></div></article>
            <article><span className="stepNumber">02</span><div><h3>{zh ? "直接使用熟悉的命令" : "Run the commands you already know."}</h3><p>{zh ? "初始化 Shell 后，命令会路由到项目锁定的工具。" : "With your shell configured, commands route to the project's locked tools."}</p><CodeBlock locale={locale} label={zh ? "项目终端" : "Terminal"}>{"node --version\npython --version\npinset which node --explain"}</CodeBlock></div></article>
            <article><span className="stepNumber">03</span><div><h3>{zh ? "把同一套环境交给团队和 CI" : "Bring your team and CI along."}</h3><p>{zh ? "提交 pinset.toml 与 pinset.lock；在新机器上按锁文件安装。" : "Commit pinset.toml and pinset.lock, then install the locked toolchain on the next machine."}</p><CodeBlock locale={locale} label={zh ? "新机器 / CI" : "New machine / CI"}>{"pinset install --locked"}</CodeBlock></div></article>
          </div>
        </section>
        <section className="landingSection" id="providers">
          <div className="sectionIntro sectionIntroSplit"><div><span className="eyebrow">TOOLCHAINS</span><h2>{zh ? <>不止一种语言。<br /><span>只需一种工作方式。</span></> : <>More than one language.<br /><span>Just one workflow.</span></>}</h2></div><p>{zh ? "管理运行时、包管理器和声明式开发 CLI。各工具保留自己的命令，Pinset 负责把它们放在正确的位置。" : "Manage runtimes, package managers, and declarative development CLIs. Each tool keeps its commands. Pinset puts them in the right place."}</p></div>
          <div className="toolchainGrid">{providers.map(([name, commands], index) => <div key={name}><span className="toolchainIndex">{String(index + 1).padStart(2, "0")}</span><h3>{name}</h3><code>{commands}</code></div>)}<Link href={prefix + "/docs/commands/provider-list"} className="moreTools"><span className="toolchainIndex">+</span><h3>{zh ? "还有更多 CLI" : "And more CLIs"}</h3><span>{zh ? "探索 Provider" : "Explore Providers"} ↗</span></Link></div>
          <p className="supportNote">{zh ? "支持 Windows、macOS、Linux。具体架构支持取决于各工具上游发行包。" : "Available on Windows, macOS, and Linux. Architecture support depends on each tool's upstream releases."} <a href={siteConfig.repository + "#supported-providers"}>{zh ? "查看支持矩阵" : "View the support matrix"} ↗</a></p>
        </section>
        <section className="boundarySection landingSection" id="boundaries"><div><span className="eyebrow">{zh ? "可验证，才可依赖" : "CONFIDENCE THROUGH VERIFICATION"}</span><h2>{zh ? <>每个版本有来源。<br />每条边界有声明。</> : <>A source for every version.<br />A policy for every boundary.</>}</h2><p>{zh ? "完整性校验、显式项目策略和诊断命令，让“为什么运行这个版本”有据可查。" : "Integrity checks, explicit project policy, and diagnostics make “why this version?” a question you can answer."}</p></div><div className="verificationList">{[
          ["pinset lock audit", zh ? "检查锁文件中的完整性信息" : "Audit integrity metadata in the lockfile", "lock-audit"],
          ["pinset doctor --deep", zh ? "深入检查安装、路由与环境" : "Inspect installations, routing, and environment", "doctor"],
          ["pinset current --explain", zh ? "追溯项目选择与策略来源" : "Trace project selection and policy", "current"],
        ].map(([command, label, slug]) => <Link href={prefix + "/docs/commands/" + slug} key={command}><code>{command}</code><span>{label}</span><b>↗</b></Link>)}</div></section>
        <section className="landingSection" id="commands"><div className="sectionIntro sectionIntroSplit"><div><span className="eyebrow">DOCUMENTATION</span><h2>{zh ? "从第一条命令，到整个工作流。" : "From your first command to your whole workflow."}</h2></div><Link className="textCta" href={prefix + "/docs/commands"}>{zh ? "浏览全部 " + commandCount + " 个命令" : "Explore all " + commandCount + " commands"} →</Link></div><div className="commandGrid">{groups.map(group => <Link key={group.title} href={prefix + "/docs/commands#" + group.commands[0]?.slug}><span>{group.title}</span><small>{group.commands.length} {zh ? "个命令" : "commands"}</small><b>↗</b></Link>)}</div></section>
        <section className="landingSection faqSection" id="faq"><div className="sectionIntro"><span className="eyebrow">{zh ? "你可能想知道" : "GOOD QUESTIONS"}</span><h2>{zh ? "开始之前。" : "Before you get started."}</h2></div><div>{faq.map(([question, answer]) => <details key={question}><summary>{question}<span aria-hidden="true">+</span></summary><p>{answer}</p></details>)}</div></section>
        <section className="closingSection"><img src="/brand/pinset-mark.svg" alt="" width="64" height="70" loading="lazy" /><h2>{zh ? "让下一个项目，一开始就就位。" : "Start your next project in the right place."}</h2><p>{zh ? "开源、免费，让开发环境跟着项目走。" : "Free and open source. A development environment that belongs to your project."}</p><a className="solidCta" href="#install">{zh ? "安装 Pinset" : "Install Pinset"} <span>↑</span></a><Link className="textCta" href={prefix + "/docs/commands"}>{zh ? "阅读文档" : "Read the docs"} →</Link></section>
        <footer className="landingFooter"><div><Brand href={prefix || "/"} /><p>{zh ? "多语言开发，一处锁定。" : "Every runtime. Right in place."}</p></div><nav aria-label={zh ? "页脚导航" : "Footer navigation"}><Link href={prefix + "/docs/commands"}>{zh ? "命令文档" : "Documentation"}</Link><a href={siteConfig.repository}>GitHub ↗</a><a href={siteConfig.repository + "/releases"}>{zh ? "版本发布" : "Releases"} ↗</a><a href={siteConfig.repository + "/blob/main/LICENSE"}>MIT License ↗</a></nav><div className="footerCredit">© {new Date(siteConfig.contentUpdatedAt).getUTCFullYear()} Future Element<span>{zh ? "为开发者而造。" : "Made for developers."}</span></div></footer>
      </div>
    </DocsShell>
  );
}
