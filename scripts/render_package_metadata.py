#!/usr/bin/env python3
"""Render Homebrew and Scoop package definitions from release checksums."""

from __future__ import annotations

import argparse
import re
from pathlib import Path

ASSET_PLACEHOLDERS = {
    "pinset-linux-x86_64.tar.gz": "@LINUX_X86_64_SHA256@",
    "pinset-macos-aarch64.tar.gz": "@MACOS_AARCH64_SHA256@",
    "pinset-windows-x86_64.zip": "@WINDOWS_X86_64_SHA256@",
}
VERSION_PATTERN = re.compile(
    r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)
SHA256_PATTERN = re.compile(r"^[0-9a-fA-F]{64}$")
UNRESOLVED_PLACEHOLDER_PATTERN = re.compile(r"@[A-Z0-9_]+@")


def parse_checksums(path: Path) -> dict[str, str]:
    checksums: dict[str, str] = {}
    for line_number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = raw_line.strip()
        if not line:
            continue
        parts = line.split(maxsplit=1)
        if len(parts) != 2 or not SHA256_PATTERN.fullmatch(parts[0]):
            raise ValueError(f"invalid SHA-256 line {line_number} in {path}")
        filename = Path(parts[1].lstrip("*")).name
        if filename in checksums:
            raise ValueError(f"duplicate checksum for {filename}")
        checksums[filename] = parts[0].lower()

    missing = sorted(set(ASSET_PLACEHOLDERS) - set(checksums))
    if missing:
        raise ValueError(f"missing release checksums: {', '.join(missing)}")
    return checksums


def render_template(template: Path, replacements: dict[str, str]) -> str:
    content = template.read_text(encoding="utf-8")
    for placeholder, value in replacements.items():
        content = content.replace(placeholder, value)
    unresolved = sorted(set(UNRESOLVED_PLACEHOLDER_PATTERN.findall(content)))
    if unresolved:
        raise ValueError(
            f"unresolved placeholders in {template}: {', '.join(unresolved)}"
        )
    return content


def render_package_metadata(
    version: str, checksums_path: Path, templates_dir: Path, output_dir: Path
) -> tuple[Path, Path]:
    if not VERSION_PATTERN.fullmatch(version):
        raise ValueError(f"invalid release version: {version}")

    checksums = parse_checksums(checksums_path)
    replacements = {"@VERSION@": version}
    replacements.update(
        {
            placeholder: checksums[filename]
            for filename, placeholder in ASSET_PLACEHOLDERS.items()
        }
    )

    outputs = (
        (
            templates_dir / "homebrew" / "pinset.rb.template",
            output_dir / "pinset.rb",
        ),
        (
            templates_dir / "scoop" / "pinset.json.template",
            output_dir / "pinset.json",
        ),
    )
    output_dir.mkdir(parents=True, exist_ok=True)
    for template, destination in outputs:
        destination.write_text(
            render_template(template, replacements), encoding="utf-8", newline="\n"
        )
    return outputs[0][1], outputs[1][1]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--checksums", required=True, type=Path)
    parser.add_argument(
        "--templates-dir",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "packaging",
    )
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()

    try:
        render_package_metadata(
            args.version, args.checksums, args.templates_dir, args.output_dir
        )
    except (OSError, ValueError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    main()
