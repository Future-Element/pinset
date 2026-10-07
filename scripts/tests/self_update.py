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

    state = {'latest': versions[0], 'requests': [], 'checksum_error': None}

    class Origin(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass
        def do_GET(self):
            host = self.headers['Host'].split(':')[0]
            state['requests'].append((host, self.path))
            version = state['latest']
            if host == 'api.github.com':
                if self.path.startswith('/repos/Future-Element/pinset/releases/tags/v'):
                    version = self.path.rsplit('/v', 1)[1]
                elif self.path != '/repos/Future-Element/pinset/releases/latest':
                    self.send_error(404)
                    return
                if version not in artifacts:
                    self.send_error(404)
                    return
                archive = artifacts[version]
                base = f'https://github.com/Future-Element/pinset/releases/download/v{version}/'
                body = json.dumps({'tag_name': 'v'+version, 'draft': False, 'prerelease': False, 'assets': [
                    {'name': archive.name, 'browser_download_url': base+archive.name, 'state': 'uploaded', 'size': archive.stat().st_size},
                    {'name': 'SHA256SUMS', 'browser_download_url': base+'SHA256SUMS', 'state': 'uploaded', 'size': 100}]}).encode()
            else:
                prefix = '/Future-Element/pinset/releases/download/v'
                if host != 'github.com' or not self.path.startswith(prefix):
                    self.send_error(404)
                    return
                version, name = self.path[len(prefix):].split('/', 1)
                archive = artifacts[version]
                if name == 'SHA256SUMS':
                    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
                    body = (digest+'  '+archive.name+'\n').encode()
                    if state['checksum_error'] == 'duplicate':
                        body *= 2
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

    server, proxy_env = create_https_fixture(fixture, ['github.com','api.github.com'], Origin)
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
            assert not any(path.endswith('.zip') for _, path in state['requests'][first_request:]), 'preview downloaded a binary'
            explicit = json.loads(run(command+[version, '--plan'], env=env, cwd=project))
            assert explicit == plan
            assert ('api.github.com', '/repos/Future-Element/pinset/releases/tags/v'+version) in state['requests']
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
    finally:
        server.shutdown()
        server.server_close()
    return {'from': current_version, 'updated_to': versions, 'asset_names': [artifacts[v].name for v in versions],
            'preview': 'read-only; no binary download', 'integrity_failure': 'original binaries retained', 'json': 'one parseable document'}
