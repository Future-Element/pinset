import type { Metadata } from "next";
import { CommandIndexPage } from "@/components/command-index-page";
import { getCommandGroups } from "@/lib/commands";
import { openGraphImage, twitterImage } from "@/lib/metadata";
import { languageAlternates, siteConfig } from "@/lib/site";

export const metadata: Metadata = {
  title: "Command reference",
  description: "Complete Pinset 2.1 CLI command reference: syntax, parameters, state changes, JSON, exit codes, and errors.",
  alternates: {
    canonical: "/en/docs/commands",
    languages: languageAlternates("/docs/commands", "/en/docs/commands"),
  },
  openGraph: {
    type: "website",
    siteName: siteConfig.name,
    locale: "en_US",
    url: "/en/docs/commands",
    title: "Pinset 2.1 command reference",
    description: "Complete Pinset 2.1 CLI command reference: syntax, state changes, JSON, exit codes, and errors.",
    images: [openGraphImage],
  },
  twitter: {
    card: "summary_large_image",
    title: "Pinset 2.1 command reference",
    description: "Complete Pinset 2.1 CLI command reference.",
    images: [twitterImage],
  },
};

export default function Page() {
  return <CommandIndexPage locale="en" groups={getCommandGroups("en")} />;
}
