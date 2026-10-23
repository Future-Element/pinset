"""Production-signed Release artifact, exercised through the actual CLI over TLS.

The CA routes requests to a local fixture; attestation trust remains the real
Sigstore public-good trust root. The fixture never issues a signing certificate.
This acceptance test downloads a pinned 8 MiB archive unless --archive is given.
It is deliberately registered only in acceptance.py, outside the fast contracts.
"""
import argparse
import base64
import copy
import hashlib
import http.server
import json
import os
import tempfile
import tomllib
from pathlib import Path
from urllib.parse import urlsplit

from https_fixture import create_https_fixture
from support import cli, download, report


VERSION = '3.0.9'
ARCHIVE_NAME = f'pinset-v{VERSION}-linux-x86_64.zip'
ARCHIVE_URL = f'https://github.com/Future-Element/pinset/releases/download/v{VERSION}/{ARCHIVE_NAME}'
ARCHIVE_SHA256 = '2bc89cbee5b40191ab37f71914e90c03c5ac304411d52832cbac20ab7fa39de4'
ATTESTATION_PATH = f'/repos/Future-Element/pinset/attestations/sha256:{ARCHIVE_SHA256}'
FIXTURE_BUNDLE = (Path(__file__).resolve().parents[2] / 'crates/pinset-engine/tests/fixtures/'
                  'github-attestation/pinset-v3.0.9.sigstore.json')


def altered_signature(bundle):
    changed = copy.deepcopy(bundle)
    signature = bytearray(base64.b64decode(changed['dsseEnvelope']['signatures'][0]['sig']))
    signature[-1] ^= 1
    changed['dsseEnvelope']['signatures'][0]['sig'] = base64.b64encode(signature).decode()
    return changed


