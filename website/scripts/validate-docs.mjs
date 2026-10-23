import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";

const plain = (value) => value.replace(/[`*_~]/g, "").replace(/\[(.*?)\]\(.*?\)/g, "$1").replace(/\s+/g, " ").trim();
const slug = (value) => value.toLowerCase().replace(/[^a-z0-9.]+/g, "-").replace(/\./g, "-").replace(/^-|-$/g, "");
const decode = (value) => value.replace(/&quot;/g, '"').replace(/&#x27;|&#39;/g, "'").replace(/&lt;/g, "<").replace(/&gt;/g, ">").replace(/&amp;/g, "&");
const withoutComments = (value) => value.replace(/<!--[\s\S]*?-->/g, "");

function commands(source) {
  const result = [];
  let group;
  let current;
  let fenced = false;
  const flush = () => { if (current) result.push(current); current = undefined; };
  for (const line of source.split(/\r?\n/)) {
    if (/^\s*```/.test(line)) { fenced = !fenced; current?.lines.push(line); continue; }
    if (!fenced && /^## /.test(line)) { flush(); group = line.slice(3); continue; }
    const title = !fenced && line.match(/^### `([^`]+)`\s*$/)?.[1];
    if (title) { flush(); current = { title, slug: slug(title), group, lines: [] }; continue; }
    if (!fenced && /^### /.test(line)) { flush(); continue; }
    current?.lines.push(line);
  }
  flush();
  return result;
}

export async function validateDocs() {
  const version = JSON.parse(await readFile("package.json", "utf8")).version;
  const sets = [];
  for (const locale of ["zh-CN", "en"]) {
    const prefix = locale === "en" ? "en/" : "";
    const source = await readFile(path.join("..", "docs", locale === "en" ? "commands.md" : "commands.zh-CN.md"), "utf8");
    const docs = commands(source);
    assert.equal(docs.length, 29, `${locale}: every command must have a document`);
    assert.equal(new Set(docs.map((doc) => doc.group)).size, 4, `${locale}: expected four command groups`);
    assert.equal(new Set(docs.map((doc) => doc.slug)).size, docs.length, `${locale}: duplicate command URLs`);
    sets.push(docs.map((doc) => doc.slug));
    const index = withoutComments(await readFile(path.join("out", prefix, "docs/commands.html"), "utf8"));
    assert.ok(index.includes(`Pinset ${version}`), `${locale}: index must name its documented version`);
    assert.ok(!index.includes("data-latest-release-version"), `${locale}: documentation must not claim a newer dynamic release`);
    for (const command of ["init", "use", "import", "install", "check"]) assert.ok(index.includes(`href="/${prefix}docs/commands/${command}"`), `${locale}: starter flow missing ${command}`);
    for (const doc of docs) {
      const body = doc.lines.join("\n").trim();
      const [summary, ...rest] = body.split(/\n\s*\n/);
      assert.ok(summary && !/^(?:#|```|\||[-*>] )/.test(summary), `${locale}/${doc.slug}: missing first-paragraph purpose`);
      assert.ok(!/^(?:pinset .+ 命令参考[。.]?|Reference for pinset .+)$/i.test(summary), `${locale}/${doc.slug}: template summary`);
      const description = plain(summary);
      assert.ok(description.length >= 15, `${locale}/${doc.slug}: purpose is too short`);
      const html = withoutComments(await readFile(path.join("out", prefix, "docs/commands", `${doc.slug}.html`), "utf8"));
      const header = html.match(/<header class="docHeader compactHeader">([\s\S]*?)<\/header>/)?.[1];
      assert.ok(header, `${locale}/${doc.slug}: missing detail header`);
      const subtitle = header.match(/<p>([\s\S]*?)<\/p>/)?.[1];
      assert.equal(decode(subtitle || ""), description, `${locale}/${doc.slug}: detail subtitle differs from source purpose`);
      const meta = html.match(/<meta name="description" content="([^"]+)"/)?.[1];
      assert.equal(decode(meta || ""), description, `${locale}/${doc.slug}: SEO purpose differs from detail`);
      assert.ok(index.includes(description.replaceAll("&", "&amp;").replaceAll('"', "&quot;").replaceAll("'", "&#x27;").replaceAll("<", "&lt;").replaceAll(">", "&gt;")), `${locale}/${doc.slug}: index lacks purpose`);
      const rendered = html.match(/<div class="markdownBody">([\s\S]*?)<\/div><nav class="pager"/)?.[1];
      assert.ok(rendered, `${locale}/${doc.slug}: missing command body`);
      assert.ok(rendered.startsWith("<h2 "), `${locale}/${doc.slug}: detail sections must follow the page h1 with h2`);
      assert.ok(!plain(decode(rendered.replace(/<[^>]+>/g, " "))).startsWith(description), `${locale}/${doc.slug}: duplicated purpose`);
      assert.ok((rest.join("\n\n").match(/^#### /gm) || []).length >= 4, `${locale}/${doc.slug}: incomplete detail template`);
      assert.ok(/```(?:sh|shell|bash|powershell|text)/.test(body), `${locale}/${doc.slug}: missing executable example or output`);
      assert.ok(rendered.includes("<table>") || /无专用参数|No (?:command-specific|dedicated) (?:options|parameters)/i.test(body), `${locale}/${doc.slug}: missing parameter table or explicit no-options note`);
      assert.ok(html.includes(`Pinset ${version}`), `${locale}/${doc.slug}: missing documented version`);
    }
    for (const guide of ["concepts", "generic-tools", "ci", "agents", "migrate-from-volta"]) {
      const html = withoutComments(await readFile(path.join("out", prefix, "docs", `${guide}.html`), "utf8"));
      const guideSource = await readFile(path.join("..", "docs", `${guide}${locale === "en" ? "" : ".zh-CN"}.md`), "utf8");
      const paragraphs = guideSource.trim().split(/\r?\n\s*\r?\n/).slice(1);
      const purpose = paragraphs.find((paragraph) => !/^(?:\[[^\]]+\]\([^)]+\)(?:\s*[·|]\s*)?)+$/.test(paragraph));
      const header = html.match(/<header class="docHeader compactHeader">([\s\S]*?)<\/header>/)?.[1];
      assert.equal(decode(header?.match(/<p>([\s\S]*?)<\/p>/)?.[1] || ""), plain(purpose || ""), `${locale}/${guide}: guide subtitle must use its purpose, not language links`);
      assert.ok(html.includes('class="markdownBody"'), `${locale}/${guide}: empty guide`);
      assert.ok(index.includes(`href="/${prefix}docs/${guide}"`), `${locale}/${guide}: missing guide entry`);
      assert.ok(html.includes(`href="/${prefix}docs/commands"`), `${locale}/${guide}: missing command index link`);
      assert.ok(html.includes(`Pinset ${version}`), `${locale}/${guide}: missing documented version`);
    }
  }
  assert.deepEqual(sets[0], sets[1], "bilingual command URLs must match");
  console.log("Documentation validated: 29 bilingual commands, four groups, shared purpose summaries, detail templates, version labels, starter flows, and five bilingual guides.");
}
