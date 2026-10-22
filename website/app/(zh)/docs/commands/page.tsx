import type { Metadata } from "next";
import { CommandIndexPage } from "@/components/command-index-page";
import { getCommandGroups } from "@/lib/commands";
import { openGraphImage, twitterImage } from "@/lib/metadata";
import { languageAlternates, siteConfig } from "@/lib/site";

export const metadata: Metadata = {
  title: "命令参考",
  description: "按用途查找 Pinset 命令：开始管理项目、选择和还原工具版本、检查环境、管理密钥与清理安装。每页包含示例、执行结果和参数。",
  alternates: {
    canonical: "/docs/commands",
    languages: languageAlternates("/docs/commands", "/en/docs/commands"),
  },
  openGraph: {
    type: "website",
    siteName: siteConfig.name,
    url: "/docs/commands",
    title: "Pinset 命令参考",
    description: "按用途查找 Pinset 命令，了解什么时候用、怎么运行，以及运行后会发生什么。",
    images: [openGraphImage],
  },
  twitter: {
    card: "summary_large_image",
    title: "Pinset 命令参考",
    description: "Pinset 完整 CLI 命令参考。",
    images: [twitterImage],
  },
};

export default function Page() {
  return <CommandIndexPage locale="zh-CN" groups={getCommandGroups("zh-CN")} />;
}
