import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { findProject, prepare, resolveVersion } from '../action/prepare.mjs';

const fixture = realpathSync(mkdtempSync(path.join(tmpdir(), 'pinset-setup-action-')));
let checks = 0;
function check(value, expected) {
  assert.deepEqual(value, expected);
  checks++;
}
try {
  const networkForbidden = () => { throw new Error('exact versions must not resolve remotely'); };
  check(await resolveVersion('3.0.8', 'secret', networkForbidden), '3.0.8');
  check(await resolveVersion('v3.1.0-rc.2', undefined, networkForbidden), '3.1.0-rc.2');
  for (const invalid of ['3', '>=3.0.0', '3.0.8;false', '4.0.0', '3.0.0\ninjected']) {
    await assert.rejects(resolveVersion(invalid, undefined, networkForbidden), /version must be/);
    checks++;
  }
  let latestRequests = 0;
  const latest = async (url, options) => {
    check(url, 'https://api.github.com/repos/Future-Element/pinset/releases/latest');
    check(options.headers.Authorization, 'Bearer test-token');
    check(options.redirect, 'error');
    latestRequests++;
    return { ok: true, json: async () => ({ tag_name: 'v3.9.1', draft: false, prerelease: false }) };
  };
  check(await resolveVersion('latest', 'test-token', latest), '3.9.1');
  check(latestRequests, 1);
  for (const release of [{ tag_name: 'v3.1.0', draft: true }, { tag_name: 'v3.1.0-rc.1', prerelease: true }]) {
    await assert.rejects(resolveVersion('latest', undefined, async () => ({ ok: true, json: async () => release })), /published stable/);
    checks++;
  }

  const project = path.join(fixture, 'project with spaces');
  const child = path.join(project, 'packages', 'app');
  mkdirSync(path.join(project, '.git'), { recursive: true });
  mkdirSync(path.join(project, '.pinset'));
  mkdirSync(child, { recursive: true });
  const config = path.join(project, '.pinset', 'config.toml');
  const lock = path.join(project, '.pinset', 'lock.toml');
  writeFileSync(config, '[tools]\n');
  writeFileSync(lock, 'locked content one\n');
  check(findProject(child, fixture), { root: project, lock });
  const nested = path.join(child, 'nested-checkout');
  mkdirSync(path.join(nested, '.git'), { recursive: true });
  assert.throws(() => findProject(nested, fixture), /No Pinset project/);
  checks++;
  const unconfigured = path.join(fixture, 'unconfigured');
  mkdirSync(unconfigured);
  assert.throws(() => findProject(unconfigured, path.join(fixture, 'project with spaces')), /No Pinset project/);
  checks++;

  const environment = {
    GITHUB_WORKSPACE: project,
    RUNNER_TEMP: fixture,
    RUNNER_OS: 'Linux',
    RUNNER_ARCH: 'X64',
    ImageOS: 'ubuntu24',
    PINSET_PROJECT_DIRECTORY: 'packages/app',
    PINSET_HOME: path.join(fixture, "Pinset's home"),
    PINSET_ACTION_VERSION: '3.0.8',
    PINSET_ACTION_INSTALL: 'true',
    PINSET_ACTION_CACHE: 'true',
    PINSET_ACTION_SKIP_CHECK: 'false',
    GITHUB_OUTPUT: path.join(fixture, 'outputs'),
    GITHUB_ENV: path.join(fixture, 'environment'),
  };
  const result = await prepare(environment, networkForbidden);
  check(result.version, '3.0.8');
  check(result['project-directory'], project);
  check(result['cache-directory'], path.join(environment.PINSET_HOME, 'v3', 'installs'));
  assert.ok(result['cache-key'].startsWith('pinset-installs-v1-Linux-X64-ubuntu24-3.0.8-'));
  assert.ok(result['cache-key'].endsWith(createHash('sha256').update(readFileSync(lock)).digest('hex')));
  check(readFileSync(environment.GITHUB_ENV, 'utf8'), `PINSET_HOME=${environment.PINSET_HOME}\n`);
  check(readFileSync(config, 'utf8'), '[tools]\n');
  check(readFileSync(lock, 'utf8'), 'locked content one\n');
  check(existsSync(environment.PINSET_HOME), false);
  check(existsSync(result['check-report']), false);
  const output = readFileSync(environment.GITHUB_OUTPUT, 'utf8');
  assert.ok(output.includes(`version=3.0.8\n`) && output.includes(`check-report=${result['check-report']}\n`));
  checks++;
  for (const change of [
    { PINSET_HOME: path.join(fixture, 'different-home') },
    { RUNNER_OS: 'Windows' },
    { RUNNER_ARCH: 'ARM64' },
    { ImageOS: 'ubuntu26' },
    { PINSET_ACTION_VERSION: '3.0.9' },
  ]) {
    const changed = await prepare({ ...environment, ...change }, networkForbidden);
    assert.notEqual(changed['cache-key'], result['cache-key']);
    checks++;
  }
  writeFileSync(lock, 'locked content two\n');
  assert.notEqual((await prepare(environment, networkForbidden))['cache-key'], result['cache-key']);
  checks++;
  for (const change of [
    { PINSET_ACTION_INSTALL: 'yes' },
    { PINSET_ACTION_CACHE: 'TRUE' },
    { PINSET_ACTION_SKIP_CHECK: 'maybe' },
    { PINSET_HOME: 'relative-home' },
    { PINSET_HOME: path.join(fixture, 'home\ninjected=value') },
    { PINSET_PROJECT_DIRECTORY: 'packages/app\ninjected=value' },
  ]) {
    await assert.rejects(prepare({ ...environment, ...change }, networkForbidden));
    checks++;
  }
  rmSync(lock);
  await assert.rejects(prepare(environment, networkForbidden), /Project lock is missing/);
  checks++;
  const binaryOnly = await prepare({ ...environment, PINSET_ACTION_INSTALL: 'false', PINSET_PROJECT_DIRECTORY: unconfigured }, networkForbidden);
  check(binaryOnly['cache-key'], '');
  check(binaryOnly['project-directory'], unconfigured);
  check(binaryOnly['check-report'], '');
  check((await prepare({ ...environment, PINSET_ACTION_INSTALL: 'false', PINSET_ACTION_SKIP_CHECK: 'true' }, networkForbidden))['check-report'], '');
  console.log(JSON.stringify({ status: 'passed', checks, scope: 'setup-pinset metadata and local preparation; no CI execution or native Windows/macOS runtime' }));
} finally {
  rmSync(fixture, { recursive: true, force: true });
}
