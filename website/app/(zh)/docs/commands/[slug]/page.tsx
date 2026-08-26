import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { CommandPage } from "@/components/command-page";
import { getCommand, getCommandDocs, getCommandGroups } from "@/lib/commands";
import { openGraphImage, twitterImage } from "@/lib/metadata";
import { languageAlternates } from "@/lib/site";

export function generateStaticParams() {
  return getCommandDocs("zh-CN").map(({ slug }) => ({ slug }));
}

export async function generateMetadata({ params }: { params: Promise<{ slug: string }> }): Promise<Metadata> {
  const { slug } = await params;
  const command = getCommand("zh-CN", slug);
  if (!command) return {};
  return {
    title: `pinset ${command.title} 命令`,
    description: command.description,
    alternates: {
      canonical: `/docs/commands/${slug}`,
      languages: languageAlternates(`/docs/commands/${slug}`, `/en/docs/commands/${slug}`),
    },
    openGraph: {
      type: "article",
      title: `pinset ${command.title}`,
      description: command.description,
      url: `/docs/commands/${slug}`,
      images: [openGraphImage],
    },
    twitter: {
      card: "summary_large_image",
      title: `pinset ${command.title}`,
      description: command.description,
      images: [twitterImage],
    },
  };
}

export default async function Page({ params }: { params: Promise<{ slug: string }> }) {
  const { slug } = await params;
  const command = getCommand("zh-CN", slug);
  if (!command) notFound();
  return <CommandPage locale="zh-CN" command={command} groups={getCommandGroups("zh-CN")} />;
}
