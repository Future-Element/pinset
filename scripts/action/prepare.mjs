import { createHash, randomUUID } from 'node:crypto';
import { appendFileSync, existsSync, readFileSync, realpathSync, statSync } from 'node:fs';
import { homedir } from 'node:os';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const exactVersion = /^3\.\d+\.\d+(?:-rc\.\d+)?$/;
const sha256 = (value) => createHash('sha256').update(value).digest('hex');

function booleanInput(value, name, fallback) {
  const actual = value === undefined ? fallback : value;
  if (actual !== 'true' && actual !== 'false') {
    throw new Error(`${name} must be true or false`);
  }
  return actual === 'true';
}

function singleLine(value, name) {
  if (typeof value !== 'string' || /[\r\n\0]/.test(value)) {
    throw new Error(`${name} must be a single line`);
  }
  return value;
}

export async function resolveVersion(input, token, request = fetch) {
  let version = (input || 'latest').trim();
  if (version === 'latest') {
    const headers = { Accept: 'application/vnd.github+json', 'User-Agent': 'setup-pinset' };
    if (token) headers.Authorization = `Bearer ${token}`;
    let release;
    for (let attempt = 0; attempt < 4; attempt++) {
      try {
        const response = await request('https://api.github.com/repos/Future-Element/pinset/releases/latest', {
          headers,
          signal: AbortSignal.timeout(30000),
          redirect: 'error',
        });
        if (!response.ok) throw new Error(`GitHub latest release request failed (${response.status})`);
        release = await response.json();
        break;
      } catch (error) {
        if (attempt === 3) throw error;
        await new Promise((resolve) => setTimeout(resolve, 1000 * (attempt + 1)));
      }
    }
    if (release.draft || release.prerelease || typeof release.tag_name !== 'string') {
      throw new Error('GitHub did not return a published stable Pinset release');
    }
    version = release.tag_name;
  }
  version = version.replace(/^v/, '');
  if (!exactVersion.test(version)) {
    throw new Error('version must be latest or an exact Pinset 3 stable/rc release (for example 3.0.9)');
  }
  return version;
}

// Match Pinset's discovery boundary: nearest Git checkout, otherwise the user's
// home when inside it, otherwise the requested directory itself. Never fall back
// to a global selection in CI when a project lock is missing.
export function findProject(start, userHome = homedir()) {
  const directory = realpathSync(start);
  if (!statSync(directory).isDirectory()) throw new Error('working-directory must be a directory');
  const ancestors = [];
  for (let current = directory; ; current = path.dirname(current)) {
    ancestors.push(current);
    if (path.dirname(current) === current) break;
  }
  const relativeHome = path.relative(realpathSync(userHome), directory);
  const withinHome = relativeHome === '' || (!relativeHome.startsWith(`..${path.sep}`) && relativeHome !== '..' && !path.isAbsolute(relativeHome));
  const boundary = ancestors.find((candidate) => existsSync(path.join(candidate, '.git')))
    || (withinHome ? realpathSync(userHome) : directory);
  for (const candidate of ancestors) {
    if (existsSync(path.join(candidate, '.pinset', 'config.toml'))) {
      const lock = path.join(candidate, '.pinset', 'lock.toml');
      if (!existsSync(lock) || !statSync(lock).isFile()) {
        throw new Error(`Project lock is missing: ${lock}; generate and commit .pinset/lock.toml before CI setup`);
      }
      return { root: candidate, lock };
    }
    if (candidate === boundary) break;
  }
  throw new Error(`No Pinset project config and lock found inside the project boundary from ${directory}`);
}

export async function prepare(env = process.env, request = fetch) {
  const install = booleanInput(env.PINSET_ACTION_INSTALL, 'install', 'true');
  booleanInput(env.PINSET_ACTION_CACHE, 'cache', 'true');
  const skipCheck = booleanInput(env.PINSET_ACTION_SKIP_CHECK, 'skip-check', 'false');
  const workspace = path.resolve(env.GITHUB_WORKSPACE || process.cwd());
  const home = singleLine(env.PINSET_HOME || path.join(homedir(), '.pinset'), 'PINSET_HOME');
  if (!path.isAbsolute(home)) throw new Error('PINSET_HOME must be absolute');
  const resolvedHome = path.normalize(home);
  const temporary = singleLine(env.RUNNER_TEMP, 'RUNNER_TEMP');
  if (!path.isAbsolute(temporary)) throw new Error('RUNNER_TEMP must be absolute');
  const directory = singleLine(path.resolve(workspace, env.PINSET_PROJECT_DIRECTORY || '.'), 'working-directory');
  const project = install ? findProject(directory) : { root: directory };
  const version = await resolveVersion(env.PINSET_ACTION_VERSION, env.PINSET_GITHUB_TOKEN, request);
  const platform = [env.RUNNER_OS, env.RUNNER_ARCH, env.ImageOS || 'unknown'].map((value) => {
    if (!value || !/^[a-zA-Z0-9_.-]+$/.test(value)) throw new Error('Runner platform metadata is unavailable or invalid');
    return value;
  }).join('-');
  const result = {
    version,
    'project-directory': project.root,
    'install-directory': path.join(temporary, 'pinset-v3', 'bin'),
    'cache-directory': path.join(resolvedHome, 'v3', 'installs'),
    // Absolute home identity prevents restoring SDKs with embedded paths into a
    // different installation prefix. No profiles, identities or local state are cached.
    'cache-key': install ? `pinset-installs-v1-${platform}-${version}-${sha256(resolvedHome).slice(0, 16)}-${sha256(readFileSync(project.lock))}` : '',
    'check-report': install && !skipCheck ? path.join(temporary, `pinset-check-${randomUUID()}.json`) : '',
  };
  for (const [name, value] of Object.entries(result)) {
    singleLine(value, name);
    appendFileSync(env.GITHUB_OUTPUT, `${name}=${value}\n`, 'utf8');
  }
  appendFileSync(env.GITHUB_ENV, `PINSET_HOME=${resolvedHome}\n`, 'utf8');
  return result;
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  prepare().catch((error) => {
    console.error(`setup-pinset: ${error.message}`);
    process.exitCode = 1;
  });
}
