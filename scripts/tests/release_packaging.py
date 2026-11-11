"""Offline packaging regressions: never execute the fixture binaries."""
import importlib.util
import json
from pathlib import Path
import tempfile
from unittest.mock import patch
import zipfile


def verify_packaging():
    source = Path(__file__).resolve().parents[1] / 'package_release.py'
    spec = importlib.util.spec_from_file_location('pinset_release_packaging', source)
    packaging = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(packaging)
    metadata = {'packages': [
        {'name': 'pinset-cli', 'version': '3.1.1', 'license': 'MIT'},
        {'name': 'patched-native', 'version': '1.0.0', 'source': 'git+https://example.invalid/source#abc'},
    ]}
    checks = []
    with tempfile.TemporaryDirectory(prefix='pinset-packaging-') as temporary:
        root = Path(temporary)
        for target in packaging.TARGETS:
            case = root / target
            directory = case / 'target'
            if target == 'windows-x86_64':
                directory /= 'x86_64-pc-windows-msvc'
            directory /= 'release'
            directory.mkdir(parents=True)
            suffix = '.exe' if target.startswith('windows-') else ''
            expected = {name + suffix: (target + ':' + name).encode() for name in ('pinset', 'pinset-shim')}
            for name, content in expected.items():
                (directory / name).write_bytes(content)
            if suffix:
                # A previous default-target build must never leak into the ZIP.
                stale = case / 'target/release'
                stale.mkdir()
                for name in expected:
                    (stale / name).write_bytes(b'stale dynamic CRT build')
            with patch.object(packaging.subprocess, 'check_output', return_value=json.dumps(metadata)) as cargo:
                archive = packaging.package(case, target, 'v3.1.1')
            cargo.assert_called_once_with(
                ['cargo', 'metadata', '--format-version', '1', '--locked'], cwd=case, text=True)
            with zipfile.ZipFile(archive) as output:
                assert {name: output.read(name) for name in output.namelist()} == expected
                assert all(info.external_attr >> 16 == 0o100755 for info in output.infolist())
            sbom = json.loads((case / f'dist/pinset-{target}.sbom.json').read_text())
            assert sbom['metadata']['component']['version'] == '3.1.1'
            assert sbom['components'][1]['properties'] == [{'name': 'cargo:source', 'value': metadata['packages'][1]['source']}]
        checks.append('all four archives contain the matching binary pair; Windows ignores stale target/release binaries')

        for custom in ('relative-cache', str(root / 'absolute-cache')):
            case = root / ('custom-relative' if custom == 'relative-cache' else 'custom-absolute')
            case.mkdir()
            directory = Path(custom)
            if not directory.is_absolute():
                directory = case / directory
            directory /= 'x86_64-pc-windows-msvc/release'
            directory.mkdir(parents=True)
            for name in ('pinset.exe', 'pinset-shim.exe'):
                (directory / name).write_bytes(name.encode())
            with patch.object(packaging.subprocess, 'check_output', return_value=json.dumps(metadata)):
                archive = packaging.package(case, 'windows-x86_64', '3.1.1', custom)
            with zipfile.ZipFile(archive) as output:
                assert output.read('pinset-shim.exe') == b'pinset-shim.exe'
        checks.append('relative and absolute CARGO_TARGET_DIR use the explicit MSVC target directory')

        for absent in ('pinset.exe', 'pinset-shim.exe'):
            for empty in (False, True):
                case = root / f'invalid-{absent}-{empty}'
                directory = case / 'target/x86_64-pc-windows-msvc/release'
                directory.mkdir(parents=True)
                for name in ('pinset.exe', 'pinset-shim.exe'):
                    if name != absent or empty:
                        (directory / name).write_bytes(b'' if name == absent else b'fixture')
                try:
                    packaging.package(case, 'windows-x86_64', '3.1.1')
                except RuntimeError as error:
                    assert absent in str(error)
                else:
                    raise AssertionError('packaging accepted an incomplete binary pair')
                assert not (case / 'dist').exists(), 'missing binary left partial release assets'
        checks.append('either missing or empty executable fails before writing release assets')
    return {'passed': True, 'checks': checks, 'scope': 'offline fixture packaging; no native executable validation'}


if __name__ == '__main__':
    print(json.dumps(verify_packaging(), indent=2))
