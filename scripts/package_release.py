"""Packaging and artifact inventory. Deliberately contains no validation commands."""
import json
import os
from pathlib import Path
import subprocess
import zipfile


TARGETS = ('linux-x86_64', 'linux-aarch64', 'windows-x86_64', 'macos-aarch64')
WINDOWS_TARGET = 'x86_64-pc-windows-msvc'


def binary_directory(root, target, cargo_target_dir=None):
    if target not in TARGETS:
        raise ValueError('unsupported release target: ' + target)
    directory = Path(cargo_target_dir or 'target')
    if not directory.is_absolute():
        directory = root / directory
    if target == 'windows-x86_64':
        directory /= WINDOWS_TARGET
    return directory / 'release'


def package(root, target, version, cargo_target_dir=None):
    root = Path(root)
    version = version.removeprefix('v')
    directory = binary_directory(root, target, cargo_target_dir)
    suffix = '.exe' if target == 'windows-x86_64' else ''
    binaries = [(name + suffix, directory / (name + suffix)) for name in ('pinset', 'pinset-shim')]
    for _, path in binaries:
        if not path.is_file() or path.stat().st_size == 0:
            raise RuntimeError('compiled release artifact is missing or empty: ' + str(path))
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--format-version', '1', '--locked'], cwd=root, text=True))
    components = []
    for item in metadata['packages']:
        component = {'type': 'library', 'name': item['name'], 'version': item['version'],
                     'purl': f"pkg:cargo/{item['name']}@{item['version']}",
                     'licenses': [{'expression': item['license']}] if item.get('license') else []}
        if (item.get('source') or '').startswith('git+'):
            component['properties'] = [{'name': 'cargo:source', 'value': item['source']}]
        components.append(component)
    sbom = {'bomFormat': 'CycloneDX', 'specVersion': '1.6', 'version': 1,
            'metadata': {'component': {'type': 'application', 'name': 'pinset', 'version': version}},
            'components': components}
    destination = root / 'dist'
    destination.mkdir(exist_ok=True)
    archive = destination / f'pinset-v{version}-{target}.zip'
    with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as output:
        for name, path in binaries:
            info = zipfile.ZipInfo(name)
            info.external_attr = 0o100755 << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            output.writestr(info, path.read_bytes())
    (destination / f'pinset-{target}.sbom.json').write_text(
        json.dumps(sbom, sort_keys=True, separators=(',', ':')), encoding='utf-8')
    return archive


if __name__ == '__main__':
    package(Path.cwd(), os.environ['PINSET_RELEASE_TARGET'], os.environ['PINSET_RELEASE_VERSION'],
            os.environ.get('CARGO_TARGET_DIR'))
