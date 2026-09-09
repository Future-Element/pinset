import type { Metadata } from "next";
import { HomePage } from "@/components/home-page";
import { getCommandGroups } from "@/lib/commands";
import { openGraphImage } from "@/lib/metadata";
import { languageAlternates, siteConfig } from "@/lib/site";

export const metadata: Metadata = {
  description: siteConfig.descriptionEn,
  alternates: { canonical: "/en", languages: languageAlternates("/", "/en") },
  openGraph: {
    locale: "en_US",
    alternateLocale: "zh_CN",
    siteName: siteConfig.name,
    type: "website",
    url: "/en",
    title: siteConfig.titleEn,
    description: siteConfig.descriptionEn,
    images: [openGraphImage],
  },
};

export default function Page() {
  return <HomePage locale="en" groups={getCommandGroups("en")} />;
}
