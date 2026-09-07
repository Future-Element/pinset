#!/usr/bin/env python3
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("release_version", Path(__file__).resolve().parents[1] / "validate_release_version.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ReleaseVersionTest(unittest.TestCase):
    def test_double_digit_minor_versions_keep_their_exact_base(self):
        for base in ("2.9.0", "2.10.0"):
            self.assertTrue(module.validate(base, f"v{base}-rc.10"))
        self.assertFalse(module.validate("2.9.0", "v2.10.0-rc.1"))

    def test_stable_and_multiple_candidates(self):
        for tag in ("v2.2.0", "2.2.0", "v2.2.0-rc.1", "v2.2.0-rc.2", "v2.2.0-rc.10"):
            with self.subTest(tag=tag):
                self.assertTrue(module.validate("2.2.0", tag))

    def test_rejects_mismatches_and_invalid_candidates(self):
        for tag in ("v2.3.0", "v2.2.1-rc.1", "v2.2.0-rc.0", "v2.2.0-rc.01", "v2.2.0-rc.-1", "v2.2.0-rc.1.extra", "v2.2.0-beta.1", "v2.2.0\n"):
            with self.subTest(tag=tag):
                self.assertFalse(module.validate("2.2.0", tag))
        self.assertFalse(module.validate("2.02.0", "v2.02.0"))


if __name__ == "__main__":
    unittest.main()
