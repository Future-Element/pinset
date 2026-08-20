#!/usr/bin/env python3
"""Generate package-manager manifests from verified Pinset release archive hashes."""

from __future__ import annotations

import argparse
import json
import pathlib
import re


REPOSITORY = "https://github.com/Future-Element/pinset"
REQUIRED_ARCHIVES = (
    "pinset-linux-x86_64.tar.gz",
    "pinset-linux-aarch64.tar.gz",
    "pinset-macos-aarch64.tar.gz",
    "pinset-windows-x86_64.zip",
)


def read_checksums(path: pathlib.Path) -> dict[str, str]:
    checksums: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        match = re.fullmatch(r"([0-9a-fA-F]{64})\s+\*?(.+)", line.strip())
        if match:
            checksums[match.group(2)] = match.group(1).lower()
    missing = [archive for archive in REQUIRED_ARCHIVES if archive not in checksums]
    if missing:
        raise SystemExit(f"missing release checksums: {', '.join(missing)}")
    return checksums


def release_url(version: str, archive: str) -> str:
    return f"{REPOSITORY}/releases/download/v{version}/{archive}"


def winget(version: str, checksums: dict[str, str]) -> str:
    archive = "pinset-windows-x86_64.zip"
    return f"""# yaml-language-server: $schema=https://aka.ms/winget-manifest.singleton.1.12.0.schema.json
PackageIdentifier: FutureElement.Pinset
PackageVersion: {version}
PackageLocale: en-US
Publisher: Future Element
PackageName: Pinset
License: MIT
ShortDescription: Predictable runtime version management for multilingual projects
PackageUrl: {REPOSITORY}
InstallerType: zip
NestedInstallerType: portable
Installers:
  - Architecture: x64
    InstallerUrl: {release_url(version, archive)}
    InstallerSha256: {checksums[archive].upper()}
    NestedInstallerFiles:
      - RelativeFilePath: pinset.exe
        PortableCommandAlias: pinset
      - RelativeFilePath: pinset-shim.exe
        PortableCommandAlias: pinset-shim
ManifestType: singleton
ManifestVersion: 1.12.0
"""


def scoop(version: str, checksums: dict[str, str]) -> str:
    archive = "pinset-windows-x86_64.zip"
    manifest = {
        "version": version,
        "description": "Predictable runtime version management for multilingual projects",
        "homepage": REPOSITORY,
        "license": "MIT",
        "architecture": {
            "64bit": {
                "url": release_url(version, archive),
                "hash": checksums[archive],
            }
        },
        "bin": ["pinset.exe", "pinset-shim.exe"],
        "checkver": {"github": REPOSITORY},
        "autoupdate": {
            "architecture": {
                "64bit": {
                    "url": f"{REPOSITORY}/releases/download/v$version/{archive}"
                }
            }
        },
    }
    return json.dumps(manifest, indent=2) + "\n"


def homebrew(version: str, checksums: dict[str, str]) -> str:
    linux_x64 = "pinset-linux-x86_64.tar.gz"
    linux_arm64 = "pinset-linux-aarch64.tar.gz"
    macos_arm64 = "pinset-macos-aarch64.tar.gz"
    return f'''class Pinset < Formula
  desc "Predictable runtime version management for multilingual projects"
  homepage "{REPOSITORY}"
  version "{version}"
  license "MIT"

  on_macos do
    depends_on arch: :arm64
    url "{release_url(version, macos_arm64)}"
    sha256 "{checksums[macos_arm64]}"
  end

  on_linux do
    if Hardware::CPU.arm?
      url "{release_url(version, linux_arm64)}"
      sha256 "{checksums[linux_arm64]}"
    else
      url "{release_url(version, linux_x64)}"
      sha256 "{checksums[linux_x64]}"
    end
  end

  def install
    bin.install "pinset", "pinset-shim"
  end

  test do
    assert_match version.to_s, shell_output("#{{bin}}/pinset --version")
  end
end
'''


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--checksums", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:-rc\.[0-9]+)?", args.version):
        raise SystemExit(f"invalid release version: {args.version}")
    checksums = read_checksums(args.checksums)
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / "pinset-winget.yaml").write_text(
        winget(args.version, checksums), encoding="utf-8", newline="\n"
    )
    (args.output / "pinset-scoop.json").write_text(
        scoop(args.version, checksums), encoding="utf-8", newline="\n"
    )
    (args.output / "pinset.rb").write_text(
        homebrew(args.version, checksums), encoding="utf-8", newline="\n"
    )


if __name__ == "__main__":
    main()
