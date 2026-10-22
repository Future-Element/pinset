"""Real native CLI fixtures over verified HTTPS; no source builds inside the Provider."""
import hashlib, http.server, io, json, os, tarfile, tempfile, tomllib, zipfile
from pathlib import Path
from urllib.parse import urlsplit
from https_fixture import create_https_fixture
from support import BIN, cli, run, report


def verify_generic_tools():
    fixture = Path(tempfile.mkdtemp(prefix='pinset-generic-'))
    project = fixture / 'project'; project.mkdir()
    home = fixture / 'home'
    payloads = {}
    for version in ['1.0.0', '2.0.0']:
        source = fixture / ('native-' + version + '.c')
        source.write_text('#include <stdio.h>\n#include <string.h>\n#include <stdlib.h>\nint main(int n,char **v){if(n>1 && !strcmp(v[1],"--exit"))return 37;const char *reported=getenv("PINSET_FIXTURE_VERSION");puts(reported?reported:"fixture-tool ' + version + '");return getenv("PINSET_FIXTURE_VERSION_EXIT")?23:0;}\n')
        executable = fixture / ('native-' + version)
        run(['cc', '-o', executable, source])
        payloads[version] = executable.read_bytes()

    assets = {}
    for version, binary in payloads.items():
        packed = io.BytesIO()
        with zipfile.ZipFile(packed, 'w', zipfile.ZIP_DEFLATED) as archive:
            archive.writestr('package/bin/real', binary)
            archive.writestr('install.sh', '#!/bin/sh\ntouch SHOULD_NOT_RUN\n')
        assets[f'/{version}/zipcli_linux_amd64.zip'] = packed.getvalue()
        packed = io.BytesIO()
        with tarfile.open(fileobj=packed, mode='w:gz') as archive:
            entry = tarfile.TarInfo(f'tool-{version}/bin/real'); entry.size = len(binary); entry.mode = 0o644
            archive.addfile(entry, io.BytesIO(binary))
        assets[f'/{version}/tarcli_linux_amd64.tar.gz'] = packed.getvalue()
        assets[f'/{version}/rawcli_linux_amd64'] = binary
    assets['/script'] = b'#!/bin/sh\ntouch SHOULD_NOT_RUN\n' * 8
    requests = []
    state = {'offline': False}

    class Origin(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_): pass
        def do_GET(self):
            path = urlsplit(self.path).path
            requests.append((self.headers.get('Host'), self.path))
            if state['offline']:
                self.send_error(503); return
            if path == '/redirect':
                self.send_response(302)
                self.send_header('Location', 'https://release-assets.githubusercontent.com/1.0.0/zipcli_linux_amd64.zip?signature=fixture')
                self.send_header('Content-Length', '0'); self.end_headers(); return
            if path == '/downgrade':
                self.send_response(302); self.send_header('Location', 'http://downloads.pinset.example/script')
                self.send_header('Content-Length', '0'); self.end_headers(); return
            if path not in assets:
                self.send_error(404); return
            body = assets[path]
            self.send_response(200); self.send_header('Content-Length', str(len(body)))
            self.send_header('Connection', 'close'); self.end_headers(); self.wfile.write(body)

    server, proxy = create_https_fixture(fixture, ['downloads.pinset.example', 'github.com', 'release-assets.githubusercontent.com'], Origin)
    env = {**os.environ, **proxy, 'PINSET_HOME': str(home), 'HOME': str(fixture)}
    for key in ['PINSET_IDENTITY', 'PINSET_PROFILE', 'PINSET_NO_ENV', 'PINSET_ENV_RESOLVED']:
        env.pop(key, None)

    def invoke(root, *args, expected=0, native=False, environment=None):
        output = cli(root, '--lang', 'en', *args, expected=expected, native=native, env=environment or env)
        return output if native else json.loads(output)

    def snapshot(root):
        return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest() for p in root.rglob('*') if p.is_file()}

    def define(root, name, url, format, executable):
        path = root / '.pinset/config.toml'
        with path.open('a') as stream:
            stream.write(f'\n[generic.{name}]\nurl = {json.dumps(url)}\nformat = {json.dumps(format)}\nexecutable = {json.dumps(executable)}\n')
            stream.write('os = { macos = "darwin" }\narch = { x86_64 = "amd64", aarch64 = "arm64" }\n')

    try:
        invoke(project, 'init')
        for name, format, suffix, executable in [
            ('zipcli', 'zip', '.zip', 'package/bin/real'),
            ('tarcli', 'tar.gz', '.tar.gz', 'tool-{version}/bin/real'),
            ('rawcli', 'binary', '', 'custom/bin/rawcli{exe}'),
        ]:
            define(project, name, f'https://downloads.pinset.example/{{version}}/{name}_{{os}}_{{arch}}{suffix}', format, executable)
        initial = (project / '.pinset/config.toml').read_text()
        before_project, before_home = snapshot(project), snapshot(home)
        plan = invoke(project, 'use', 'zipcli@1.0.0', 'tarcli@1.0.0', 'rawcli@1.0.0', '--plan')
        assert plan['plan'] is True and len(plan['lock']['tool']) == 3
        assert snapshot(project) == before_project and snapshot(home) == before_home, 'generic preview wrote managed state'
        invoke(project, 'use', 'zipcli@1.0.0', 'tarcli@1.0.0', 'rawcli@1.0.0', '--no-install')
        lock_path = project / '.pinset/lock.toml'
        lock_bytes = lock_path.read_bytes()
        lock = tomllib.loads(lock_bytes.decode())
        assert not (home / 'v3/installs').exists()
        for tool in lock['tool']:
            artifact = tool['artifact'][0]
            assert tool['provider'] == 'generic-binary' and tool['requested'] == tool['version'] == '1.0.0'
            assert artifact['sha256'] == hashlib.sha256(assets[urlsplit(artifact['canonical_url']).path]).hexdigest()

        # Both URL and executable template edits must be ignored by install until a new use.
        config_path = project / '.pinset/config.toml'
        config_path.write_text(initial.replace('https://downloads.pinset.example/{version}/zipcli_',
                                              'https://different.pinset.example/never/{version}/zipcli_')
                                      .replace('executable = "package/bin/real"', 'executable = "new/path/real"')
                                      .replace('[tools]', '[tools]\nzipcli = "1.0.0"\ntarcli = "1.0.0"\nrawcli = "1.0.0"'))
        zip_tool = next(tool for tool in lock['tool'] if tool['name'] == 'zipcli')
        cache = home / 'v3/cache/downloads/sha256' / (zip_tool['artifact'][0]['sha256'] + '.archive')
        assert cache.is_file(); cache.unlink()
        count = len(requests)
        invoke(project, 'install', 'zipcli')
        assert requests[count:] == [('downloads.pinset.example', '/1.0.0/zipcli_linux_amd64.zip')]
        assert lock_path.read_bytes() == lock_bytes
        assert invoke(project, 'which', 'zipcli', '--explain')['executable'].endswith('/package/bin/real')
        count = len(requests); state['offline'] = True
        invoke(project, 'install', 'tarcli', 'rawcli', '--offline')
        assert len(requests) == count
        for name in ['zipcli', 'tarcli', 'rawcli']:
            route = invoke(project, 'which', name, '--explain')
            assert route['provider'] == 'generic-binary' and route['version'] == '1.0.0'
            assert 'fixture-tool 1.0.0' in invoke(project, 'exec', '--no-env', '--', name, '--version', native=True)
            direct_env = {**env, 'PINSET_NO_ENV': '1'}
            assert 'fixture-tool 1.0.0' in run([home / 'v3/bin' / name, '--version'], cwd=project, env=direct_env)
        invoke(project, 'exec', '--no-env', '--', 'zipcli', '--exit', native=True, expected=37)
        checks = invoke(project, 'check', '--deep', '--probe')['report']['checks']
        assert all(check['installed'] and check['bound'] and check['actually_verified'] for check in checks)
        from check_json import verify_check_json
        check_json = verify_check_json(project, env, requests)
        invoke(project, 'self', 'repair')
        assert 'fixture-tool 1.0.0' in run([home / 'v3/bin/rawcli', '--version'], cwd=project, env={**env, 'PINSET_NO_ENV': '1'})
        assert not (project / 'SHOULD_NOT_RUN').exists()
        assert invoke(project, 'use', 'zipcli@latest', expected=1)['error']['code'] == 'PINSET_GENERIC_INVALID'
        state['offline'] = False

        # Rebuilding from another home must use the lock and reject changed upstream bytes.
        assets['/1.0.0/zipcli_linux_amd64.zip'] += b'tampered'
        fresh = {**env, 'PINSET_HOME': str(fixture / 'tamper-home')}
        pair = (config_path.read_bytes(), lock_path.read_bytes())
        failed = invoke(project, 'install', 'zipcli', expected=1, environment=fresh)
        assert 'integrity mismatch' in failed['error']['message'].lower(), failed
        assert (config_path.read_bytes(), lock_path.read_bytes()) == pair
        assert not list((fixture / 'tamper-home').rglob('.pinset-install.toml'))
        assert invoke(project, 'which', 'zipcli', expected=1, environment=fresh)['error']['code'] == 'PINSET_INSTALL_MISSING'
        assets['/1.0.0/zipcli_linux_amd64.zip'] = assets['/1.0.0/zipcli_linux_amd64.zip'][:-8]

        define(project, 'redirectcli', 'https://github.com/redirect', 'zip', 'package/bin/real')
        invoke(project, 'use', 'redirectcli@1.0.0')
        redirected = next(t for t in tomllib.loads(lock_path.read_text())['tool'] if t['name'] == 'redirectcli')
        assert redirected['artifact'][0]['canonical_url'] == 'https://github.com/redirect'
        define(project, 'downgradecli', 'https://github.com/downgrade', 'binary', 'tool')
        assert invoke(project, 'use', 'downgradecli@1.0.0', expected=1)['error']['code'] == 'PINSET_GENERIC_DOWNLOAD_FAILED'
        define(project, 'scriptcli', 'https://downloads.pinset.example/script', 'binary', 'tool.sh')
        assert invoke(project, 'use', 'scriptcli@1.0.0', expected=1)['error']['code'] == 'PINSET_GENERIC_BINARY_INVALID'
        assert not (project / 'SHOULD_NOT_RUN').exists()
        assert 'scriptcli' not in tomllib.loads(config_path.read_text())['tools']

        # A release tag may have a v prefix even when --version omits it.
        define(project, 'tagcli', 'https://downloads.pinset.example/1.0.0/rawcli_linux_amd64', 'binary', 'tagcli')
        invoke(project, 'use', 'tagcli@v1.0.0')
        tagged = invoke(project, 'check', '--probe')
        tag_check = next(item for item in tagged['report']['checks'] if item['tool'] == 'tagcli')
        assert tagged['passed'] and tag_check['passed'], tagged
        assert tag_check['expected_version'] == 'v1.0.0' and tag_check['actual_version'] == '1.0.0', tag_check

        # Generic global defaults follow the same project override and strict-policy rules.
        global_dir = home / 'v3/global'; global_dir.mkdir(exist_ok=True)
        original_id = tomllib.loads(initial)['project_id']
        (global_dir / 'config.toml').write_text(initial.replace(original_id, 'global'))
        invoke(project, 'use', 'zipcli@2.0.0', '--global')
        child = fixture / 'child'; child.mkdir(); invoke(child, 'init')
        assert invoke(child, 'which', 'zipcli', '--explain')['source'] == 'global'
        assert 'fixture-tool 2.0.0' in invoke(child, 'exec', '--no-env', '--', 'zipcli', '--version', native=True)
        assert 'fixture-tool 1.0.0' in invoke(project, 'exec', '--no-env', '--', 'zipcli', '--version', native=True)
        strict = child / '.pinset/config.toml'
        strict.write_text(strict.read_text().replace('[policy]', '[policy]\nrequire_project_tools = true'))
        assert invoke(child, 'which', 'zipcli', expected=1)['error']['code'] == 'PINSET_TOOL_NOT_SELECTED'
        invoke(project, 'remove', 'rawcli')
        assert invoke(project, 'which', 'rawcli', expected=1)['error']['code'] == 'PINSET_TOOL_NOT_SELECTED'
        assert invoke(project, 'clean', 'installs', 'rawcli@1.0.0', '--plan')['plan'] is True
        (project / '.tool-versions').write_text('rawcli 1.0.0\n')
        before_project, before_home = snapshot(project), snapshot(home)
        migration = invoke(project, 'import', '--plan')
        assert migration['imports'][0]['tool'] == 'rawcli' and migration['imports'][0]['provider'] == 'generic-binary'
        assert snapshot(project) == before_project and snapshot(home) == before_home
        imported = cli(project, '--lang', 'en', 'import', native=True, env=env, stdin='yes\n')
        assert 'Config and lock written' in imported
        assert invoke(project, 'which', 'rawcli', '--explain')['provider'] == 'generic-binary'
        report('generic-provider', formats=['zip', 'tar.gz', 'binary'],
               checks=['read-only resolution preview', 'exact lock hashes', 'lock-only install after template edits',
                       'offline cache', 'partial install', 'CLI and shim routing', 'native exit codes', 'deep probes',
                       'tampered downloads', 'missing installation fail-closed', 'HTTPS redirects', 'HTTP downgrade rejection',
                       'script rejection', 'v-prefixed version probes', 'shim repair', 'global inheritance', 'project override', 'strict project policy', 'remove and clean', 'declared generic import'])
        return {'formats': ['zip', 'tar.gz', 'binary'], 'check_json': check_json, 'status': 'passed'}
    finally:
        server.shutdown()


if __name__ == '__main__':
    verify_generic_tools()
