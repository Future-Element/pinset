import type { ReactNode } from "react";
import { createRootMetadata, siteViewport } from "@/lib/metadata";
import "../globals.css";

export const metadata = createRootMetadata("en");
export const viewport = siteViewport;

export default function EnglishRootLayout({ children }: { children: ReactNode }) {
  return <html lang="en" data-scroll-behavior="smooth"><body>{children}</body></html>;
}
