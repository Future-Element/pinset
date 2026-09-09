# Pinset website

Next.js App Router product website and command reference. Chinese and English homepages include installation commands, project examples, supported tools, and a quickstart. Command pages are generated at build time from the canonical bilingual documents in `../docs/commands*.md`.

The visual direction uses large typography, clear section borders, and an installation-first layout informed by [Bun's website](https://bun.sh/), with Pinset's own indigo palette, logo, and product content. No Bun graphics or fonts are included.

The approved interlocking P logo has a web SVG adaptation at `public/brand/pinset-mark.svg`. Keep `app/icon.svg` identical; Open Graph and Apple touch images read that same source at build time. The original concept and provenance notes are in `../output/brand/pinset-logo-v1/`.

## Development

```powershell
pnpm install
pnpm dev
```

Open <http://localhost:3000>.

The displayed Pinset version is resolved at runtime from the same-origin `/api/latest-release` Pages Function. To exercise that Function locally, build first and use the Pages runtime:

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

The export validator checks all indexable pages for unique titles, self-referencing canonical URLs, Chinese/English/x-default alternates, one h1, descriptions, crawlability, sitemap coverage, structured data, generated logo image dimensions, and the absence of a frozen release version. `NEXT_PUBLIC_SITE_URL` must match the build origin when validating a custom origin. Homepage and command-document modification dates are maintained separately in `lib/site.ts`.

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
