import type { Metadata } from "next";
import { CommandIndexPage } from "@/components/command-index-page";
import { getCommandGroups } from "@/lib/commands";
import { openGraphImage, twitterImage } from "@/lib/metadata";
import { languageAlternates } from "@/lib/site";

export const metadata: Metadata = {
  title: "命令参考",
  description: "Pinset 2.1 完整 CLI 命令参考：语法、参数、状态修改、JSON、退出码与错误。",
  alternates: {
    canonical: "/docs/commands",
    languages: languageAlternates("/docs/commands", "/en/docs/commands"),
  },
  openGraph: {
    type: "website",
    url: "/docs/commands",
    title: "Pinset 2.1 命令参考",
    description: "Pinset 2.1 完整 CLI 命令参考：语法、参数、状态修改、JSON、退出码与错误。",
    images: [openGraphImage],
  },
  twitter: {
    card: "summary_large_image",
    title: "Pinset 2.1 命令参考",
    description: "Pinset 2.1 完整 CLI 命令参考。",
    images: [twitterImage],
  },
};

export default function Page() {
  return <CommandIndexPage locale="zh-CN" groups={getCommandGroups("zh-CN")} />;
}
