#!/usr/bin/env python3
"""Inspect both Windows release executables without executing them.

This local acceptance check reads AMD64 PE32+ import and delay-import tables.
It rejects dynamic C/C++ runtimes and DLLs outside the Windows system policy.
It does not establish clean-Windows startup, runtime LoadLibrary dependencies,
transitive OS dependencies, shim routing, or WinGet installation. Run those
checks separately on a clean host.
"""

import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import sys


BINARY_NAMES = ("pinset.exe", "pinset-shim.exe")
# Windows SDK system import libraries used by Rust std, Windows APIs, native
# TLS/credential access, and common networking APIs. An unexpected DLL requires
# an explicit policy review; never accept an arbitrary DLL beside the archive.
SYSTEM_DLLS = frozenset("""
advapi32.dll authz.dll bcrypt.dll bcryptprimitives.dll cabinet.dll cfgmgr32.dll
combase.dll comctl32.dll comdlg32.dll credui.dll crypt32.dll cryptbase.dll
cryptnet.dll cryptsp.dll dbghelp.dll dnsapi.dll dwmapi.dll gdi32.dll imagehlp.dll
imm32.dll iphlpapi.dll kernel32.dll kernelbase.dll mpr.dll mswsock.dll ncrypt.dll
netapi32.dll normaliz.dll ntdll.dll ole32.dll oleaut32.dll powrprof.dll propsys.dll
psapi.dll rpcrt4.dll secur32.dll setupapi.dll shell32.dll shcore.dll shlwapi.dll
sspicli.dll user32.dll userenv.dll uxtheme.dll version.dll wevtapi.dll winhttp.dll
wininet.dll winmm.dll winspool.drv wintrust.dll wldap32.dll ws2_32.dll wtsapi32.dll
""".split())
# API-set contract names are supplied by Windows' loader rather than by an
# application package. Keep this limited to the core family required by Rust.
CORE_API_SET = re.compile(r"api-ms-win-core-[a-z0-9-]+\.dll\Z")
VC_RUNTIME = re.compile(r"(?:vcruntime|msvcp|msvcr|concrt|vcomp|vcamp).*\.dll\Z")
SYSTEM_CRT = frozenset({"msvcrt.dll", "msvcp_win.dll"})


class PEFormatError(ValueError):
    pass


