"""Stable CI check output, exercised against installed native HTTPS fixtures."""
import hashlib, json, subprocess, time, tomllib
from pathlib import Path
from support import BIN, CASES, report


def verify_check_json(project, environment, requests):
    home = Path(environment['PINSET_HOME'])
    config_path = project / '.pinset/config.toml'
    lock = tomllib.loads((project / '.pinset/lock.toml').read_text())
    expected_digests = {tool['name']: 'sha256:' + tool['artifact'][0]['sha256'] for tool in lock['tool']}
    expected_tools = set(expected_digests)

    def invoke(*args, expected=0, env=None, global_json=False):
        command = [str(BIN), '-C', str(project), '--lang', 'en']
        command += ['--json', 'check'] if global_json else ['check', '--format', 'json']
        command += list(args)
        started = time.monotonic()
        result = subprocess.run(command, cwd=project, env=env or environment,
                                capture_output=True, text=True, timeout=45)
        CASES.append({'command': command, 'cwd': str(project), 'exit': result.returncode,
                      'seconds': round(time.monotonic() - started, 3)})
        assert result.returncode == expected, (command, result.returncode, result.stdout, result.stderr)
        assert result.stderr == '', ('JSON check emitted terminal output', result.stderr)
        value = json.loads(result.stdout)  # Rejects multiple documents or terminal text on stdout.
        assert value['protocol'] == 'pinset/3' and value['schema_version'] == 1
        assert type(value['passed']) is bool
        return value

    def checks(value):
        items = {item['tool']: item for item in value['report']['checks']}
        assert set(items) == expected_tools, value
        for name, item in items.items():
            assert item['expected_version'] == '1.0.0', item
            assert item['actual_version'] is None or isinstance(item['actual_version'], str), item
            for field in ['configured', 'installed', 'bound', 'actually_verified', 'passed']:
                assert type(item[field]) is bool, (field, item)
            source = item['source_verification']
            assert source['status'] == 'digest-only' and source['passed'] is True, source
            assert source['method'] == 'generic-https-sha256' and source['repository'] is None, source
            digest = item['artifact_digest']
            assert digest['expected'] == expected_digests[name], digest
            assert type(digest['receipt_matches_lock']) is bool and type(digest['passed']) is bool, digest
            assert digest['payload_verified'] is None or type(digest['payload_verified']) is bool, digest
        return items

    def snapshot():
        return [(str(path), path.stat().st_mode, hashlib.sha256(path.read_bytes()).hexdigest())
                for base in [project, home] for path in sorted(base.rglob('*')) if path.is_file()]

    request_count = len(requests)
    before = snapshot()
    default = invoke()
    assert default == invoke(global_json=True), '--format json and --json check diverged'
    assert default['passed'] is True and default['report']['evidence'] == []
    for item in checks(default).values():
        assert item['passed'] and item['actual_version'] is None and not item['actually_verified'], item
        digest = item['artifact_digest']
        assert digest['receipt_matches_lock'] and digest['payload_verified'] is None and digest['passed'], item
    assert snapshot() == before, 'default JSON check wrote files or changed executable modes'

    verified = invoke('--deep', '--probe')
    assert verified['passed'] and len(verified['report']['evidence']) == len(expected_tools), verified
    for item in checks(verified).values():
        assert item['passed'] and item['actually_verified'] and item['actual_version'] == '1.0.0', item
        assert item['artifact_digest']['payload_verified'] is True and item['artifact_digest']['passed'], item

    receipt_path = next(path for path in (home / 'v3/installs').rglob('.pinset-install.toml')
                        if tomllib.loads(path.read_text())['tool'] == 'zipcli')
    receipt_bytes = receipt_path.read_bytes()
    receipt = tomllib.loads(receipt_bytes.decode())
    executable = receipt_path.parent / receipt['commands']['zipcli']
    binary = executable.read_bytes()
    mode = executable.stat().st_mode
    try:
        receipt_path.unlink()
        missing = invoke(expected=1)
        item = checks(missing)['zipcli']
        assert not missing['passed'] and not item['installed'] and not item['bound'] and not item['passed'], item
        assert item['actual_version'] is None and not item['artifact_digest']['receipt_matches_lock'], item
        # Human-readable checks must fail CI in the same way as their JSON equivalent.
        result = subprocess.run([str(BIN), '-C', str(project), '--lang', 'en', 'check'],
                                cwd=project, env=environment, capture_output=True, text=True, timeout=45)
        assert result.returncode == 1 and 'zipcli' in result.stdout and not result.stdout.lstrip().startswith('{'), result
    finally:
        receipt_path.write_bytes(receipt_bytes)

    try:
        receipt_path.write_text(receipt_bytes.decode().replace(receipt['artifact_integrity'], 'sha256:' + '0' * 64, 1))
        mismatch = invoke(expected=1)
        item = checks(mismatch)['zipcli']
        assert not mismatch['passed'] and not item['artifact_digest']['receipt_matches_lock'], item
        assert not item['artifact_digest']['passed'] and item['artifact_digest']['payload_verified'] is None, item
    finally:
        receipt_path.write_bytes(receipt_bytes)

    try:
        # Preserve size and file count so this requires the payload fingerprint.
        executable.write_bytes(binary[:-1] + bytes([binary[-1] ^ 1]))
        changed = invoke('--deep', expected=1)
        item = checks(changed)['zipcli']
        assert not changed['passed'] and not item['installed'] and not item['passed'], item
        assert item['artifact_digest']['receipt_matches_lock'] and item['artifact_digest']['payload_verified'] is False, item
        assert not item['artifact_digest']['passed'] and item['actual_version'] is None, item
    finally:
        executable.write_bytes(binary)
        executable.chmod(mode)

    mismatch = invoke('--probe', expected=1, env={**environment, 'PINSET_FIXTURE_VERSION': 'fixture-tool 2.0.0'})
    assert not mismatch['passed'] and len(mismatch['report']['evidence']) == len(expected_tools), mismatch
    for item in checks(mismatch).values():
        assert item['actual_version'] == '2.0.0' and not item['actually_verified'] and not item['passed'], item
        assert item['artifact_digest']['passed'], item

    nonzero = invoke('--probe', expected=1, env={**environment, 'PINSET_FIXTURE_VERSION_EXIT': '1'})
    assert not nonzero['passed'] and len(nonzero['report']['evidence']) == len(expected_tools), nonzero
    for item in checks(nonzero).values():
        assert item['actual_version'] == '1.0.0' and not item['actually_verified'] and not item['passed'], item

    try:
        executable.chmod(0o644)
        unstartable = invoke('--probe', expected=1)
        items = checks(unstartable)
        assert not unstartable['passed'] and not items['zipcli']['actually_verified'], unstartable
        assert items['zipcli']['actual_version'] is None and not items['zipcli']['passed'], items['zipcli']
        assert all(item['actually_verified'] and item['passed'] for name, item in items.items() if name != 'zipcli'), items
    finally:
        executable.chmod(mode)

    config_bytes = config_path.read_bytes()
    try:
        config_path.write_text(config_bytes.decode() + '\n[unrecognized]\ninvalid = true\n')
        failed = invoke(expected=1)
        assert failed['passed'] is False and isinstance(failed['error']['code'], str), failed
    finally:
        config_path.write_bytes(config_bytes)

    assert invoke('--deep')['passed'] is True, 'check fixture restoration left a damaged install'
    assert len(requests) == request_count, 'check fetched metadata or artifacts'
    report('check-json', schema_version=1, status='passed',
           checks=['format alias', 'single JSON stdout', 'read-only defaults', 'honest unobserved versions',
                   'explicit successful probes', 'missing install', 'receipt digest mismatch', 'same-size payload tamper',
                   'observed version mismatch', 'nonzero version probe', 'unstartable probe isolation',
                   'structured configuration errors', 'text failure exit status', 'no network'])
    return {'schema_version': 1, 'status': 'passed'}
