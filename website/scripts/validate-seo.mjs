import { readFile, readdir } from "node:fs/promises";
import path from "node:path";

const origin = "https://pinset.future-element.com";
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
for (const entry of htmlFiles) {
  const html = await readFile(path.join("out", entry), "utf8");
  const label = entry.replaceAll("\\", "/");
  requireText(html, '<meta name="application-name" content="Pinset"', label);
  requireText(html, '<meta property="og:site_name" content="Pinset"', label);
  requireText(html, `<link rel="canonical" href="${origin}`, label);
}

console.log(`website site-name and canonical SEO signals are consistent across ${htmlFiles.length} pages`);
