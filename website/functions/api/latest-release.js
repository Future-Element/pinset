const githubLatestRelease = "https://github.com/Future-Element/pinset/releases/latest";
const releaseLocation = /^https:\/\/github\.com\/Future-Element\/pinset\/releases\/tag\/v(\d+\.\d+\.\d+)$/;

function json(body, init = {}) {
  const headers = new Headers(init.headers);
  headers.set("Content-Type", "application/json; charset=utf-8");
  headers.set("X-Content-Type-Options", "nosniff");
  return new Response(JSON.stringify(body), { ...init, headers });
}

export async function onRequestGet({ request, waitUntil }) {
  const cacheUrl = new URL(request.url);
  cacheUrl.search = "";
  const cacheKey = new Request(cacheUrl.toString(), { method: "GET" });
  const cache = caches.default;
  const cached = await cache.match(cacheKey);
  if (cached) return cached;

  try {
    const upstream = await fetch(githubLatestRelease, {
      method: "HEAD",
      redirect: "manual",
      headers: { "User-Agent": "Pinset-website" },
    });
    const match = upstream.headers.get("Location")?.match(releaseLocation);
    if (!match) throw new Error(`Unexpected GitHub release redirect: ${upstream.status}`);

    const response = json(
      { version: match[1] },
      { headers: { "Cache-Control": "public, max-age=300" } },
    );
    waitUntil(cache.put(cacheKey, response.clone()));
    return response;
  } catch {
    return json(
      { error: "latest_release_unavailable" },
      { status: 503, headers: { "Cache-Control": "no-store" } },
    );
  }
}
