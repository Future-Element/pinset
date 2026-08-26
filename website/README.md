# Pinset documentation website

Next.js App Router documentation website. Command pages are generated at build time from the canonical bilingual documents in `../docs/commands*.md`.

## Development

```powershell
pnpm install
pnpm dev
```

Open <http://localhost:3000>.

## Production

Set the public production origin before building so canonical URLs, Open Graph metadata, `robots.txt`, and `sitemap.xml` use the deployed domain:

```powershell
$env:NEXT_PUBLIC_SITE_URL = "https://pinset.future-element.com"
pnpm build
pnpm exec wrangler pages deploy out --project-name pinset
```

The production site is deployed to Cloudflare Pages and served from `pinset.future-element.com`.
