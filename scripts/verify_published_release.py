#!/usr/bin/env python3
"""Independently verify downloaded release assets; never build release binaries."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile
import time
import zipfile


def run(*arguments, **kwargs):
    return subprocess.check_output(arguments, text=True, **kwargs).strip()


tag, commit, archive, previous = sys.argv[1:]
assert re.fullmatch(r"v\d+\.\d+\.\d+(?:-rc\.[1-9]\d*)?", tag)
assert re.fullmatch(r"[0-9a-f]{40}", commit)
assert re.fullmatch(r"\d+\.\d+\.\d+(?:-rc\.[1-9]\d*)?", previous)
repository = "Future-Element/pinset"
archives = {
    "pinset-linux-x86_64.tar.gz", "pinset-linux-aarch64.tar.gz",
    "pinset-macos-aarch64.tar.gz", "pinset-windows-x86_64.zip",
}
assert archive in archives
sboms = {f"{name}.cdx.json" for name in ("pinset-cli", "pinset-core", "pinset-env", "pinset-shim")}
release = json.loads(run("gh", "api", f"repos/{repository}/releases/tags/{tag}"))
published_assets = {asset["name"] for asset in release["assets"]}
vsix_assets = {name for name in published_assets if re.fullmatch(r"pinset-vscode-\d+\.\d+\.\d+\.vsix", name)}
assert len(vsix_assets) == 1, "release must contain exactly one versioned Pinset VSIX"
extension_asset = next(iter(vsix_assets))
assets = archives | sboms | {
    "install.sh", "install.ps1", "uninstall.sh", "uninstall.ps1",
    "pinset-winget.yaml", "pinset-scoop.json", "pinset.rb", "SHA256SUMS",
    extension_asset,
}
assert release["tag_name"] == tag and not release["draft"]
assert release["prerelease"] == ("-rc." in tag)
assert {asset["name"] for asset in release["assets"]} == assets
ref = json.loads(run("gh", "api", f"repos/{repository}/git/ref/tags/{tag}"))
assert ref["object"]["type"] == "tag", "release requires an annotated signed tag"
signed = json.loads(run("gh", "api", f"repos/{repository}/git/tags/{ref['object']['sha']}"))
assert signed["object"]["sha"] == commit
assert signed["verification"]["verified"], "GitHub did not verify the release tag signature"

with tempfile.TemporaryDirectory(prefix="pinset-published-") as temporary:
    root = Path(temporary)
    downloaded = root / "assets"
    downloaded.mkdir()
    run("gh", "release", "download", tag, "--repo", repository, "--dir", str(downloaded))
    checksums = {}
    for line in (downloaded / "SHA256SUMS").read_text().splitlines():
        digest, name = line.split("  ", 1)
        assert re.fullmatch(r"[0-9a-f]{64}", digest)
        assert name in assets - {"SHA256SUMS"} and name not in checksums
        checksums[name] = digest
        assert hashlib.sha256((downloaded / name).read_bytes()).hexdigest() == digest, name
    assert set(checksums) == assets - {"SHA256SUMS"}
    for name in sboms:
        document = json.loads((downloaded / name).read_text())
        assert document["bomFormat"] == "CycloneDX" and document["specVersion"]
    for name in archives | {"SHA256SUMS", extension_asset}:
        run("gh", "attestation", "verify", str(downloaded / name), "--repo", repository,
            "--source-digest", commit, "--source-ref", f"refs/tags/{tag}",
            "--signer-workflow", f"{repository}/.github/workflows/release.yml")
    for name in archives:
        suffix = ".exe" if name.endswith(".zip") else ""
        expected = {f"pinset{suffix}", f"pinset-shim{suffix}"}
        if suffix:
            with zipfile.ZipFile(downloaded / name) as packed:
                assert set(packed.namelist()) == expected
        else:
            with tarfile.open(downloaded / name) as packed:
                assert set(packed.getnames()) == expected
                assert all(member.isfile() for member in packed.getmembers())
    with zipfile.ZipFile(downloaded / extension_asset) as packed:
        names = set(packed.namelist())
        assert "extension/dist/extension.js" in names
        assert not any(name.startswith(("extension/src/", "extension/test/", "extension/node_modules/")) for name in names)
    print(f"Verified {tag}: signed commit, 17 assets, 16 checksums, four SBOMs, VSIX and provenance")

    suffix = ".exe" if os.name == "nt" else ""
    installed = root / "install with spaces"
    updated = root / "update with spaces"
    environment = os.environ.copy()
    environment.update(PINSET_HOME=str(root / "home"), PINSET_LANG="en")
    for name in ("PINSET_IDENTITY", "PINSET_IDENTITY_FILE", "PINSET_ENV_PROFILE", "PINSET_ENV_DISABLE"):
        environment.pop(name, None)

    def install(version, destination):
        if os.name == "nt":
            run("pwsh", "-NoProfile", "-File", str(downloaded / "install.ps1"),
                "-Version", version, "-InstallDir", str(destination), env=environment)
        else:
            run("sh", str(downloaded / "install.sh"), "--version", version,
                "--install-dir", str(destination), env=environment)

    install(tag[1:], installed)
    binary = installed / f"pinset{suffix}"
    assert run(str(binary), "--version", env=environment) == f"pinset {tag[1:]}"
    assert (installed / f"pinset-shim{suffix}").is_file()
    run(sys.executable, "scripts/tests/environment_wizard_test.py", str(binary), "--keyring", env=environment)
    install(previous, updated)
    binary = updated / f"pinset{suffix}"
    assert run(str(binary), "--version", env=environment) == f"pinset {previous}"
    # Windows replaces the running executable using its background update helper.
    subprocess.run([str(binary), "self", "update", "--version", tag[1:]],
                   env=environment, check=True, timeout=180)
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        try:
            if run(str(binary), "--version", env=environment) == f"pinset {tag[1:]}":
                break
        except (OSError, subprocess.CalledProcessError):
            pass
        time.sleep(1)
    else:
        raise AssertionError("self update did not publish the expected version")
    for name in (f"pinset{suffix}", f"pinset-shim{suffix}"):
        assert (installed / name).read_bytes() == (updated / name).read_bytes(), name
    print(f"Verified native installation and self update from {previous} on {archive}")
