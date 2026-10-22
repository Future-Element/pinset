# Pinset website

Next.js App Router product website and command reference. Chinese and English homepages include installation commands, project examples, supported tools, and a quickstart. Command pages are generated at build time from the canonical bilingual documents in `../docs/commands*.md`; the core concepts, custom CLI, and CI guides come from their matching bilingual files in `../docs/`.

The visual direction uses large typography, clear section borders, and an installation-first layout informed by [Bun's website](https://bun.sh/), with Pinset's own indigo palette, logo, and product content. No Bun graphics or fonts are included.

The approved interlocking P logo has a web SVG adaptation at `public/brand/pinset-mark.svg`. Keep `app/icon.svg` identical; Open Graph and Apple touch images read that same source at build time. The original concept and provenance notes are in `../output/brand/pinset-logo-v1/`.

## Development

```powershell
pnpm install
pnpm dev
```

Open <http://localhost:3000>.

The homepage's available release version is resolved at runtime from the same-origin `/api/latest-release` Pages Function. Documentation shows the version of the checked-in `package.json`, so newer releases cannot silently change which version its instructions describe. To exercise that Function locally, build first and use the Pages runtime:

```powershell
pnpm build
pnpm exec wrangler pages dev out
```

## Validation and SEO

```powershell
pnpm typecheck
pnpm build
pnpm validate:seo
```

The export validator checks all indexable pages for unique titles, self-referencing canonical URLs, Chinese/English/x-default alternates, one h1, descriptions, crawlability, sitemap coverage, structured data, generated logo image dimensions, and working internal links. It checks all 29 bilingual command pages for purpose summaries, four command groups, detail templates, visible parameter tables, version labels, starter flows, and guide links. The homepage keeps a dynamic release version. `NEXT_PUBLIC_SITE_URL` must match the build origin when validating a custom origin. Homepage and command-document modification dates are maintained separately in `lib/site.ts`.

For each `### \`command\`` section, the first normal paragraph is the canonical purpose summary used in directory cards, search, page subtitles, and metadata. The body starts at the next paragraph. A missing purpose is a build error. Four `##` headings define navigation groups. Keep examples and implementation details below the purpose. Relative links to the command, concepts, custom CLI, and CI Markdown sources are mapped to the matching localized website routes.

Main content is exported as HTML. Interactive installation controls and project examples use small client components; navigation receives summaries rather than all command bodies. Homepage structured data describes the free software without inventing ratings or reviews. These checks validate the build, not search rankings, indexing, or deployment.

## Production

Set the public production origin before building so canonical URLs, Open Graph metadata, `robots.txt`, and `sitemap.xml` use the deployed domain:

```powershell
$env:NEXT_PUBLIC_SITE_URL = "https://pinset.future-element.com"
pnpm build
pnpm exec wrangler pages deploy out --project-name pinset
```

The production site is deployed to Cloudflare Pages and served from `pinset.future-element.com`.

Wrangler automatically deploys `functions/api/latest-release.js` with the static export. It validates GitHub's latest-release redirect and caches the resolved stable version at the edge for five minutes, so publishing a new GitHub Release does not require rebuilding or redeploying the website.
