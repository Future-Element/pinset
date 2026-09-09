import { readFile, readdir } from "node:fs/promises";
import path from "node:path";

const origin = (process.env.NEXT_PUBLIC_SITE_URL || "https://pinset.future-element.com").replace(/\/$/, "");
const root = await readFile(path.join("out", "index.html"), "utf8");
const english = await readFile(path.join("out", "en.html"), "utf8");

function jsonLdNodes(html) {
  return [...html.matchAll(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/g)]
    .flatMap((match) => {
      const value = JSON.parse(match[1]);
      return Array.isArray(value["@graph"]) ? value["@graph"] : [value];
    });
}

function requireText(html, value, label) {
  if (!html.includes(value)) {
    throw new Error(`${label} is missing ${value}`);
  }
}

const websites = jsonLdNodes(root).filter((node) => node["@type"] === "WebSite");
if (websites.length !== 1) {
  throw new Error(`root page must contain exactly one WebSite node; found ${websites.length}`);
}
const website = websites[0];
if (website.name !== "Pinset" || website.alternateName !== "Pinset Runtime Manager") {
  throw new Error("WebSite names do not match the Pinset brand identity");
}
if (website.url !== `${origin}/`) {
  throw new Error(`WebSite URL must be the canonical subdomain root; received ${website.url}`);
}
if (JSON.stringify(website.alternateName).includes("future-element.com")) {
  throw new Error("a hostname must not be used as a brand alternateName");
}
if (jsonLdNodes(english).some((node) => node["@type"] === "WebSite")) {
  throw new Error("the language subpage must reference, not redefine, the root WebSite identity");
}

const htmlFiles = (await readdir("out", { recursive: true }))
  .filter((entry) => entry.endsWith(".html"))
  .filter((entry) => !entry.includes("404") && !entry.includes("_not-found"));
const titles = new Set();
const canonicalUrls = new Set();
const sitemap = await readFile(path.join("out", "sitemap.xml"), "utf8");
const sitemapUrls = new Set([...sitemap.matchAll(/<loc>(.*?)<\/loc>/g)].map((match) => new URL(match[1]).href));
for (const entry of htmlFiles) {
  const html = await readFile(path.join("out", entry), "utf8");
  const label = entry.replaceAll("\\", "/");
  requireText(html, '<meta name="application-name" content="Pinset"', label);
  requireText(html, '<meta property="og:site_name" content="Pinset"', label);
  requireText(html, `<link rel="canonical" href="${origin}`, label);
  const head = html.slice(0, html.indexOf("</head>"));
  const expectedPath = label === "index.html" ? "/" : `/${label.replace(/\.html$/, "")}`;
  const canonical = head.match(/<link rel="canonical" href="([^"]+)"/)?.[1];
  const expectedUrl = new URL(expectedPath, origin).href;
  if (new URL(canonical).href !== expectedUrl || canonicalUrls.has(expectedUrl)) {
    throw new Error(`${label}: canonical must point to its own unique URL, ${expectedUrl}`);
  }
  canonicalUrls.add(expectedUrl);
  const title = head.match(/<title>(.*?)<\/title>/)?.[1];
  if (!title || titles.has(title)) throw new Error(`${label}: missing or duplicate title`);
  titles.add(title);
  if ((html.match(/<h1[ >]/g) || []).length !== 1) throw new Error(`${label}: expected exactly one h1`);
  if (!head.match(/<meta name="description" content="[^"]+"/)) throw new Error(`${label}: missing description`);
  if (/https?:\/\/(?:localhost|127\.0\.0\.1)/.test(head)) throw new Error(`${label}: local URL in metadata`);
  const en = expectedPath === "/en" || expectedPath.startsWith("/en/");
  requireText(html, `<html lang="${en ? "en" : "zh-CN"}"`, label);
  requireText(head, '<meta name="robots" content="index, follow"', label);
  requireText(head, '<meta name="twitter:card" content="summary_large_image"', label);
  requireText(head, `<meta property="og:image" content="${origin}/opengraph-image`, label);
  requireText(head, '<link rel="apple-touch-icon"', label);
  const zhPath = en ? expectedPath.replace(/^\/en/, "") || "/" : expectedPath;
  const enPath = `/en${zhPath === "/" ? "" : zhPath}`;
  for (const [language, target] of [["zh-CN", zhPath], ["en", enPath], ["x-default", enPath]]) {
    const alternate = head.match(new RegExp(`<link rel="alternate" hrefLang="${language}" href="([^"]+)"`))?.[1];
    if (!alternate || new URL(alternate).href !== new URL(target, origin).href) throw new Error(`${label}: incorrect ${language} alternate`);
  }
  if (!sitemapUrls.has(expectedUrl)) throw new Error(`${label}: missing from sitemap`);
  for (const node of jsonLdNodes(html)) {
    if (node["@type"] === "TechArticle" && node.url !== canonical) throw new Error(`${label}: article URL mismatch`);
  }
  if (expectedPath.includes("/docs/commands/")) {
    const breadcrumbs = jsonLdNodes(html).find((node) => node["@type"] === "BreadcrumbList");
    if (breadcrumbs?.itemListElement?.at(-1)?.item !== canonical) throw new Error(`${label}: missing canonical breadcrumb`);
  }
}

