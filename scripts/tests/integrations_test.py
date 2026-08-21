#!/usr/bin/env python3
"""Static contract checks for v2.0 editor, CI, container, and distribution assets."""

from __future__ import annotations

import json
import pathlib
import re
import subprocess
import sys
import tempfile
import tomllib


ROOT = pathlib.Path(__file__).resolve().parents[2]
WORKSPACE_VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"][
    "package"
]["version"]
ARCHIVES = (
    "pinset-linux-x86_64.tar.gz",
    "pinset-linux-aarch64.tar.gz",
    "pinset-macos-aarch64.tar.gz",
    "pinset-windows-x86_64.zip",
)


def require_text(path: pathlib.Path, values: tuple[str, ...]) -> None:
    content = path.read_text(encoding="utf-8")
    display = path.relative_to(ROOT) if path.is_relative_to(ROOT) else path
    for value in values:
        if value not in content:
            raise AssertionError(f"{display} is missing {value!r}")


for schema_name in ("pinset.schema.json", "pinset-lock.schema.json"):
    schema = json.loads((ROOT / "schemas" / schema_name).read_text(encoding="utf-8"))
    assert schema["$schema"] == "https://json-schema.org/draft/2020-12/schema"
    assert schema["additionalProperties"] is False

devcontainer = json.loads(
    (ROOT / "examples/devcontainer/.devcontainer/devcontainer.json").read_text(encoding="utf-8")
)
assert devcontainer["build"]["args"]["PINSET_VERSION"] == WORKSPACE_VERSION

require_text(
    ROOT / "action.yml",
    (
        "using: composite",
        "SHA256SUMS",
        "sha256sum",
        "Get-FileHash",
        "pinset install --locked",
        "trust-project-id",
        "pinset trust add --project-id",
    ),
)
action = (ROOT / "action.yml").read_text(encoding="utf-8")
action_version = re.search(r"(?ms)^  version:\s*$.*?^    default: ([^\s]+)$", action)
assert action_version and action_version.group(1) == WORKSPACE_VERSION
require_text(
    ROOT / "integrations/renovate/pinset.json5",
    ("customType: \"regex\"", "datasource", "depName", "currentValue"),
)
require_text(
    ROOT / ".github/workflows/release.yml",
    (
        "generate_package_manifests.py",
        "dist/pinset-winget.yaml",
        "dist/pinset-scoop.json",
        "dist/pinset.rb",
        "dist/install.ps1",
        "dist/pinset-env.cdx.json",
    ),
)
require_text(
    ROOT / "examples/devcontainer/.devcontainer/Dockerfile",
    (f"ARG PINSET_VERSION={WORKSPACE_VERSION}", "SHA256SUMS", "sha256sum"),
)

install_ps1 = (ROOT / "install.ps1").read_text(encoding="utf-8")
assert f"[string] $Version = '{WORKSPACE_VERSION}'" in install_ps1
for required in ("Get-FileHash", "SHA256SUMS", "pinset-shim.exe", "shim install --all"):
    assert required in install_ps1

with tempfile.TemporaryDirectory() as temporary:
    temporary_path = pathlib.Path(temporary)
    checksums = temporary_path / "SHA256SUMS"
    checksums.write_text(
        "".join(f"{'ab' * 32}  {archive}\n" for archive in ARCHIVES), encoding="utf-8"
    )
    output = temporary_path / "manifests"
    subprocess.run(
        [
            sys.executable,
            str(ROOT / "scripts/generate_package_manifests.py"),
            "--version",
            WORKSPACE_VERSION,
            "--checksums",
            str(checksums),
            "--output",
            str(output),
        ],
        check=True,
    )
    scoop = json.loads((output / "pinset-scoop.json").read_text(encoding="utf-8"))
    assert scoop["version"] == WORKSPACE_VERSION
    assert scoop["architecture"]["64bit"]["hash"] == "ab" * 32
    require_text(
        output / "pinset-winget.yaml", (f"PackageVersion: {WORKSPACE_VERSION}", "AB" * 32)
    )
    require_text(
        output / "pinset.rb",
        (f'version "{WORKSPACE_VERSION}"', 'sha256 "' + "ab" * 32 + '"'),
    )

print("v2.0 integration contracts passed")
