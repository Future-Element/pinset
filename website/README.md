# Pinset website

Next.js App Router product website and command reference. Chinese and English homepages include installation commands, project examples, supported tools, and a quickstart. Command pages are generated at build time from the canonical bilingual documents in `../docs/commands*.md`; the core concepts, custom CLI, CI, AI coding agent and Volta migration guides come from their matching bilingual files in `../docs/`.

The visual direction uses large typography, clear section borders, and an installation-first layout informed by [Bun's website](https://bun.sh/), with Pinset's own indigo palette, logo, and product content. No Bun graphics or fonts are included.

The approved interlocking P logo has a web SVG adaptation at `public/brand/pinset-mark.svg`. Keep `app/icon.svg` identical; Open Graph and Apple touch images read that same source at build time. The original concept and provenance notes are in `../output/brand/pinset-logo-v1/`.

## Development

```powershell
pnpm install
pnpm dev
```

Open <http://localhost:3000>.

### Preview analytics

Copy `.env.example` to `.env.local` and set `NEXT_PUBLIC_POSTHOG_KEY` to enable PostHog locally, or set it in the Cloudflare Pages **Preview** environment before building a `*.pages.dev` preview. Leave the key empty to disable analytics entirely. `NEXT_PUBLIC_POSTHOG_HOST` defaults to `https://us.i.posthog.com`. These public variables are embedded at build time; keep real keys out of the repository and leave the production Pages environment unchanged.

Only the ten explicit marketing events are captured; automatic pageviews, clicks, session replay, and feature flags are disabled. Every event includes `locale`, `path`, and `release_shown`: the latest runtime release on the landing page when available, falling back to `siteConfig.docsVersion` (from `package.json`) while loading or when `/api/latest-release` is unavailable; docs pages always use `siteConfig.docsVersion`. `landing_viewed` fires once and is not captured again when the live release resolves. `docs_nav_clicked.target` is the localized destination path and fragment. FAQ IDs (`faq-1` through `faq-5`) match across locales. Installation copy events fire only after clipboard writes succeed; other code-block copies are not tracked.

The homepage's available release version is resolved at runtime from the same-origin `/api/latest-release` Pages Function, using the official GitHub latest stable v3 release redirect as its authority. It does not freeze a tag into the homepage or use the source package version as a release substitute. Documentation shows the version of the checked-in `package.json`, so newer releases cannot silently change which version its instructions describe. To exercise that Function locally, build first and use the Pages runtime:

```powershell
pnpm build
pnpm exec wrangler pages dev out
```

## Validation and SEO

```sh
# From the repository root; validation runs only in the local Docker verifier.
bash verify.sh integrations
```

The integrations suite runs `pnpm test:release`, typechecks, builds and SEO checks in Docker. Release contracts cover valid and invalid upstream redirects, outages, cache expiry, malformed cache timestamps, and exact version arguments in the two script installation commands. Build and deployment scripts do not run tests.

For a manual release check, compare `https://github.com/Future-Element/pinset/releases/latest` with the JSON returned by `/api/latest-release` in the Pages runtime. Both homepages should show that version after loading. Their macOS/Linux and Windows script commands must pass the same exact version; Homebrew and Scoop explain that their formula/manifest may lag. Leave a tab open for five minutes or refocus it after cache expiry to check refresh. With the API unavailable, the page uses a version-neutral fallback and the script's embedded recommendation, without claiming that a release is unpublished. No release check deploys the site.

The export validator checks all indexable pages for unique titles, self-referencing canonical URLs, Chinese/English/x-default alternates, one h1, descriptions, crawlability, sitemap coverage, structured data, generated logo image dimensions, and working internal links. It checks all 29 bilingual command pages for purpose summaries, four command groups, detail templates, visible parameter tables, version labels, starter flows, and guide links. The homepage keeps a dynamic release version. `NEXT_PUBLIC_SITE_URL` must match the build origin when validating a custom origin. Homepage and command-document modification dates are maintained separately in `lib/site.ts`.

For each `### \`command\`` section, the first normal paragraph is the canonical purpose summary used in directory cards, search, page subtitles, and metadata. The body starts at the next paragraph. A missing purpose is a build error. Four `##` headings define navigation groups. Keep examples and implementation details below the purpose. Relative links to the command and all five guide Markdown sources are mapped to the matching localized website routes.

Main content is exported as HTML. Interactive installation controls and project examples use small client components; navigation receives summaries rather than all command bodies. Homepage structured data describes the free software without inventing ratings or reviews. These checks validate the build, not search rankings, indexing, or deployment.

## Production

Set the public production origin before building so canonical URLs, Open Graph metadata, `robots.txt`, and `sitemap.xml` use the deployed domain:

```powershell
$env:NEXT_PUBLIC_SITE_URL = "https://pinset.future-element.com"
pnpm build
pnpm exec wrangler pages deploy out --project-name pinset
```

The production site is deployed to Cloudflare Pages and served from `pinset.future-element.com`.

Wrangler automatically deploys `functions/api/latest-release.js` with the static export. It accepts redirect statuses only to this repository's stable v3 tags and caches the resolved version at the edge for five minutes. The browser validates its own five-minute session cache and refreshes open tabs periodically and on focus; an expired result gives way to neutral fallback text if refresh fails. Thus the edge and browser caches can each delay a release update by up to five minutes. Publishing a new GitHub Release does not require rebuilding or redeploying the website.
