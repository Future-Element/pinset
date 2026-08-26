import type { ReactNode } from "react";
import { createRootMetadata, siteViewport } from "@/lib/metadata";
import "../globals.css";

export const metadata = createRootMetadata("zh-CN");
export const viewport = siteViewport;

export default function ChineseRootLayout({ children }: { children: ReactNode }) {
  return <html lang="zh-CN"><body>{children}</body></html>;
}
