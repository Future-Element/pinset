"use client";

import { useState } from "react";
import type { Locale } from "@/lib/site";
import { useLatestReleaseVersion } from "./latest-release";

const config = '[policy]\nboundary = "git"\n\n[tools]\nnode = "24"\npnpm = "11"\npython = "3.14"';
const commands = "pinset init\npinset use node@24 pnpm@11 python@3.14 --no-install\npinset install --locked\n\nnode --version\npython --version\npinset current --explain";
export function ProjectPreview({ locale }: { locale: Locale }) {
  const zh = locale === "zh-CN";
  const [active, setActive] = useState(0);
  const version = useLatestReleaseVersion();
  const ci = `- uses: actions/checkout@v4\n- uses: Future-Element/pinset@${version ? `v${version}` : "main"}\n  with:\n    version: ${version ?? "latest"}\n- run: node --version`;
  const examples = [
    { title: "pinset.toml", code: config, note: zh ? "配置片段 · 精确制品记录在 pinset.lock。" : "Configuration excerpt · Exact artifacts live in pinset.lock." },
    { title: zh ? "终端" : "Terminal", code: commands, note: zh ? "在已初始化 Shell 的项目中运行这些命令。" : "Run these commands in a project after setting up your shell." },
    { title: "CI", code: ci, note: zh ? "GitHub Actions 示例片段，复用项目已提交的锁文件。" : "GitHub Actions snippet using the lockfile committed to your project." },
  ];
  return (
    <div className="projectPreview">
      <div className="previewTitle"><span><i /><i /><i /></span><span>~/your-next-project</span><span aria-hidden="true">↗</span></div>
      <div className="previewTabs" role="group" aria-label={zh ? "项目示例" : "Project examples"}>{examples.map((example, index) => <button type="button" key={example.title} aria-pressed={active === index} onClick={() => setActive(index)}>{example.title}</button>)}</div>
      {examples.map((example, index) => <div key={example.title} hidden={active !== index}><pre className="previewCode"><code>{example.code.split("\n").map((line, i) => <span className="previewLine" key={i}><span className="lineNumber" aria-hidden="true">{String(i + 1).padStart(2, "0")}</span><span className={line.startsWith("[") ? "codeSection" : line.includes("=") ? "codeValue" : ""}>{line || " "}</span></span>)}</code></pre><p className="previewNote">{example.note}</p></div>)}
      <div className="previewFooter"><span className="precisionDot" />{zh ? "同一个项目，同一组运行环境。" : "One project. One runtime set."}<span>→</span></div>
    </div>
  );
}
