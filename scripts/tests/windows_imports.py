#!/usr/bin/env python3
"""Offline PE import-policy regression tests. Fixtures are never executable."""

import importlib.util
import io
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "check_windows_imports.py"
SPEC = importlib.util.spec_from_file_location("check_windows_imports", SCRIPT)
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


def fixture(ordinary=("KERNEL32.dll",), delayed=()):
    """Build import-bearing PE32+ bytes, without machine code or execution."""
    data = bytearray(0x2200)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<HHIIIHH", data, 0x84, 0x8664, 1, 0, 0, 0, 240, 0x0002)
    optional = 0x98
    struct.pack_into("<H", data, optional, 0x20B)
    struct.pack_into("<Q", data, optional + 24, 0x140000000)
    struct.pack_into("<II", data, optional + 32, 0x1000, 0x200)
    struct.pack_into("<II", data, optional + 56, 0x3000, 0x200)
    struct.pack_into("<I", data, optional + 108, 16)
    section = optional + 240
    data[section:section + 8] = b".rdata\0\0"
    struct.pack_into("<IIII", data, section + 8, 0x2000, 0x1000, 0x2000, 0x200)
    cursor = 0x200

    def allocate(payload, alignment=1):
        nonlocal cursor
        cursor = (cursor + alignment - 1) // alignment * alignment
        start = cursor
        data[start:start + len(payload)] = payload
        cursor += len(payload)
        return start, start + 0xE00

    def imports(names, directory, width):
        if not names:
            return
        descriptors, rva = allocate(bytes((len(names) + 1) * width), 8)
        struct.pack_into("<II", data, optional + 112 + directory * 8, rva, (len(names) + 1) * width)
        for index, name in enumerate(names):
            _, dll = allocate(name.encode("ascii") + b"\0")
            _, symbol = allocate(b"\0\0GetCurrentProcessId\0", 2)
            _, lookup = allocate(struct.pack("<QQ", symbol, 0), 8)
            _, iat = allocate(struct.pack("<QQ", symbol, 0), 8)
            offset = descriptors + index * width
            if directory == 13:
                _, handle = allocate(bytes(8), 8)
                struct.pack_into("<IIIIIIII", data, offset, 1, dll, handle, iat, lookup, 0, 0, 0)
            else:
                struct.pack_into("<IIIII", data, offset, lookup, 0, 0, dll, iat)

    imports(ordinary, 1, 20)
    imports(delayed, 13, 32)
    return data


