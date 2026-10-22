import fs from "node:fs";
import path from "node:path";
import type { Locale } from "./site";

export type CommandDoc = {
  group: string;
  slug: string;
  title: string;
  markdown: string;
  description: string;
};

export type CommandGroup = {
  title: string;
  commands: CommandDoc[];
};

export type CommandNavigationGroup = {
  title: string;
  commands: Pick<CommandDoc, "slug" | "title" | "description">[];
};

// Navigation and search do not need every command's full Markdown body.
export function commandNavigation(groups: CommandGroup[]): CommandNavigationGroup[] {
  return groups.map(({ title, commands }) => ({ title, commands: commands.map(({ slug, title, description }) => ({ slug, title, description })) }));
}

function docsPath(locale: Locale) {
  const filename = locale === "en" ? "commands.md" : "commands.zh-CN.md";
  return path.resolve(process.cwd(), "..", "docs", filename);
}

function stripInlineMarkdown(value: string) {
  return value.replace(/[`*_~]/g, "").replace(/\[(.*?)\]\(.*?\)/g, "$1").trim();
}

export function commandSlug(command: string) {
  return command.toLowerCase().replace(/[^a-z0-9.]+/g, "-").replace(/\./g, "-").replace(/^-|-$/g, "");
}

function extractPurpose(markdown: string, command: string) {
  const [paragraph, ...body] = markdown.split(/\n\s*\n/);
  if (!paragraph || /^(?:#{1,6}\s|```|\||[-*>]\s)/.test(paragraph) || /^(?:pinset .+ 命令参考[。.]?|Reference for pinset .+)$/i.test(paragraph)) {
    throw new Error(`pinset ${command}: start the command document with a purpose paragraph`);
  }
  return { description: stripInlineMarkdown(paragraph.replace(/\s+/g, " ")), markdown: body.join("\n\n").trim() };
}

export function getCommandDocs(locale: Locale): CommandDoc[] {
  const source = fs.readFileSync(docsPath(locale), "utf8");
  const lines = source.split(/\r?\n/);
  const docs: CommandDoc[] = [];
  let group = locale === "en" ? "Commands" : "命令";
  let inFence = false;
  let current: { group: string; title: string; body: string[] } | null = null;

  const flush = () => {
    if (!current) return;
    const markdown = current.body.join("\n").trim();
    docs.push({
      group: current.group,
      slug: commandSlug(current.title),
      title: current.title,
      ...extractPurpose(markdown, current.title),
    });
  };

  for (const line of lines) {
    if (/^\s*```/.test(line)) {
      inFence = !inFence;
      if (current) current.body.push(line);
      continue;
    }

    if (!inFence) {
      const h2 = line.match(/^##\s+(.+)$/);
      if (h2) {
        flush();
        current = null;
        group = stripInlineMarkdown(h2[1]);
        continue;
      }

      const h3 = line.match(/^###\s+`([^`]+)`\s*$/);
      if (h3) {
        flush();
        current = { group, title: h3[1].trim(), body: [] };
        continue;
      }

      if (/^###\s+/.test(line)) {
        flush();
        current = null;
        continue;
      }
    }

    if (current) current.body.push(line);
  }

  flush();
  return docs;
}

export function getCommandGroups(locale: Locale): CommandGroup[] {
  const groups = new Map<string, CommandDoc[]>();
  for (const command of getCommandDocs(locale)) {
    const entries = groups.get(command.group) || [];
    entries.push(command);
    groups.set(command.group, entries);
  }
  return Array.from(groups, ([title, commands]) => ({ title, commands }));
}

export function getCommand(locale: Locale, slug: string) {
  return getCommandDocs(locale).find((command) => command.slug === slug);
}
