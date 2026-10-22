import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { GuidePage, guideMetadata } from "@/components/guide-page";
import { getGuide, guideSlugs } from "@/lib/guides";

export const dynamicParams = false;
export function generateStaticParams() { return guideSlugs.map((guide) => ({ guide })); }
export async function generateMetadata({ params }: { params: Promise<{ guide: string }> }): Promise<Metadata> {
  return guideMetadata("zh-CN", (await params).guide);
}
export default async function Page({ params }: { params: Promise<{ guide: string }> }) {
  const guide = getGuide("zh-CN", (await params).guide);
  if (!guide) notFound();
  return <GuidePage locale="zh-CN" guide={guide} />;
}