class ImportsTests(unittest.TestCase):
    def inspect(self, data):
        return CHECKER.inspect_pe(bytes(data))

    def pair(self, cli, shim=None):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / "pinset.exe").write_bytes(cli)
            if shim is not None:
                (directory / "pinset-shim.exe").write_bytes(shim)
            return CHECKER.check_pair(directory)

    def test_both_executables_and_both_import_tables_are_reported(self):
        report = self.pair(fixture(("KERNEL32.dll", "bcrypt.dll"), ("CRYPT32.dll",)),
                           fixture(("KERNEL32.dll",), ("api-ms-win-core-synch-l1-2-0.dll",)))
        self.assertTrue(report["passed"])
        self.assertEqual([entry["name"] for entry in report["binaries"]], list(CHECKER.BINARY_NAMES))
        self.assertEqual(report["binaries"][0]["imports"],
                         {"ordinary": ["bcrypt.dll", "kernel32.dll"], "delay": ["crypt32.dll"]})
        self.assertTrue(all(len(entry["sha256"]) == 64 for entry in report["binaries"]))

    def test_dynamic_crt_in_either_table_is_rejected(self):
        names = ("VCRUNTIME140.dll", "msvcp140_1.dll", "msvcr120.dll", "msvcrt.dll", "concrt140.dll",
                 "ucrtbase.dll", "ucrtbased.dll", "api-ms-win-crt-runtime-l1-1-0.dll", "vcomp140.dll")
        for name in names:
            for delayed in (False, True):
                for member in (0, 1):
                    with self.subTest(name=name, delayed=delayed, member=member):
                        pair = [fixture(), fixture()]
                        pair[member] = fixture(delayed=(name,)) if delayed else fixture((name,))
                        report = self.pair(*pair)
                        self.assertFalse(report["passed"])
                        self.assertTrue(report["binaries"][1 - member]["passed"])
                        self.assertIn("dynamic C/C++ runtime", " ".join(report["binaries"][member]["errors"]))

    def test_unknown_native_dll_is_rejected(self):
        report = self.pair(fixture(("libcrypto-3-x64.dll",)), fixture())
        self.assertFalse(report["passed"])
        self.assertIn("outside the Windows system allowlist", " ".join(report["binaries"][0]["errors"]))

    def test_system_ucrt_and_vc_runtime_have_distinct_policy_classes(self):
        classes = {
            "KERNEL32.dll": "windows-system", "bcrypt.dll": "windows-system",
            "api-ms-win-core-synch-l1-2-0.dll": "windows-system",
            "ucrtbase.dll": "ucrt", "api-ms-win-crt-runtime-l1-1-0.dll": "ucrt",
            "ucrtbased.dll": "debug-ucrt", "msvcrt.dll": "system-crt", "msvcp_win.dll": "system-crt",
            "VCRUNTIME140.dll": "vc-runtime", "msvcp140.dll": "vc-runtime",
            "libcrypto-3-x64.dll": "unknown",
        }
        for name, expected in classes.items():
            with self.subTest(name=name):
                self.assertEqual(CHECKER.dependency_class(name), expected)
                error = CHECKER.dependency_error(name)
                if expected == "windows-system":
                    self.assertIsNone(error)
                elif expected == "unknown":
                    self.assertIn("outside the Windows system allowlist", error)
                else:
                    self.assertIn(expected, error)
                    self.assertIn("static-CRT policy", error)
        report = self.pair(fixture(("ucrtbase.dll",)), fixture(("msvcp_win.dll",)))
        self.assertFalse(report["passed"])
        self.assertEqual(report["binaries"][0]["import_classes"]["ordinary"]["ucrtbase.dll"], "ucrt")
        self.assertEqual(report["binaries"][1]["import_classes"]["ordinary"]["msvcp_win.dll"], "system-crt")
        self.assertIn("does not prove a missing", report["policy"]["boundary"])

    def test_missing_shim_and_broken_cli_do_not_skip_other_binary(self):
        missing = self.pair(fixture())
        self.assertFalse(missing["passed"])
        self.assertIn("FileNotFoundError", " ".join(missing["binaries"][1]["errors"]))
        report = self.pair(b"not a PE", fixture())
        self.assertFalse(report["passed"])
        self.assertTrue(report["binaries"][1]["passed"])

    def test_wrong_machine_and_optional_header_are_rejected(self):
        for offset, value in ((0x84, 0xAA64), (0x98, 0x10B)):
            data = fixture()
            struct.pack_into("<H", data, offset, value)
            with self.subTest(offset=offset), self.assertRaises(CHECKER.PEFormatError):
                self.inspect(data)

    def test_no_imports_is_inconclusive(self):
        with self.assertRaisesRegex(CHECKER.PEFormatError, "no DLL imports"):
            self.inspect(fixture(()))

    def test_truncated_headers_and_section_data_are_rejected(self):
        data = fixture()
        for length in (0, 63, 0x90, 0x170, 0x1AF, len(data) - 1):
            with self.subTest(length=length), self.assertRaises(CHECKER.PEFormatError):
                self.inspect(data[:length])

    def test_import_directory_must_map_to_file_data(self):
        data = fixture()
        struct.pack_into("<II", data, 0x98 + 112 + 8, 0x2FF0, 40)
        with self.assertRaises(CHECKER.PEFormatError):
            self.inspect(data)

    def test_import_descriptors_require_a_terminator(self):
        for directory, width in ((1, 20), (13, 32)):
            data = fixture() if directory == 1 else fixture((), ("kernel32.dll",))
            struct.pack_into("<I", data, 0x98 + 112 + directory * 8 + 4, width)
            with self.subTest(directory=directory), self.assertRaisesRegex(CHECKER.PEFormatError, "unterminated import descriptor"):
                self.inspect(data)

    def test_unterminated_dll_name_and_paths_are_rejected(self):
        data = fixture()
        struct.pack_into("<I", data, 0x200 + 12, 0x2FF0)
        data[-16:] = b"x" * 16
        with self.assertRaisesRegex(CHECKER.PEFormatError, "unterminated DLL name"):
            self.inspect(data)
        with self.assertRaisesRegex(CHECKER.PEFormatError, "plain DLL basename"):
            self.inspect(fixture(("../KERNEL32.dll",)))

    def test_delay_import_addressing_is_checked(self):
        data = fixture((), ("kernel32.dll",))
        struct.pack_into("<I", data, 0x200, 0)
        with self.assertRaisesRegex(CHECKER.PEFormatError, "RVA addressing"):
            self.inspect(data)

    def test_empty_lookup_table_is_rejected(self):
        data = fixture()
        lookup = struct.unpack_from("<I", data, 0x200)[0] - 0xE00
        struct.pack_into("<Q", data, lookup, 0)
        with self.assertRaisesRegex(CHECKER.PEFormatError, "empty import lookup"):
            self.inspect(data)

    def test_ordinal_imports_and_iat_lookup_fallback_are_supported(self):
        data = fixture()
        struct.pack_into("<I", data, 0x200, 0)
        iat = struct.unpack_from("<I", data, 0x200 + 16)[0] - 0xE00
        struct.pack_into("<Q", data, iat, (1 << 63) | 17)
        self.assertEqual(self.inspect(data)["imports"]["ordinary"], ["kernel32.dll"])

    def test_import_symbols_and_address_table_terminators_are_checked(self):
        data = fixture()
        iat = struct.unpack_from("<I", data, 0x200 + 16)[0] - 0xE00
        struct.pack_into("<Q", data, iat + 8, 1)
        with self.assertRaisesRegex(CHECKER.PEFormatError, "unterminated import address"):
            self.inspect(data)
        data = fixture()
        lookup = struct.unpack_from("<I", data, 0x200)[0] - 0xE00
        struct.pack_into("<Q", data, lookup, 0x400000001)
        with self.assertRaises(CHECKER.PEFormatError):
            self.inspect(data)

    def test_overlapping_sections_are_rejected(self):
        data = fixture()
        struct.pack_into("<H", data, 0x86, 2)
        section = 0x98 + 240
        data[section + 40:section + 80] = data[section:section + 40]
        with self.assertRaisesRegex(CHECKER.PEFormatError, "overlapping virtual sections"):
            self.inspect(data)

    def test_cli_saves_evidence_and_nonzero_failure(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            (directory / "pinset.exe").write_bytes(fixture())
            (directory / "pinset-shim.exe").write_bytes(fixture(delayed=("VCRUNTIME140.dll",)))
            output = directory / "reports/imports.json"
            result = subprocess.run([sys.executable, str(SCRIPT), "--binary-dir", str(directory),
                                     "--output", str(output)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertEqual(json.loads(result.stdout), json.loads(output.read_text()))
            self.assertFalse(json.loads(result.stdout)["passed"])
            self.assertEqual(len(json.loads(result.stdout)["binaries"]), 2)


def verify_windows_imports():
    """Run fixture contracts from the existing local distribution suite."""
    stream = io.StringIO()
    result = unittest.TextTestRunner(stream=stream).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(ImportsTests))
    if not result.wasSuccessful():
        raise AssertionError("Windows import-policy fixtures failed:\n" + stream.getvalue())
    return {"passed": True, "tests": result.testsRun,
            "scope": "synthetic PE32+ fixtures; no real Windows artifact or runtime",
            "tables": ["ordinary imports", "delay imports"],
            "failure_paths": ["dynamic CRT", "unexpected native DLL", "missing/broken pair member",
                              "malformed or unsupported PE", "CLI failure and saved evidence"]}


if __name__ == "__main__":
    print(json.dumps(verify_windows_imports(), indent=2))