if (sitemapUrls.size !== canonicalUrls.size) throw new Error("sitemap contains missing or extra pages");
const robots = await readFile(path.join("out", "robots.txt"), "utf8");
requireText(robots, `Sitemap: ${origin}/sitemap.xml`, "robots.txt");
for (const [asset, width, height] of [["opengraph-image", 1200, 630], ["apple-icon", 180, 180]]) {
  const png = await readFile(path.join("out", asset));
  if (png.subarray(0, 8).toString("hex") !== "89504e470d0a1a0a" || png.readUInt32BE(16) !== width || png.readUInt32BE(20) !== height) throw new Error(`${asset}: invalid PNG or dimensions`);
}
const mark = await readFile(path.join("out", "brand", "pinset-mark.svg"), "utf8");
const favicon = await readFile(path.join("out", "icon.svg"), "utf8");
if (mark.replaceAll("\r\n", "\n") !== favicon.replaceAll("\r\n", "\n")) {
  throw new Error("favicon and brand mark have drifted");
}
for (const [label, html] of [["zh", root], ["en", english]]) {
  const software = jsonLdNodes(html).find((node) => node["@type"] === "SoftwareApplication");
  if (!software?.isAccessibleForFree || software.offers?.price !== "0") throw new Error(`${label}: missing free software metadata`);
  if ("softwareVersion" in software) throw new Error(`${label}: release version must not be frozen into structured data`);
  requireText(html, 'href="https://github.com/Future-Element/pinset/releases/latest"', label);
  requireText(html, "data-latest-release-version", label);
  if (/PINSET \d+\.\d+\.\d+|Pinset v?\d+\.\d+\.\d+/.test(html)) throw new Error(`${label}: release version is frozen into static HTML`);
  requireText(html, 'id="faq"', label);
  requireText(html, 'src="/brand/pinset-mark.svg"', label);
  if (html.includes('\\"markdown\\":')) throw new Error(`${label}: full command bodies leaked into navigation data`);
  const current = new URL(label === "en" ? "/en" : "/", origin);
  for (const match of html.matchAll(/<a\b[^>]*href="([^"]+)"/g)) {
    const target = new URL(match[1].replaceAll("&amp;", "&"), current);
    if (target.origin !== current.origin) continue;
    const targetFile = target.pathname === "/" ? "index.html" : `${target.pathname.slice(1)}.html`;
    const targetHtml = await readFile(path.join("out", targetFile), "utf8");
    if (target.hash && !targetHtml.includes(`id="${decodeURIComponent(target.hash.slice(1))}"`)) throw new Error(`${label}: broken internal anchor ${target.href}`);
  }
}
console.log(`SEO validated: ${htmlFiles.length} pages, unique titles and canonicals, bilingual alternates, h1, descriptions, sitemap, robots, JSON-LD, and new logo assets.`);
