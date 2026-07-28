from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from scripts.render_package_metadata import (
    ASSET_PLACEHOLDERS,
    parse_checksums,
    render_package_metadata,
)


class RenderPackageMetadataTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary_directory.name)
        self.templates = self.root / "packaging"
        (self.templates / "homebrew").mkdir(parents=True)
        (self.templates / "scoop").mkdir(parents=True)
        (self.templates / "homebrew" / "pinset.rb.template").write_text(
            'version "@VERSION@"\n'
            + "\n".join(ASSET_PLACEHOLDERS.values())
            + "\n",
            encoding="utf-8",
        )
        (self.templates / "scoop" / "pinset.json.template").write_text(
            '{"version":"@VERSION@","hash":"@WINDOWS_X86_64_SHA256@"}\n',
            encoding="utf-8",
        )
        self.checksums = self.root / "SHA256SUMS"
        self.checksums.write_text(
            "\n".join(
                f"{str(index) * 64}  {filename}"
                for index, filename in enumerate(ASSET_PLACEHOLDERS, 1)
            )
            + "\n",
            encoding="utf-8",
        )

    def tearDown(self) -> None:
        self.temporary_directory.cleanup()

    def test_renders_formula_and_manifest(self) -> None:
        output = self.root / "output"
        formula, manifest = render_package_metadata(
            "1.2.3-beta.1", self.checksums, self.templates, output
        )

        formula_text = formula.read_text(encoding="utf-8")
        self.assertIn('version "1.2.3-beta.1"', formula_text)
        self.assertNotIn("@", formula_text)
        self.assertEqual(
            json.loads(manifest.read_text(encoding="utf-8"))["version"],
            "1.2.3-beta.1",
        )

    def test_repository_templates_render(self) -> None:
        repository_templates = Path(__file__).resolve().parents[2] / "packaging"
        formula, manifest = render_package_metadata(
            "1.2.3", self.checksums, repository_templates, self.root / "repository-output"
        )

        formula_text = formula.read_text(encoding="utf-8")
        self.assertIn("class Pinset < Formula", formula_text)
        self.assertIn("pinset-macos-aarch64.tar.gz", formula_text)
        manifest_data = json.loads(manifest.read_text(encoding="utf-8"))
        self.assertEqual(manifest_data["version"], "1.2.3")
        self.assertEqual(manifest_data["bin"], ["pinset.exe", "pinset-shim.exe"])

    def test_rejects_missing_checksum(self) -> None:
        self.checksums.write_text(
            f"{'a' * 64}  pinset-linux-x86_64.tar.gz\n", encoding="utf-8"
        )

        with self.assertRaisesRegex(ValueError, "missing release checksums"):
            parse_checksums(self.checksums)

    def test_rejects_invalid_version(self) -> None:
        with self.assertRaisesRegex(ValueError, "invalid release version"):
            render_package_metadata(
                "../1.2.3", self.checksums, self.templates, self.root / "output"
            )


if __name__ == "__main__":
    unittest.main()
