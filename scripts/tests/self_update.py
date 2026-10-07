"""Real consecutive CLI/shim updates against an isolated HTTPS release fixture."""
import hashlib, http.server, json, os, re, shutil, zipfile
from pathlib import Path
from support import BIN, run
from https_fixture import create_https_fixture


def verify_self_update(source, fixture, current_version):
    major, minor, patch = map(int, current_version.split('-')[0].split('.'))
    versions = [f'{major}.{minor}.{patch+1}', f'{major}.{minor+1}.0']
    fixture = fixture/'self-update'
    fixture.mkdir()
    project = fixture/'project'
    project.mkdir()
    home = fixture/'home'
    bin_dir = home/'v3/bin'
    bin_dir.mkdir(parents=True)
    (home/'v3/.pinset-home.json').write_text('{"protocol":"pinset/3","owner":"pinset"}')
    for name in ['pinset', 'pinset-shim']:
        shutil.copy2(BIN.parent/name, bin_dir/name)

    # Build real future binaries from this source in a separate container copy.
    # No host files or installed host runtimes are involved.
    build = fixture/'source'
    build.mkdir()
    shutil.copytree(source/'crates', build/'crates')
    manifest = (source/'Cargo.toml').read_text()
    lock = (source/'Cargo.lock').read_text()
    artifacts = {}
    for index, version in enumerate(versions):
        updated = manifest.replace(f'version = "{current_version}"', f'version = "{version}"', 1)
        assert updated != manifest
        (build/'Cargo.toml').write_text(updated)
        (build/'Cargo.lock').write_text(re.sub(
            r'(name = "pinset(?:-cli|-core|-engine|-env|-shim)"\nversion = ")[^"]+',
            lambda match: match[1]+version, lock))
        run(['cargo', 'build', '--offline', '--locked', '-p', 'pinset-cli', '-p', 'pinset-shim'], cwd=build, timeout=600)
        name = f'pinset-v{version}-linux-x86_64.zip' if index == 0 else 'pinset-linux-x86_64.zip'
        archive = fixture/name
        with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED, compresslevel=1) as output:
            for binary in ['pinset', 'pinset-shim']:
                path = Path(os.environ['CARGO_TARGET_DIR'])/'debug'/binary
                info = zipfile.ZipInfo(binary)
                info.create_system = 3
                info.external_attr = 0o100755 << 16
                output.writestr(info, path.read_bytes(), compress_type=zipfile.ZIP_DEFLATED, compresslevel=1)
        artifacts[version] = archive

    state = {'latest': versions[0], 'requests': [], 'checksum_error': None,
             'head_statuses': [], 'location': None, 'switch_after_head': None, 'checksum_redirect': None}

    class Origin(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass
        def do_HEAD(self):
            host = self.headers['Host'].split(':')[0]
            state['requests'].append(('HEAD', host, self.path))
            if host == 'api.github.com':
                self.send_error(403, 'REST API disabled for self-update acceptance')
                return
            if host != 'github.com' or self.path != '/Future-Element/pinset/releases/latest':
                self.send_error(404)
                return
            if state['head_statuses']:
                status, retry_after = state['head_statuses'].pop(0)
                self.send_response(status)
                if retry_after is not None:
                    self.send_header('Retry-After', retry_after)
                self.send_header('Connection', 'close')
                self.end_headers()
                return
            self.send_response(302)
            self.send_header('Location', state['location'] or
                             'https://github.com/Future-Element/pinset/releases/tag/v'+state['latest'])
            self.send_header('Connection', 'close')
            self.end_headers()
            if state['switch_after_head']:
                state['latest'] = state['switch_after_head']
                state['switch_after_head'] = None
        def do_GET(self):
            host = self.headers['Host'].split(':')[0]
            state['requests'].append(('GET', host, self.path))
            if host == 'api.github.com':
                self.send_error(403, 'REST API disabled for self-update acceptance')
                return
            if host == 'release-assets.githubusercontent.com':
                version = self.path.split('/')[1]
                name = 'SHA256SUMS'
            else:
                prefix = '/Future-Element/pinset/releases/download/v'
                if host != 'github.com' or not self.path.startswith(prefix):
                    self.send_error(404)
                    return
                version, name = self.path[len(prefix):].split('/', 1)
                if name == 'SHA256SUMS' and state['checksum_redirect']:
                    self.send_response(302)
                    self.send_header('Location', state['checksum_redirect'].format(version=version))
                    self.send_header('Connection', 'close')
                    self.end_headers()
                    return
            if version not in artifacts:
                self.send_error(404)
                return
            archive = artifacts[version]
            if name == 'SHA256SUMS':
                if state['checksum_error'] == 'missing':
                    self.send_error(404)
                    return
                digest = hashlib.sha256(archive.read_bytes()).hexdigest()
                body = (digest+'  '+archive.name+'\n').encode()
                if state['checksum_error'] == 'duplicate':
                    body *= 2
                if state['checksum_error'] == 'platform':
                    body = (digest+'  pinset-other-platform.zip\n').encode()
                if state['checksum_error'] == 'oversize':
                    body = b'x' * (1024 * 1024 + 1)
            elif name == archive.name:
                body = archive.read_bytes()
                if state['checksum_error'] == 'corrupt':
                    body = b'corrupt release payload'
            else:
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header('Content-Length', str(len(body)))
            self.send_header('Connection', 'close')
            self.end_headers()
            self.wfile.write(body)

    server, proxy_env = create_https_fixture(fixture, ['github.com','api.github.com','release-assets.githubusercontent.com'], Origin)
    env = {**os.environ, **proxy_env, 'HOME': str(fixture), 'PINSET_HOME': str(home)}
    for name in ['PINSET_IDENTITY', 'PINSET_PROFILE', 'PINSET_NO_ENV', 'PINSET_ENV_RESOLVED']:
        env.pop(name, None)
    command = [bin_dir/'pinset', '-C', project, '--json', 'self', 'update']
    snapshot = lambda: [(str(path.relative_to(home)), hashlib.sha256(path.read_bytes()).hexdigest())
                        for path in sorted(home.rglob('*')) if path.is_file()]
    try:
        for index, version in enumerate(versions):
            state['latest'] = version
            before = snapshot()
            first_request = len(state['requests'])
            plan = json.loads(run(command+['--plan'], env=env, cwd=project))
            assert plan['version'] == version and plan['artifact'] == artifacts[version].name
            assert snapshot() == before, 'self update preview wrote state'
            assert not any(path.endswith('.zip') for _, _, path in state['requests'][first_request:]), 'preview downloaded a binary'
            first_request = len(state['requests'])
            explicit = json.loads(run(command+[version, '--plan'], env=env, cwd=project))
            assert explicit == plan
            assert not any(method == 'HEAD' for method, _, _ in state['requests'][first_request:]), 'explicit version queried latest'
            if index == 0:
                # A new release becoming latest after HEAD must not change the
                # exact version directory used for checksums or the package.
                state['switch_after_head'] = versions[1]
                pinned = json.loads(run(command+['--plan'], env=env, cwd=project))
                assert pinned == plan and state['latest'] == versions[1]
                state['latest'] = version
                state['checksum_redirect'] = 'https://release-assets.githubusercontent.com/{version}/SHA256SUMS?proof=fixture'
                assert json.loads(run(command+['--plan'], env=env, cwd=project)) == plan
                state['checksum_redirect'] = 'https://example.com/SHA256SUMS'
                error = json.loads(run(command+['--plan'], env=env, cwd=project, expected=1))
                assert error['error']['code'] == 'PINSET_UPDATE_REDIRECT'
                state['checksum_redirect'] = None
                for location in ['https://github.com/other/pinset/releases/tag/v'+version,
                                 'http://github.com/Future-Element/pinset/releases/tag/v'+version,
                                 '/Future-Element/pinset/releases/tag/v3.1.0-rc.1']:
                    state['location'] = location
                    error = json.loads(run(command+['--plan'], env=env, cwd=project, expected=1))
                    assert error['error']['code'] == 'PINSET_UPDATE_REDIRECT'
                state['location'] = None
                for status, delay, code in [(403, None, 'PINSET_UPDATE_ACCESS'),
                                            (429, '3600', 'PINSET_UPDATE_RATE_LIMIT'),
                                            (429, 'Wed, 07 Oct 2037 12:00:00 GMT', 'PINSET_UPDATE_RATE_LIMIT'),
                                            (200, None, 'PINSET_UPDATE_REDIRECT')]:
                    state['head_statuses'] = [(status, delay)]
                    first_request = len(state['requests'])
                    error = json.loads(run(command+['--plan'], env=env, cwd=project, expected=1))
                    assert error['error']['code'] == code
                    assert len(state['requests']) == first_request+1, 'denied or long-delayed request retried'
                for status, delay in [(503, None), (429, '0')]:
                    state['head_statuses'] = [(status, delay)]
                    assert json.loads(run(command+['--plan'], env=env, cwd=project)) == plan
                for mode, code in [('missing', 'PINSET_UPDATE_NOT_FOUND'), ('platform', 'PINSET_UPDATE_ASSET'), ('oversize', 'PINSET_UPDATE_CHECKSUM')]:
                    state['checksum_error'] = mode
                    error = json.loads(run(command+['--plan'], env=env, cwd=project, expected=1))
                    assert error['error']['code'] == code
                state['checksum_error'] = None
                error = json.loads(run(command+['3.99.0', '--plan'], env=env, cwd=project, expected=1))
                assert error['error']['code'] == 'PINSET_UPDATE_NOT_FOUND'
                assert snapshot() == before, 'failed preview wrote state'
            state['checksum_error'] = 'duplicate'
            error = json.loads(run(command+['--plan'], env=env, cwd=project, expected=1))
            assert error['error']['code'] == 'PINSET_UPDATE_CHECKSUM'
            assert snapshot() == before
            state['checksum_error'] = 'corrupt'
            run(command, env=env, cwd=project, expected=1, timeout=120)
            assert run([bin_dir/'pinset', '--version'], env=env).strip() == 'pinset '+(current_version if index == 0 else versions[0])
            state['checksum_error'] = None
            result = json.loads(run(command, env=env, cwd=project, timeout=120))
            assert result['updated'] == version and result['restart_required']
            assert run([bin_dir/'pinset', '--version'], env=env).strip() == 'pinset '+version
            assert (home/'v3/state/shims.json').is_file()
            assert not (home/'v3/state/self-update/transaction.json').exists()
            assert (home/'v3/state/shims.json').read_text().find(hashlib.sha256((bin_dir/'pinset-shim').read_bytes()).hexdigest()) >= 0
            before = snapshot()
            first_request = len(state['requests'])
            current = json.loads(run(command, env=env, cwd=project))
            assert current['already_current'] and snapshot() == before
            assert not any(path.endswith('.zip') for _, _, path in state['requests'][first_request:]), 'same version downloaded binaries'
        state['latest'] = versions[0]
        before = snapshot()
        error = json.loads(run(command+['--plan'], env=env, cwd=project, expected=1))
        assert error['error']['code'] == 'PINSET_UPDATE_METADATA' and snapshot() == before
        assert not any(host == 'api.github.com' for _, host, _ in state['requests']), 'self update called the blocked REST API'
    finally:
        server.shutdown()
        server.server_close()
    return {'from': current_version, 'updated_to': versions, 'asset_names': [artifacts[v].name for v in versions],
            'preview': 'read-only; no binary download', 'integrity_failure': 'original binaries retained', 'json': 'one parseable document',
            'rest_api': 'blocked; zero requests', 'release_switch': 'version pinned after HEAD',
            'redirects': 'official CDN accepted; foreign/downgraded/pre-release rejected',
            'network_errors': 'access, rate limit, not found and missing platform distinguished; bounded retries',
            'same_version': 'no binary download or local writes', 'stale_latest': 'automatic downgrade rejected'}
