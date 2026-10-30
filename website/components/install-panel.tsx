"use client";

import Link from "next/link";
import { useState } from "react";
import type { InstallMethod } from "@/lib/analytics";
import { localePrefix, siteConfig, type Locale } from "@/lib/site";
import { useAnalytics } from "./analytics-provider";
import { CopyButton } from "./copy-button";
import { LatestReleaseVersion, useLatestReleaseVersion } from "./latest-release";

const installMethods = {
  unix: {
    label: "macOS / Linux",
    command: "curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/install.sh | sh",
    sourceUrl: `${siteConfig.repository}/blob/main/install.sh`,
    sourceLabel: { zh: "查看安装脚本", en: "View install script" },
  },
  windows: {
    label: "Windows",
    command: "Invoke-WebRequest https://raw.githubusercontent.com/Future-Element/pinset/main/install.ps1 -OutFile install.ps1\n.\\install.ps1",
    sourceUrl: `${siteConfig.repository}/blob/main/install.ps1`,
    sourceLabel: { zh: "查看安装脚本", en: "View install script" },
  },
  homebrew: {
    label: "Homebrew",
    command: "brew install Future-Element/tap/pinset",
    sourceUrl: "https://github.com/Future-Element/homebrew-tap/blob/main/Formula/pinset.rb",
    sourceLabel: { zh: "查看 Formula", en: "View formula" },
  },
  scoop: {
    label: "Scoop",
    command: "scoop install https://raw.githubusercontent.com/Future-Element/scoop-bucket/main/bucket/pinset.json",
    sourceUrl: "https://github.com/Future-Element/scoop-bucket/blob/main/bucket/pinset.json",
    sourceLabel: { zh: "查看 manifest", en: "View manifest" },
  },
};

export function InstallPanel({ locale }: { locale: Locale }) {
  const capture = useAnalytics();
  const [method, setMethod] = useState<InstallMethod>("unix");
  const installation = installMethods[method];
  const zh = locale === "zh-CN";
  const release = useLatestReleaseVersion();
  return (
    <div className="installPanel" id="install">
      <div className="installHeading"><strong>{zh ? "安装 Pinset" : "Install Pinset"} <LatestReleaseVersion fallback={zh ? "最新稳定版" : "latest stable"} prefix="v" /></strong><span>MIT · {zh ? "免费开源" : "Free & open source"}</span></div>
      {!release && <p>{zh ? "版本信息暂未载入。安装脚本默认获取最新稳定版，也可以在 GitHub 查看发布记录。" : "Release details have not loaded. The installer fetches the latest stable release by default; you can also view releases on GitHub."}</p>}
      <div className="platformSwitch" role="group" aria-label={zh ? "安装方式" : "Installation method"}>
        {(Object.keys(installMethods) as InstallMethod[]).map((option) => <button key={option} type="button" aria-pressed={method === option} onClick={() => { setMethod(option); capture("install_method_selected", { method: option }); }}>{installMethods[option].label}</button>)}
      </div>
      <div className="installCommand"><span className="terminalPrompt" aria-hidden="true">$</span><pre><code>{installation.command}</code></pre><CopyButton key={method} value={installation.command} label={zh ? "复制" : "Copy"} copiedLabel={zh ? "已复制" : "Copied"} errorLabel={zh ? "请选中命令手动复制" : "Select and copy manually"} onCopied={() => capture("install_command_copied", { method })} /></div>
      <div className="installLinks"><a href={installation.sourceUrl} onClick={() => capture("install_source_clicked", { method })}>{zh ? installation.sourceLabel.zh : installation.sourceLabel.en} ↗</a><Link href={`${localePrefix(locale)}/docs/commands/self-shell`} data-analytics-event="shell_setup_clicked">{zh ? "接着配置 Shell" : "Then set up your shell"} →</Link><a href="https://github.com/marketplace/actions/setup-pinset" data-analytics-event="ci_action_clicked">CI: setup-pinset ↗</a></div>
    </div>
  );
}
