import type { Metadata } from "next";
import { HomePage } from "@/components/home-page";
import { getCommandGroups } from "@/lib/commands";
import { languageAlternates } from "@/lib/site";

export const metadata: Metadata = {
  alternates: { canonical: "/", languages: languageAlternates("/", "/en") },
};

export default function Page() {
  return <HomePage locale="zh-CN" groups={getCommandGroups("zh-CN")} />;
}
