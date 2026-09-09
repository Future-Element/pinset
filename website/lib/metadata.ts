import type { Metadata, Viewport } from "next";
import { languageAlternates, type Locale, siteConfig, siteUrl } from "./site";

export const openGraphImage = {
  url: `${siteUrl}/opengraph-image`,
  width: 1200,
  height: 630,
  alt: "Pinset — Every runtime. Right in place.",
  type: "image/png",
};

export const twitterImage = {
  url: `${siteUrl}/opengraph-image`,
  width: 1200,
  height: 630,
  alt: "Pinset — Every runtime. Right in place.",
};

export function createRootMetadata(locale: Locale): Metadata {
  const english = locale === "en";
  const title = english ? siteConfig.titleEn : siteConfig.titleZh;
  const description = english ? siteConfig.descriptionEn : siteConfig.descriptionZh;
  const canonical = english ? "/en" : "/";

  return {
    metadataBase: new URL(siteUrl),
    title: { default: title, template: "%s | Pinset" },
    description,
    applicationName: siteConfig.name,
    authors: [{ name: siteConfig.organization.name, url: siteConfig.organization.sameAs }],
    creator: siteConfig.organization.name,
    publisher: siteConfig.organization.name,
    keywords: [
      "Pinset",
      "runtime version manager",
      "polyglot runtime manager",
      "Node.js version manager",
      "Python version manager",
      "Rust toolchain",
      "reproducible development environments",
      "runtime lockfile",
    ],
    alternates: {
      canonical,
      languages: languageAlternates("/", "/en"),
    },
    openGraph: {
      type: "website",
      locale: english ? "en_US" : "zh_CN",
      alternateLocale: english ? "zh_CN" : "en_US",
      url: canonical,
      siteName: siteConfig.name,
      title,
      description,
      images: [openGraphImage],
    },
    twitter: {
      card: "summary_large_image",
      title,
      description,
      images: [twitterImage],
    },
    robots: {
      index: true,
      follow: true,
      googleBot: {
        index: true,
        follow: true,
        "max-image-preview": "large",
        "max-snippet": -1,
        "max-video-preview": -1,
      },
    },
    category: "technology",
  };
}

export const siteViewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  themeColor: "#4f57d8",
  colorScheme: "light",
};
