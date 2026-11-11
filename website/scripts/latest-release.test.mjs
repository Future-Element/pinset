import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import ts from "typescript";

async function sourceModule(relative, typescript = false) {
  const source = await readFile(new URL(relative, import.meta.url), "utf8");
  const javascript = typescript ? ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } }).outputText : source;
  return import(`data:text/javascript;base64,${Buffer.from(javascript).toString("base64")}`);
}

const { onRequestGet } = await sourceModule("../functions/api/latest-release.js");
const { releaseVersion, cachedRelease, releaseCacheLifetime, installerCommand } = await sourceModule("../lib/latest-release.ts", true);

function apiFixture(t, upstream) {
  const entries = new Map();
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, "caches");
  Object.defineProperty(globalThis, "caches", { configurable: true, value: { default: {
    async match(key) { return entries.get(key.url)?.clone(); },
    async put(key, value) { entries.set(key.url, value); },
  } } });
  t.after(() => {
    if (descriptor) Object.defineProperty(globalThis, "caches", descriptor);
    else delete globalThis.caches;
  });
  const fetch = t.mock.method(globalThis, "fetch", upstream);
  const pending = [];
  return {
    fetch,
    entries,
    async request(url = "https://pinset.example/api/latest-release") {
      const response = await onRequestGet({ request: new Request(url), waitUntil(work) { pending.push(work); } });
      await Promise.all(pending);
      return response;
    },
  };
}

test("official stable v3 redirect supplies one cached version across query strings", async (t) => {
  const fixture = apiFixture(t, async (url, init) => {
    assert.equal(url, "https://github.com/Future-Element/pinset/releases/latest");
    assert.equal(init.redirect, "manual");
    return new Response(null, { status: 302, headers: { Location: "https://github.com/Future-Element/pinset/releases/tag/v3.1.1" } });
  });
  const response = await fixture.request("https://pinset.example/api/latest-release?first=1");
  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { version: "3.1.1" });
  assert.equal(response.headers.get("Cache-Control"), "public, max-age=300");
  assert.deepEqual(await (await fixture.request("https://pinset.example/api/latest-release?second=1")).json(), { version: "3.1.1" });
  assert.equal(fixture.fetch.mock.callCount(), 1);
});

for (const [name, status, location] of [
  ["non-redirect success", 200, "https://github.com/Future-Element/pinset/releases/tag/v3.1.1"],
  ["upstream error with a Location", 500, "https://github.com/Future-Element/pinset/releases/tag/v3.1.1"],
  ["another protocol line", 302, "https://github.com/Future-Element/pinset/releases/tag/v4.0.0"],
  ["prerelease", 302, "https://github.com/Future-Element/pinset/releases/tag/v3.2.0-rc.1"],
  ["another repository", 302, "https://github.com/example/pinset/releases/tag/v3.1.1"],
  ["another origin", 302, "https://example.com/Future-Element/pinset/releases/tag/v3.1.1"],
  ["missing Location", 302, null],
]) {
  test(`release API refuses ${name} without caching a version`, async (t) => {
    const fixture = apiFixture(t, async () => new Response(null, { status, headers: location ? { Location: location } : {} }));
    const response = await fixture.request();
    assert.equal(response.status, 503);
    assert.deepEqual(await response.json(), { error: "latest_release_unavailable" });
    assert.equal(response.headers.get("Cache-Control"), "no-store");
    assert.equal(fixture.entries.size, 0);
  });
}

test("network failures return a neutral unavailable response", async (t) => {
  const fixture = apiFixture(t, async () => { throw new Error("offline"); });
  assert.equal((await fixture.request()).status, 503);
  assert.equal(fixture.entries.size, 0);
});

test("session cache expires and rejects malformed or future timestamps", () => {
  const now = 1_000_000;
  assert.deepEqual(cachedRelease({ version: "3.1.1", fetchedAt: now - 1 }, now), { version: "3.1.1", fetchedAt: now - 1 });
  for (const fetchedAt of [undefined, "recent", null, NaN, Infinity, now + 1, now - releaseCacheLifetime]) {
    assert.equal(cachedRelease({ version: "3.1.1", fetchedAt }, now), null);
  }
  for (const version of ["4.0.0", "3.2.0-rc.1", "3.1.1; echo unexpected", null]) {
    assert.equal(releaseVersion({ version }), null);
    assert.equal(cachedRelease({ version, fetchedAt: now }, now), null);
  }
});

test("script commands install exactly the displayed release and fall back to the script recommendation", () => {
  assert.ok(installerCommand("unix", "3.1.1").endsWith("| sh -s -- --version 3.1.1"));
  assert.ok(installerCommand("windows", "3.1.1").endsWith(".\\install.ps1 -Version 3.1.1"));
  assert.ok(installerCommand("unix", null).endsWith("| sh"));
  assert.ok(installerCommand("windows", null).endsWith(".\\install.ps1"));
  assert.equal(installerCommand("unix", "3.1.1; echo unexpected"), installerCommand("unix", null));
});
