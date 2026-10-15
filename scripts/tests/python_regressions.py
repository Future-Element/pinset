"""Project Python binding drift, versioned entries and package isolation."""
import json, os, shutil, tempfile, tomllib, uuid
from pathlib import Path
from support import data, native, report


def verify_python_regressions(first, second):
    projects = [Path(first), Path(second)]
    route = data(projects[0], 'which', 'python', '--explain')
    sdk = Path(route['sdk'])
    locked_version = route['version'].split('+', 1)[0]
    major_minor = '.'.join(locked_version.split('.')[:2])
    minor = locked_version.split('.')[1]
    python_alias = 'python' + major_minor
    pip_alias = 'pip' + major_minor
    original_python = projects[0] / '.venv/bin/python'
    assert original_python.is_symlink(), 'Unix venv must link the selected interpreter'
    original_target = os.readlink(original_python)
    cfg = projects[0] / '.venv/pyvenv.cfg'
    original_cfg = cfg.read_bytes()
    marker = projects[0] / '.venv/.pinset-owner.toml'
    original_marker = marker.read_bytes()
    marker_owner = tomllib.loads(original_marker.decode())

    # The ordinary acceptance flow retains its previous 3.12 environment. Reuse
    # that SDK; this regression suite never resolves or downloads another one.
    prior = next(
        owner for owner in (projects[0] / '.pinset/local/venv-backups').glob('*/.pinset-owner.toml')
        if tomllib.loads(owner.read_text())['interpreter_identity'] != marker_owner['interpreter_identity']
    )
    prior_owner = tomllib.loads(prior.read_text())
    home = Path(os.environ['PINSET_HOME']) / 'v3'
    older_sdk = home / 'installs/python' / prior_owner['interpreter_identity'] / prior_owner['platform']
    assert (older_sdk / 'bin/python3').is_file()
    assert older_sdk != sdk
    drift_cases = []

    def reset():
        cfg.write_bytes(original_cfg)
        marker.write_bytes(original_marker)
        original_python.unlink(missing_ok=True)
        original_python.symlink_to(original_target)

    def reject_drift(name):
        changed_cfg = cfg.read_bytes()
        changed_target = os.readlink(original_python)
        assert marker.read_bytes() == original_marker, 'fixture changed ownership rather than actual interpreter'
        for arguments in [('which', 'python', '--explain'), ('install', '--offline')]:
            result = data(projects[0], *arguments, expected=1)
            assert result['error']['code'] == 'PINSET_VENV_DAMAGED', result
            assert cfg.read_bytes() == changed_cfg and os.readlink(original_python) == changed_target
            assert marker.read_bytes() == original_marker
            assert not (projects[0] / '.pinset/local/venv-transaction.json').exists()
        drift_cases.append(name)

    try:
        cfg.write_text(original_cfg.decode().replace(str(sdk), str(older_sdk)))
        reject_drift('configuration points to another Python version')
        reset()
        original_python.unlink()
        original_python.symlink_to(older_sdk / 'bin/python3')
        reject_drift('interpreter link points to another Python version')
        reset()
        with tempfile.TemporaryDirectory(prefix='pinset-python-clone-') as temporary:
            clone = Path(temporary) / 'sdk'
            shutil.copytree(sdk, clone, symlinks=True)
            cfg.write_text(original_cfg.decode().replace(str(sdk), str(clone)))
            original_python.unlink()
            original_python.symlink_to(clone / 'bin/python3')
            reject_drift('same Python version belongs to another SDK installation')
            reset()
        cfg.write_bytes(original_cfg + b'\ninclude-system-site-packages = true\n')
        reject_drift('duplicate package isolation key cannot enable inheritance')
    finally:
        reset()

    info_script = (
        'import sys,site,json; print(json.dumps({'
        '"version":sys.version.split()[0],"executable":sys.executable,'
        '"prefix":sys.prefix,"base_prefix":sys.base_prefix,'
        '"base_executable":sys._base_executable,"user_site":site.ENABLE_USER_SITE}))'
    )

    def assert_info(root, observed, expected_sdk):
        assert observed['version'] == locked_version, observed
        assert Path(observed['executable']) == root / '.venv/bin/python', observed
        assert Path(observed['prefix']).resolve() == (root / '.venv').resolve(), observed
        assert Path(observed['base_prefix']).resolve() == expected_sdk.resolve(), observed
        assert Path(observed['base_executable']).resolve() == (expected_sdk / 'bin/python3').resolve(), observed
        assert observed['user_site'] is False, observed

    probe_results = []
    for root in projects:
        selected = data(root, 'which', 'python', '--explain')
        assert selected['version'].split('+', 1)[0] == locked_version
        expected_sdk = Path(selected['sdk'])
        alias_route = data(root, 'which', python_alias, '--explain')
        assert alias_route['tool'] == 'python' and alias_route['executable'] == selected['executable']
        assert_info(root, json.loads(native(root, python_alias, '-B', '-c', info_script)), expected_sdk)
        pip_route = data(root, 'which', pip_alias, '--explain')
        assert pip_route['tool'] == 'python' and pip_route['prefix'] == ['-m', 'pip']
        assert str(root / '.venv') in native(root, pip_alias, '--version')
        wrong_minor = '12' if minor != '12' else '13'
        for mismatched_alias in ['python3.' + wrong_minor, 'pip3.' + wrong_minor]:
            result = data(root, 'which', mismatched_alias, '--explain', expected=1)
            assert result['error']['code'] == 'PINSET_COMMAND_UNAVAILABLE', result
        result = data(root, 'check', '--probe')
        check = next(item for item in result['report']['checks'] if item['tool'] == 'python')
        assert check['actually_verified'], result
        evidence = next(item for item in result['report']['evidence'] if item['command'] == 'python')
        assert evidence['success'], evidence
        observed = json.loads(evidence['observed'])
        assert_info(root, observed, expected_sdk)
        probe_results.append({'project': str(root), 'sdk': str(expected_sdk), 'observed': observed})

    # Place one project-only module directly in the new environment. -B and
    # dont_write_bytecode keep the fixture removable without touching SDK files.
    package_name = 'pinset_versioned_isolation_' + uuid.uuid4().hex
    purelib = Path(native(projects[0], 'python', '-B', '-c',
                          'import sysconfig; print(sysconfig.get_path("purelib"))').strip())
    assert purelib.resolve().is_relative_to((projects[0] / '.venv').resolve())
    module = purelib / (package_name + '.py')
    module.write_text('VALUE = 42\n')
    scripts = []
    try:
        assert native(projects[0], python_alias, '-B', '-c',
                      f'import {package_name}; print({package_name}.VALUE)').strip() == '42'
        native(projects[1], python_alias, '-B', '-c', f'import {package_name}', expected=1)
        for index, root in enumerate(projects):
            script = root / ('versioned-environment-' + uuid.uuid4().hex + '.py')
            scripts.append(script)
            script.write_text(
                '#!/usr/bin/env ' + python_alias + '\n'
                'import sys\nsys.dont_write_bytecode = True\nimport json\n'
                'try:\n    import ' + package_name + '\n    value = ' + package_name + '.VALUE\n'
                'except ModuleNotFoundError:\n    value = None\n'
                'print(json.dumps({"prefix":sys.prefix,"base_prefix":sys.base_prefix,"value":value}))\n'
            )
            script.chmod(0o755)
            observed = json.loads(native(root, 'sh', '-c', './' + script.name))
            assert Path(observed['prefix']).resolve() == (root / '.venv').resolve(), observed
            assert Path(observed['base_prefix']).resolve() == Path(data(root, 'which', 'python')['sdk']).resolve(), observed
            assert observed['value'] == (42 if index == 0 else None), observed
    finally:
        module.unlink(missing_ok=True)
        for script in scripts:
            script.unlink(missing_ok=True)
    assert marker.read_bytes() == original_marker and cfg.read_bytes() == original_cfg
    report('python-regressions', status='passed', drift_rejected=drift_cases,
           aliases=[python_alias, pip_alias], probes=probe_results,
           packages='versioned entry and env shebang keep project packages isolated',
           scope='existing Linux SDKs; temporary same-version clone; no downloads')


if __name__ == '__main__':
    import sys
    assert len(sys.argv) == 3, 'provide the ordinary Python acceptance project A and B'
    verify_python_regressions(Path(sys.argv[1]), Path(sys.argv[2]))