class PE:
    def __init__(self, data):
        self.data = data
        self.sections = []
        self.need(0, 64, "DOS header")
        if data[:2] != b"MZ":
            raise PEFormatError("missing DOS MZ signature")
        pe = self.u32(0x3C)
        if pe < 64:
            raise PEFormatError("PE header overlaps DOS header")
        self.need(pe, 24, "PE/COFF header")
        if data[pe:pe + 4] != b"PE\0\0":
            raise PEFormatError("missing PE signature")
        if self.u16(pe + 4) != 0x8664:
            raise PEFormatError("expected AMD64 machine (0x8664)")
        count = self.u16(pe + 6)
        if not 1 <= count <= 96:
            raise PEFormatError("invalid section count")
        optional_size = self.u16(pe + 20)
        optional = pe + 24
        self.need(optional, optional_size, "optional header")
        if optional_size < 112 or self.u16(optional) != 0x20B:
            raise PEFormatError("expected a PE32+ optional header")
        if not self.u16(pe + 22) & 0x0002:
            raise PEFormatError("PE is not marked executable")
        self.image_size = self.u32(optional + 56)
        self.header_size = self.u32(optional + 60)
        directory_count = self.u32(optional + 108)
        if directory_count > 16 or 112 + directory_count * 8 > optional_size:
            raise PEFormatError("invalid data-directory count/size")
        table = optional + optional_size
        self.need(table, count * 40, "section table")
        if not table + count * 40 <= self.header_size <= len(data):
            raise PEFormatError("invalid SizeOfHeaders")
        if self.image_size < self.header_size:
            raise PEFormatError("invalid SizeOfImage")
        for index in range(count):
            section = table + index * 40
            virtual_size, address, raw_size, raw = struct.unpack_from("<IIII", data, section + 8)
            span = max(virtual_size, raw_size)
            if not span:
                continue
            if address < self.header_size or address + span > self.image_size:
                raise PEFormatError("invalid section virtual range")
            if raw_size:
                self.need(raw, raw_size, "section raw data")
                if raw < self.header_size:
                    raise PEFormatError("section raw data overlaps headers")
            for previous_address, previous_span, previous_raw, previous_size in self.sections:
                if address < previous_address + previous_span and previous_address < address + span:
                    raise PEFormatError("overlapping virtual sections")
                if raw_size and previous_size and raw < previous_raw + previous_size and previous_raw < raw + raw_size:
                    raise PEFormatError("overlapping raw sections")
            self.sections.append((address, span, raw, raw_size))
        self.directories = [struct.unpack_from("<II", data, optional + 112 + index * 8)
                            for index in range(directory_count)]

    def need(self, offset, size, label):
        if offset < 0 or size < 0 or offset + size > len(self.data):
            raise PEFormatError(f"truncated/out-of-range {label}")

    def u16(self, offset):
        self.need(offset, 2, "16-bit field")
        return struct.unpack_from("<H", self.data, offset)[0]

    def u32(self, offset):
        self.need(offset, 4, "32-bit field")
        return struct.unpack_from("<I", self.data, offset)[0]

    def location(self, rva, size=1):
        if not rva or rva + size > self.image_size or rva + size > 0x100000000:
            raise PEFormatError("invalid RVA range")
        if rva < self.header_size:
            if rva + size <= self.header_size:
                return rva, self.header_size
            raise PEFormatError("RVA crosses header boundary")
        for address, _, raw, raw_size in self.sections:
            relative = rva - address
            if 0 <= relative < raw_size and relative + size <= raw_size:
                return raw + relative, raw + raw_size
        raise PEFormatError("RVA is not backed by section file data")

    def string(self, rva, label):
        offset, limit = self.location(rva)
        end = self.data.find(b"\0", offset, min(limit, offset + 4096))
        if end < 0:
            raise PEFormatError(f"unterminated {label}")
        value = self.data[offset:end]
        if not value or any(byte < 33 or byte > 126 for byte in value):
            raise PEFormatError(f"invalid ASCII {label}")
        return value.decode("ascii")

    def dll_name(self, rva):
        value = self.string(rva, "DLL name").lower()
        if not re.fullmatch(r"[a-z0-9_.-]+\.(?:dll|drv)", value):
            raise PEFormatError("DLL import must be a plain DLL basename")
        return value

    def thunks(self, names_rva, iat_rva):
        if not names_rva or not iat_rva:
            raise PEFormatError("missing import lookup/address table")
        for index in range(65536):
            offset, _ = self.location(names_rva + index * 8, 8)
            value = struct.unpack_from("<Q", self.data, offset)[0]
            if not value:
                if not index:
                    raise PEFormatError("empty import lookup table")
                iat, _ = self.location(iat_rva, (index + 1) * 8)
                if struct.unpack_from("<Q", self.data, iat + index * 8)[0]:
                    raise PEFormatError("unterminated import address table")
                return index
            if value & (1 << 63):
                if value & ~((1 << 63) | 0xFFFF) or not value & 0xFFFF:
                    raise PEFormatError("invalid ordinal import")
            else:
                self.location(value, 3)
                self.string(value + 2, "import symbol")
        raise PEFormatError("import lookup table exceeds safety limit")

    def imports(self, index, delayed=False):
        if index >= len(self.directories):
            return []
        rva, size = self.directories[index]
        if not rva and not size:
            return []
        width = 32 if delayed else 20
        if not rva or size < width:
            raise PEFormatError("invalid import directory RVA/size")
        self.location(rva, size)
        result = []
        for relative in range(0, size - width + 1, width):
            offset, _ = self.location(rva + relative, width)
            fields = struct.unpack_from("<" + "I" * (width // 4), self.data, offset)
            if not any(fields):
                return result
            if delayed:
                attributes, name, handle, iat, names, bound, unload, _ = fields
                if attributes != 1:
                    raise PEFormatError("delay imports must use RVA addressing")
                if handle:
                    self.location(handle, 8)
                count = self.thunks(names, iat)
                for table in (bound, unload):
                    if table:
                        self.location(table, (count + 1) * 8)
            else:
                names, _, _, name, iat = fields
                self.thunks(names or iat, iat)
            result.append(self.dll_name(name))
        raise PEFormatError("unterminated import descriptor table")


def inspect_pe(data):
    """Return direct/delay DLL names, or reject malformed/unsupported PE data."""
    pe = PE(data)
    ordinary = pe.imports(1)
    delayed = pe.imports(13, delayed=True)
    if not ordinary and not delayed:
        raise PEFormatError("no DLL imports found; refusing an inconclusive inspection")
    return {"machine": "AMD64", "pe_format": "PE32+",
            "imports": {"ordinary": sorted(set(ordinary)), "delay": sorted(set(delayed))}}


def dependency_class(name):
    name = name.lower()
    if name == "ucrtbased.dll":
        return "debug-ucrt"
    if name == "ucrtbase.dll" or name.startswith("api-ms-win-crt-"):
        return "ucrt"
    if name in SYSTEM_CRT:
        return "system-crt"
    if VC_RUNTIME.fullmatch(name):
        return "vc-runtime"
    if name in SYSTEM_DLLS or CORE_API_SET.fullmatch(name):
        return "windows-system"
    return "unknown"


def dependency_error(name):
    kind = dependency_class(name)
    if kind in {"vc-runtime", "ucrt", "debug-ucrt", "system-crt"}:
        return f"dynamic C/C++ runtime import ({kind}); violates this candidate's static-CRT policy"
    if kind == "unknown":
        return "DLL is outside the Windows system allowlist"
    return None


def check_pair(binary_dir):
    report = {"schema_version": 1, "status": "passed", "passed": True,
              "scope": "AMD64 PE32+ direct and delay DLL imports only; no executable was run",
              "policy": {"require_static_crt": True,
                         "boundary": "UCRT and some legacy CRT DLLs are Windows OS components. "
                                     "Their direct imports are rejected by this static-CRT policy. "
                                     "An import alone does not prove a missing VC++ Redistributable or DLL."},
              "unverified": ["clean Windows startup", "managed shim and broker routing",
                             "WinGet install/uninstall", "runtime LoadLibrary dependencies",
                             "transitive Windows system dependencies"],
              "binaries": []}
    for name in BINARY_NAMES:
        path = Path(binary_dir) / name
        entry = {"name": name, "path": str(path.resolve()), "passed": False, "errors": []}
        report["binaries"].append(entry)
        try:
            data = path.read_bytes()
            entry["sha256"] = hashlib.sha256(data).hexdigest()
            entry["size"] = len(data)
            entry.update(inspect_pe(data))
            entry["import_classes"] = {kind: {dependency: dependency_class(dependency) for dependency in names}
                                       for kind, names in entry["imports"].items()}
            for kind, names in entry["imports"].items():
                for dependency in names:
                    reason = dependency_error(dependency)
                    if reason:
                        entry["errors"].append(f"{kind}: {dependency}: {reason}")
            entry["passed"] = not entry["errors"]
        except (OSError, PEFormatError) as error:
            entry["errors"].append(f"{type(error).__name__}: {error}")
        if not entry["passed"]:
            report.update(status="failed", passed=False)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary-dir", type=Path, default=Path("target/x86_64-pc-windows-msvc/release"),
                        help="directory containing both pinset.exe and pinset-shim.exe")
    parser.add_argument("--output", type=Path, help="also save the JSON report to this file")
    args = parser.parse_args()
    report = check_pair(args.binary_dir)
    text = json.dumps(report, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text, encoding="utf-8")
    sys.stdout.write(text)
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
