"use client";

import Link from "next/link";
import { useState } from "react";
import { localePrefix, siteConfig, type Locale } from "@/lib/site";
import { CopyButton } from "./copy-button";
import { LatestReleaseVersion, useLatestReleaseVersion } from "./latest-release";

const installCommands = {
  unix: "curl -fsSL https://raw.githubusercontent.com/Future-Element/pinset/main/install.sh | sh",
  windows: "Invoke-WebRequest https://raw.githubusercontent.com/Future-Element/pinset/main/install.ps1 -OutFile install.ps1\n.\\install.ps1",
};

export function InstallPanel({ locale }: { locale: Locale }) {
  const [platform, setPlatform] = useState<"unix" | "windows">("unix");
  const zh = locale === "zh-CN";
  const release = useLatestReleaseVersion();
  return (
    <div className="installPanel" id="install">
      <div className="installHeading"><strong>{zh ? "安装 Pinset" : "Install Pinset"} <LatestReleaseVersion fallback={zh ? "最新稳定版" : "latest stable"} prefix="v" /></strong><span>MIT · {zh ? "免费开源" : "Free & open source"}</span></div>
      {!release && <p>{zh ? "版本信息暂未载入。安装脚本默认获取最新稳定版，也可以在 GitHub 查看发布记录。" : "Release details have not loaded. The installer fetches the latest stable release by default; you can also view releases on GitHub."}</p>}
      <div className="platformSwitch" role="group" aria-label={zh ? "操作系统" : "Operating system"}>
        <button type="button" aria-pressed={platform === "unix"} onClick={() => setPlatform("unix")}>macOS / Linux</button>
        <button type="button" aria-pressed={platform === "windows"} onClick={() => setPlatform("windows")}>Windows</button>
      </div>
      <div className="installCommand"><span className="terminalPrompt" aria-hidden="true">$</span><pre><code>{installCommands[platform]}</code></pre><CopyButton key={platform} value={installCommands[platform]} label={zh ? "复制" : "Copy"} copiedLabel={zh ? "已复制" : "Copied"} errorLabel={zh ? "请选中命令手动复制" : "Select and copy manually"} /></div>
      <div className="installLinks"><a href={`${siteConfig.repository}/blob/main/install.${platform === "unix" ? "sh" : "ps1"}`}>{zh ? "查看安装脚本" : "View install script"} ↗</a><Link href={`${localePrefix(locale)}/docs/commands/self-shell`}>{zh ? "接着配置 Shell" : "Then set up your shell"} →</Link></div>
    </div>
  );
}
