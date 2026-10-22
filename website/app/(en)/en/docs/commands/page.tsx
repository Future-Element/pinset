import type { Metadata } from "next";
import { CommandIndexPage } from "@/components/command-index-page";
import { getCommandGroups } from "@/lib/commands";
import { openGraphImage, twitterImage } from "@/lib/metadata";
import { languageAlternates, siteConfig } from "@/lib/site";

export const metadata: Metadata = {
  title: "Command reference",
  description: "Find Pinset commands by task: start a project, select and restore tool versions, check your environment, manage secrets, and clean installations. Includes examples, results, and options.",
  alternates: {
    canonical: "/en/docs/commands",
    languages: languageAlternates("/docs/commands", "/en/docs/commands"),
  },
  openGraph: {
    type: "website",
    siteName: siteConfig.name,
    locale: "en_US",
    url: "/en/docs/commands",
    title: "Pinset command reference",
    description: "Find the Pinset command for your task, learn when to use it, and see what happens when it runs.",
    images: [openGraphImage],
  },
  twitter: {
    card: "summary_large_image",
    title: "Pinset command reference",
    description: "Complete Pinset CLI command reference.",
    images: [twitterImage],
  },
};

export default function Page() {
  return <CommandIndexPage locale="en" groups={getCommandGroups("en")} />;
}
