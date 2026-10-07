"""Check uploaded draft asset metadata before making a release public."""
import hashlib
import json
import os
import subprocess
from pathlib import Path


def validate_assets(document, directory, tag):
    if document.get('tag_name') != tag or document.get('draft') is not True:
        raise ValueError('asset completion requires the selected draft release')
    if document.get('prerelease') != ('-' in tag):
        raise ValueError('release stability does not match its tag')
    platforms = ['linux-x86_64', 'linux-aarch64', 'windows-x86_64', 'macos-aarch64']
    expected = {'SHA256SUMS', 'pinset-vscode.vsix', 'local-verification.json'}
    expected.update(f'pinset-{tag}-{platform}.zip' for platform in platforms)
    expected.update(f'pinset-{platform}.sbom.json' for platform in platforms)
    local = {path.name: path for path in directory.iterdir() if path.is_file()}
    if set(local) != expected:
        raise ValueError('local release asset inventory is incomplete or unexpected')
    checksums = {}
    for line in local['SHA256SUMS'].read_text().splitlines():
        fields = line.split()
        if len(fields) != 2:
            raise ValueError('invalid release checksum line')
        digest, name = fields
        name = name.removeprefix('*')
        if name in checksums or len(digest) != 64 or any(c not in '0123456789abcdefABCDEF' for c in digest):
            raise ValueError('duplicate or invalid release checksum')
        checksums[name] = digest.lower()
    if set(checksums) != expected - {'SHA256SUMS'}:
        raise ValueError('release checksums do not cover the exact asset inventory')
    assets = document.get('assets', [])
    if len(assets) != len(expected) or {asset['name'] for asset in assets} != expected:
        raise ValueError('uploaded draft asset inventory is incomplete or duplicated')
    digests = 0
    for asset in assets:
        path = local[asset['name']]
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if path.name != 'SHA256SUMS' and checksums[path.name] != digest:
            raise ValueError('local release asset does not match its checksum')
        if asset.get('state') != 'uploaded' or asset.get('size') != path.stat().st_size or not path.stat().st_size:
            raise ValueError('draft release asset upload is incomplete')
        if asset.get('digest') is not None:
            if asset['digest'] != 'sha256:'+digest:
                raise ValueError('uploaded asset digest differs from the local release asset')
            digests += 1
    return {'tag': tag, 'complete_assets': len(assets), 'server_digests': digests}


if __name__ == '__main__':
    tag = os.environ['RELEASE_TAG']
    repository = os.environ['GITHUB_REPOSITORY']
    document = json.loads(subprocess.check_output(
        ['gh', 'api', f'repos/{repository}/releases/tags/{tag}'], text=True))
    print(json.dumps(validate_assets(document, Path('dist'), tag)))
