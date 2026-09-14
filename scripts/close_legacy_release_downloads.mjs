#!/usr/bin/env node

const REQUIRED_CUTOFF_ASSETS = [
  "SHA256SUMS",
  "pinset-linux-x86_64.tar.gz",
  "pinset-linux-aarch64.tar.gz",
  "pinset-macos-aarch64.tar.gz",
  "pinset-windows-x86_64.zip",
];

function parseVersion(value) {
  const match = /^v?(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/.exec(value);
  if (!match) return null;
  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    prerelease: match[4] === undefined ? null : match[4].split("."),
  };
}

function compareVersions(left, right) {
  for (const field of ["major", "minor", "patch"]) {
    if (left[field] !== right[field]) return left[field] - right[field];
  }
  if (left.prerelease === null && right.prerelease === null) return 0;
  if (left.prerelease === null) return 1;
  if (right.prerelease === null) return -1;
  for (let index = 0; index < Math.max(left.prerelease.length, right.prerelease.length); index += 1) {
    const leftPart = left.prerelease[index];
    const rightPart = right.prerelease[index];
    if (leftPart === undefined) return -1;
    if (rightPart === undefined) return 1;
    if (leftPart === rightPart) continue;
    const leftNumeric = /^\d+$/.test(leftPart);
    const rightNumeric = /^\d+$/.test(rightPart);
    if (leftNumeric && rightNumeric) return Number(leftPart) - Number(rightPart);
    if (leftNumeric !== rightNumeric) return leftNumeric ? -1 : 1;
    return leftPart < rightPart ? -1 : 1;
  }
  return 0;
}

function argument(name, fallback) {
  const index = process.argv.indexOf(name);
  return index === -1 ? fallback : process.argv[index + 1];
}

async function github(path, method = "GET") {
  const token = process.env.GH_TOKEN || process.env.GITHUB_TOKEN;
  const response = await fetch(`https://api.github.com${path}`, {
    method,
    headers: {
      Accept: "application/vnd.github+json",
      "User-Agent": "pinset-release-maintenance",
      "X-GitHub-Api-Version": "2022-11-28",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
    },
  });
  if (!response.ok) {
    throw new Error(`${method} ${path} failed: ${response.status} ${await response.text()}`);
  }
  return response.status === 204 ? null : response.json();
}

async function listReleases(repository) {
  const releases = [];
  for (let page = 1; ; page += 1) {
    const batch = await github(`/repos/${repository}/releases?per_page=100&page=${page}`);
    releases.push(...batch);
    if (batch.length < 100) return releases;
  }
}

async function main() {
  if (process.argv.includes("--self-test")) {
    const cutoff = parseVersion("2.16.0");
    if (!cutoff || compareVersions(parseVersion("2.15.0"), cutoff) >= 0) process.exit(1);
    if (compareVersions(parseVersion("v0.1.0-alpha.6"), cutoff) >= 0) process.exit(1);
    if (compareVersions(parseVersion("v0.1.0-beta.1"), cutoff) >= 0) process.exit(1);
    if (compareVersions(parseVersion("2.16.0-beta.11"), parseVersion("2.16.0-rc.1")) >= 0) process.exit(1);
    if (compareVersions(parseVersion("2.16.0-beta.2"), parseVersion("2.16.0-beta.11")) >= 0) process.exit(1);
    if (compareVersions(parseVersion("2.16.0-rc.2"), cutoff) >= 0) process.exit(1);
    if (compareVersions(parseVersion("2.16.0"), cutoff) !== 0) process.exit(1);
    if (compareVersions(parseVersion("2.17.0-rc.1"), cutoff) <= 0) process.exit(1);
    console.log("legacy release version selection passed");
    return;
  }

  const repository = argument("--repository", process.env.GITHUB_REPOSITORY || "Future-Element/pinset");
  const cutoffText = argument("--cutoff", "2.16.0");
  const cutoff = parseVersion(cutoffText);
  const apply = process.argv.includes("--apply");
  if (!cutoff || compareVersions(cutoff, parseVersion("2.16.0")) !== 0) {
    throw new Error("the legacy download cutoff must be exactly 2.16.0");
  }
  if (apply && !(process.env.GH_TOKEN || process.env.GITHUB_TOKEN)) {
    throw new Error("GH_TOKEN or GITHUB_TOKEN is required with --apply");
  }

  const releases = await listReleases(repository);
  const cutoffRelease = releases.find(
    (release) => release.tag_name === `v${cutoffText}` && !release.draft,
  );
  if (!cutoffRelease) throw new Error(`published v${cutoffText} release was not found`);
  const cutoffAssets = new Set(cutoffRelease.assets.map((asset) => asset.name));
  const missing = REQUIRED_CUTOFF_ASSETS.filter((name) => !cutoffAssets.has(name));
  if (missing.length) {
    throw new Error(`v${cutoffText} is missing required assets: ${missing.join(", ")}`);
  }

  const legacy = releases
    .map((release) => ({ release, version: parseVersion(release.tag_name) }))
    .filter(({ version }) => version && compareVersions(version, cutoff) < 0)
    .sort((left, right) => compareVersions(left.version, right.version));
  const assets = legacy.flatMap(({ release }) =>
    release.assets.map((asset) => ({ release, asset })),
  );
  for (const { release, asset } of assets) {
    console.log(`${apply ? "delete" : "would-delete"} ${release.tag_name}/${asset.name}`);
    if (apply) await github(`/repos/${repository}/releases/assets/${asset.id}`, "DELETE");
  }

  if (apply) {
    for (const { release } of legacy) {
      const refreshed = await github(`/repos/${repository}/releases/${release.id}`);
      if (refreshed.assets.length) {
        throw new Error(`${release.tag_name} still exposes ${refreshed.assets.length} assets`);
      }
    }
  }
  console.log(
    `${apply ? "closed" : "planned"} ${assets.length} binary download assets across ${legacy.length} releases before v${cutoffText}`,
  );
  console.log("release notes and Git tags are retained; GitHub-generated source archives remain available");
}

main().catch((error) => {
  console.error(`close legacy downloads: ${error.message}`);
  process.exit(1);
});