def verify_github_attestations(archive=None):
    archive = Path(archive) if archive else download(ARCHIVE_URL, ARCHIVE_SHA256, ARCHIVE_NAME)
    payload = archive.read_bytes()
    assert hashlib.sha256(payload).hexdigest() == ARCHIVE_SHA256, 'acceptance archive differs from its pinned digest'
    bundle = json.loads(FIXTURE_BUNDLE.read_text())
    tampered = altered_signature(bundle)
    fixture = Path(tempfile.mkdtemp(prefix='pinset-github-attestation-'))
    project = fixture / 'project'
    project.mkdir()
    home = fixture / 'home'
    requests = []
    state = {'mode': 'valid', 'offline': False}

    class Origin(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            host = self.headers.get('Host')
            path = urlsplit(self.path).path
            requests.append((host, self.path))
            if state['offline']:
                self.respond(503, b'fixture offline')
            elif host == 'api.github.com' and path == ATTESTATION_PATH:
                mode = state['mode']
                if mode in ['forbidden', 'rate-limited']:
                    self.respond(403 if mode == 'forbidden' else 429, b'{"message":"denied"}')
                elif mode == 'malformed':
                    self.respond(200, b'not JSON')
                elif mode == 'empty':
                    self.respond(200, b'{"attestations":[]}')
                else:
                    evidence = tampered if mode == 'bad-signature' else bundle
                    self.respond(200, json.dumps({'attestations': [{'bundle': evidence}]}).encode())
            elif (host == 'github.com' and path == urlsplit(ARCHIVE_URL).path
                  or host == 'downloads.pinset.example' and path == '/' + ARCHIVE_NAME):
                self.respond(200, payload)
            else:
                self.respond(404, b'{"message":"unexpected fixture URL"}')

        def respond(self, status, body):
            self.send_response(status)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(body)))
            self.send_header('Connection', 'close')
            # Prevent transient rate-limit retries from extending this fixture.
            self.send_header('Retry-After', '3')
            self.end_headers()
            self.wfile.write(body)

    server, proxy = create_https_fixture(
        fixture, ['github.com', 'api.github.com', 'downloads.pinset.example'], Origin)
    env = {**os.environ, **proxy, 'PINSET_HOME': str(home), 'HOME': str(fixture)}
    for key in ['PINSET_IDENTITY', 'PINSET_PROFILE', 'PINSET_NO_ENV', 'PINSET_ENV_RESOLVED']:
        env.pop(key, None)

    def invoke(root, *args, expected=0, native=False, environment=None, language='en'):
        output = cli(root, '--lang', language, *args, expected=expected, native=native,
                     env=environment or env)
        return output if native else json.loads(output)

    def define(root, name, url):
        with (root / '.pinset/config.toml').open('a') as stream:
            stream.write(f'\n[generic.{name}]\nurl = {json.dumps(url)}\n'
                         'format = "zip"\nexecutable = "pinset"\n')

    def managed_state(root):
        return {name: (root / '.pinset' / name).read_bytes()
                if (root / '.pinset' / name).is_file() else None
                for name in ['config.toml', 'lock.toml']}

    def receipts():
        return {str(path.relative_to(home)): path.read_bytes()
                for path in home.rglob('.pinset-install.toml')}

    def checked(value):
        assert value['protocol'] == 'pinset/3' and value['schema_version'] == 1, value
        checks = {item['tool']: item for item in value['report']['checks']}
        assert set(checks) == {'signedcli', 'unsignedcli'}, value
        return checks

    try:
        invoke(project, 'init')
        define(project, 'signedcli', ARCHIVE_URL)
        define(project, 'unsignedcli', 'https://downloads.pinset.example/' + ARCHIVE_NAME)
        invoke(project, 'use', f'signedcli@{VERSION}', f'unsignedcli@{VERSION}', '--no-install')
        lock_path = project / '.pinset/lock.toml'
        lock_bytes = lock_path.read_bytes()
        lock = tomllib.loads(lock_bytes.decode())
        artifacts = {tool['name']: tool['artifact'][0] for tool in lock['tool']}
        assert all(item['sha256'] == ARCHIVE_SHA256 for item in artifacts.values())
        signed, unsigned = artifacts['signedcli'], artifacts['unsignedcli']
        assert signed['verification'] == 'generic-github-attestation-sha256'
        assert signed['attestation']['repository'] == 'Future-Element/pinset'
        assert json.loads(signed['attestation']['bundle']) == bundle
        assert unsigned['verification'] == 'generic-https-sha256' and 'attestation' not in unsigned
        assert not receipts(), '--no-install created a receipt'
        assert [host for host, _ in requests].count('api.github.com') == 1, requests

        state['offline'] = True
        network_count = len(requests)
        invoke(project, 'install', '--offline')
        verified = invoke(project, 'check', '--deep', '--probe')
        checks = checked(verified)
        assert verified['passed'] is True and len(verified['report']['evidence']) == 2, verified
        assert checks['signedcli']['source_verification'] == {
            'status': 'signature-verified', 'method': 'generic-github-attestation-sha256',
            'repository': 'Future-Element/pinset', 'passed': True}, checks['signedcli']
        assert checks['unsignedcli']['source_verification'] == {
            'status': 'digest-only', 'method': 'generic-https-sha256',
            'repository': None, 'passed': True}, checks['unsignedcli']
        for item in checks.values():
            assert item['passed'] and item['installed'] and item['bound'] and item['actually_verified'], item
            assert item['actual_version'] == VERSION, item
            assert item['artifact_digest']['payload_verified'] is True, item
        text = invoke(project, 'check', native=True)
        assert 'signature verified' in text and 'digest pinned only' in text, text
        chinese = invoke(project, 'check', native=True, language='zh-CN')
        assert '已验签名' in chinese and '仅摘要固定' in chinese, chinese

        # Exercise real CLI operations through each installed generic command.
        # This checks routed native execution beyond version-only probes.
        for name in ['signedcli', 'unsignedcli']:
            child = project / ('from-' + name)
            child.mkdir()
            result = json.loads(invoke(project, 'exec', '--no-env', '--', name,
                                       '-C', str(child), '--json', 'init', native=True))
            assert isinstance(result['project_id'], str) and (child / '.pinset/config.toml').is_file(), result
            listed = json.loads(invoke(project, 'exec', '--no-env', '--', name,
                                       '-C', str(child), '--json', 'list', native=True))
            assert listed['selected']['tool'] == [], listed
            assert {item['tool'] for item in listed['installs']} == {'signedcli', 'unsignedcli'}, listed
        assert len(requests) == network_count, 'offline install/check/exec fetched data'

        # Corrupt a real DSSE signature while preserving valid TOML/JSON/base64.
        original_signature = bundle['dsseEnvelope']['signatures'][0]['sig']
        changed_signature = tampered['dsseEnvelope']['signatures'][0]['sig']
        lock_text = lock_bytes.decode()
        assert lock_text.count(original_signature) == 1
        before_receipts = receipts()
        try:
            lock_path.write_text(lock_text.replace(original_signature, changed_signature, 1))
            result = invoke(project, 'install', 'signedcli', '--offline', expected=1)
            assert result['error']['code'] == 'PINSET_ATTESTATION_INVALID', result
            assert receipts() == before_receipts, 'invalid evidence changed an installed receipt'
            fresh_home = fixture / 'tampered-home'
            result = invoke(project, 'install', 'signedcli', '--offline', expected=1,
                            environment={**env, 'PINSET_HOME': str(fresh_home)})
            assert result['error']['code'] == 'PINSET_ATTESTATION_INVALID', result
            assert not list(fresh_home.rglob('.pinset-install.toml')), 'invalid evidence created a receipt'
            failed = invoke(project, 'check', '--deep', '--probe', expected=1)
            checks = checked(failed)
            assert failed['passed'] is False
            assert checks['signedcli']['source_verification']['status'] == 'invalid', failed
            assert checks['signedcli']['source_verification']['passed'] is False
            assert checks['signedcli']['actual_version'] is None and not checks['signedcli']['actually_verified']
            assert checks['unsignedcli']['passed'] and checks['unsignedcli']['actually_verified']
            assert [item['command'] for item in failed['report']['evidence']] == ['unsignedcli'], failed
            assert len(requests) == network_count, 'tampered locked evidence triggered network access'
        finally:
            lock_path.write_bytes(lock_bytes)
        assert invoke(project, 'check', '--deep', '--probe')['passed'] is True

        state['offline'] = False
        for mode, error in [
            ('forbidden', 'PINSET_ATTESTATION_FETCH_FAILED'),
            ('rate-limited', 'PINSET_ATTESTATION_FETCH_FAILED'),
            ('malformed', 'PINSET_ATTESTATION_FETCH_FAILED'),
            ('bad-signature', 'PINSET_ATTESTATION_INVALID'),
        ]:
            state['mode'] = mode
            rejected = fixture / ('reject-' + mode)
            rejected.mkdir()
            invoke(rejected, 'init')
            define(rejected, 'signedcli', ARCHIVE_URL)
            before = managed_state(rejected)
            result = invoke(rejected, 'use', f'signedcli@{VERSION}', '--no-install', expected=1)
            assert result['error']['code'] == error, result
            assert managed_state(rejected) == before, f'{mode} rewrote selection or lock'
            assert receipts() == before_receipts, f'{mode} created an installation receipt'

        # An explicit empty API response is TOFU for a first selection.
        state['mode'] = 'empty'
        absent = fixture / 'first-selection-no-attestation'
        absent.mkdir()
        invoke(absent, 'init')
        define(absent, 'signedcli', ARCHIVE_URL)
        invoke(absent, 'use', f'signedcli@{VERSION}', '--no-install')
        first = tomllib.loads((absent / '.pinset/lock.toml').read_text())['tool'][0]['artifact'][0]
        assert first['verification'] == 'generic-https-sha256' and 'attestation' not in first, first
        # The same empty response cannot weaken an already signed selection.
        before = managed_state(project)
        downgrade = invoke(project, 'use', f'signedcli@{VERSION}', '--no-install', expected=1)
        assert downgrade['error']['code'] == 'PINSET_VERIFICATION_DOWNGRADE', downgrade
        assert managed_state(project) == before and receipts() == before_receipts
        state['offline'] = True
        count = len(requests)
        assert invoke(project, 'check', '--deep', '--probe')['passed'] is True
        assert len(requests) == count, 'final check accessed the network'
        result = {'status': 'passed', 'archive_sha256': ARCHIVE_SHA256,
                  'production_trust_root': True, 'offline_reverification': True}
        report('github-attestation', **result, checks=[
            'real production-signed release', 'verified TLS fixture', 'locked inline proof',
            'cached offline install', 'deep payload verification and probes',
            'signature and digest JSON/text classifications', 'real routed CLI init/list',
            'tampered signature blocks existing and fresh installs without network',
            'invalid source skips signed executable probe', 'restored proof passes',
            '403/429/malformed response/invalid signature preserve config and lock',
            'explicit absence retains first-use digest pinning', 'signed selection cannot downgrade'])
        return result
    finally:
        server.shutdown()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--archive', type=Path, help='local pinned Pinset 3.0.9 Linux x86_64 ZIP')
    arguments = parser.parse_args()
    verify_github_attestations(arguments.archive)
