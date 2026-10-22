#!/usr/bin/env python3
"""Standalone Pinset 2 -> 3 upgrade: install, migrate, verify, activate, clean up.

Python 3.11+; no third-party Python dependencies. See docs/migration-v2-to-v3.md.
"""

import argparse
import base64
import copy
import ctypes
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import zipfile

if sys.version_info < (3, 11):
    raise SystemExit("Pinset migration requires Python 3.11 or later")

import tomllib
import uuid

TOOLS = {"node", "pnpm", "bun", "go", "python", "java", "rust", "flutter"}
PLATFORMS = {"windows-x86_64", "linux-x86_64", "linux-aarch64",
             "macos-x86_64", "macos-aarch64"}
OLD_SERVICE = "dev.pinset.identity"
NEW_SERVICE = "dev.pinset.identity.v3"
DEFAULT_VERSION = "3.0.9"
RESERVED = {"PATH", "JAVA_HOME", "GOROOT", "GOTOOLCHAIN", "RUSTUP_TOOLCHAIN",
            "RUSTUP_HOME", "FLUTTER_ROOT", "VIRTUAL_ENV", "PYTHONHOME", "PYTHONPATH",
            "PYTHONNOUSERSITE", "NPM_CONFIG_MANAGE_PACKAGE_MANAGER_VERSIONS",
            "NPM_CONFIG_PACKAGE_MANAGER_STRICT_VERSION", "NPM_CONFIG_USE_NODE_VERSION"}


class MigrationError(Exception):
    pass


def default_legacy_home():
    if os.environ.get("PINSET_HOME"):
        return os.environ["PINSET_HOME"]
    if os.name == "nt":
        if not os.environ.get("LOCALAPPDATA"):
            raise MigrationError("2.x home requires LOCALAPPDATA or --home")
        return str(Path(os.environ["LOCALAPPDATA"]) / "Pinset")
    return str(Path(os.environ.get("XDG_DATA_HOME") or str(Path.home() / ".local/share")) / "pinset")


def default_v3_home():
    return os.environ.get("PINSET_HOME") or str(Path.home() / ".pinset")


def safe_path(path):
    """Never traverse a symlink in source or destination state paths."""
    path = Path(os.path.abspath(path))
    for part in [*reversed(path.parents), path]:
        if part.is_symlink():
            raise MigrationError(f"symbolic links are not supported: {part}")
    return path


def read_file(path):
    path = safe_path(path)
    if not path.is_file():
        raise MigrationError(f"expected a regular file: {path}")
    return path.read_bytes()


def read_toml(path):
    try:
        return tomllib.loads(read_file(path).decode("utf-8"))
    except (UnicodeError, tomllib.TOMLDecodeError) as error:
        raise MigrationError(f"invalid TOML: {path}") from error


def toml_value(value):
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(item) for item in value) + "]"
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{toml_value(key)} = {toml_value(item)}"
                                  for key, item in value.items()) + " }"
    raise MigrationError("unsupported TOML value in legacy state")


def toml_bytes(document):
    # Inline tables also handle arbitrary public profile names without interpolation.
    return ("".join(f"{toml_value(key)} = {toml_value(value)}\n"
                    for key, value in document.items())).encode("utf-8")


def identifier(value):
    if not isinstance(value, str) or not re.fullmatch(r"[A-Za-z0-9_-]{1,128}", value):
        raise MigrationError("invalid legacy project/profile/identity identifier")
    return value


def convert_profile(content):
    """Change the version envelope only; never decrypt or change age ciphertext."""
    if len(content) > 1024 * 1024 + 64 * 1024:
        raise MigrationError("encrypted profile exceeds the Pinset size limit")
    try:
        lines = content.decode("utf-8").splitlines()
    except UnicodeError as error:
        raise MigrationError("encrypted profile is not UTF-8") from error
    if not lines or lines[0] != "# pinset-encrypted-env v1":
        raise MigrationError("expected a 2.x per-value encrypted profile (v1 header)")
    output = ["# pinset-encrypted-env v3"]
    seen = set()
    for line in lines[1:]:
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            output.append(line)
            continue
        name, separator, value = stripped.partition("=")
        name = name.strip()
        upper = name.upper()
        if (not separator or not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name)
                or upper in seen or upper in RESERVED or upper.startswith("PINSET_")):
            raise MigrationError("profile contains a duplicate, reserved or invalid variable")
        seen.add(upper)
        value = value.strip()
        if value.startswith('"') and value.endswith('"'):
            value = value[1:-1]
        prefix = "encrypted:pinset:v1:"
        if not value.startswith(prefix):
            raise MigrationError("profile contains a value that is not Pinset ciphertext")
        encoded = value[len(prefix):]
        try:
            ciphertext = base64.b64decode(encoded, validate=True)
        except ValueError as error:
            raise MigrationError("invalid encrypted profile base64") from error
        if not ciphertext.startswith(b"age-encryption.org/v1\n"):
            raise MigrationError("profile value is not an age payload")
        output.append(f'{name}="encrypted:pinset:v3:{encoded}"')
    return ("\n".join(output) + "\n").encode("utf-8")


def convert_config(legacy, project_id, skip_unsupported, notices):
    schema = legacy.get("schema")
    if type(schema) is not int or schema not in range(1, 7):
        raise MigrationError(f"unsupported 2.x configuration schema: {schema}")
    if set(legacy) - {"schema", "requirements", "verification", "project-id", "policy", "tools",
                      "tool-options", "tasks", "python", "workspace", "environment"}:
        raise MigrationError("unknown field in 2.x configuration")
    tools = legacy.get("tools", {})
    if not isinstance(tools, dict) or any(not isinstance(v, str) or not v for v in tools.values()):
        raise MigrationError("invalid legacy tool selections")
    unsupported = sorted(set(tools) - TOOLS)
    if unsupported and not skip_unsupported:
        raise MigrationError("3.x does not support: " + ", ".join(unsupported)
                             + "; use --skip-unsupported to retain them only in 2.x")
    if unsupported:
        notices.append("Retained only in 2.x: " + ", ".join(unsupported))
    tools = {key: value for key, value in tools.items() if key in TOOLS}
    if "pnpm" in tools and "node" not in tools:
        raise MigrationError("3.x requires an explicit Node selection alongside pnpm")
    result = {"protocol": "pinset/3", "schema": 3,
              "project_id": identifier(project_id), "tools": tools}
    platforms = legacy.get("requirements", {}).get("platforms", [])
    if any(platform not in PLATFORMS for platform in platforms):
        raise MigrationError("legacy configuration has an unsupported platform")
    if platforms:
        result["platforms"] = platforms
    policy = legacy.get("policy", {})
    new_policy = {}
    for old, new in [("minimum-release-age", "minimum_release_age"),
                     ("verification-strength", "minimum_verification")]:
        if old in policy:
            new_policy[new] = policy[old]
    if new_policy:
        result["policy"] = new_policy
    options = legacy.get("tool-options", {})
    rust = options.get("rust", {})
    if set(rust) - {"profile", "components", "targets", "date"}:
        raise MigrationError("legacy Rust options cannot be represented in 3.x")
    if rust:
        result["rust"] = rust
    if options.keys() - {"rust"}:
        notices.append("Non-Rust tool options remain in the exact lock and legacy configuration")
    for field in ["tasks", "verification", "python"]:
        if legacy.get(field):
            notices.append(f"3.x does not import [{field}]; original declaration is retained")
    if policy.get("system-fallback") or policy.get("boundary", "git") != "git":
        notices.append("3.x uses its own project boundary and does not inherit system fallback")
    environment = legacy.get("environment", {})
    if environment.get("collision", "error") != "error" or environment.get("variables"):
        notices.append("3.x uses collision=error and does not import environment variable contracts")
    profiles = {}
    for name, profile in environment.get("profiles", {}).items():
        identifier(name)
        recipients = profile.get("recipients", [])
        if not recipients or any(not isinstance(value, str) or not value.startswith("age1")
                                 for value in recipients):
            raise MigrationError(f"invalid public recipients for profile {name}")
        profiles[name] = {"recipients": recipients, "grants": {}}
    if profiles:
        result["environment"] = {"profiles": profiles}
        default = environment.get("auto-profile")
        if default is not None:
            if default not in profiles:
                raise MigrationError("legacy default profile is not declared")
            result["environment"]["default"] = default
        notices.append("Encrypted profiles need fresh `pinset env trust add`; old trust is retained")
        notices.append("Recipient access is preserved; old request/grant history is retained in 2.x")
    return result


def rust_lock_options(config):
    options = {}
    for key, value in config.get("rust", {}).items():
        if isinstance(value, list):
            if value:
                options[key] = ",".join(sorted(value))
        else:
            options[key] = value
    return options


def convert_lock(legacy, config):
    if type(legacy.get("schema")) is not int or legacy["schema"] not in range(1, 6):
        raise MigrationError("unsupported 2.x lock schema")
    records = {}
    for record in legacy.get("tool", []):
        name = record.get("name")
        if name in records:
            raise MigrationError("duplicate legacy locked tool")
        records[name] = record
    if (set(records) & TOOLS) - config["tools"].keys():
        raise MigrationError("legacy lock contains an unconfigured tool")
    tools = []
    for name, selector in config["tools"].items():
        record = records.get(name)
        if not record or record.get("requested") != selector:
            raise MigrationError(f"legacy lock is missing or mismatched for {name}; refusing to resolve a new version")
        if not record.get("version") or not record.get("artifact"):
            raise MigrationError(f"incomplete legacy lock for {name}")
        record = copy.deepcopy(record)
        if name == "rust" and record.get("options", {}) != rust_lock_options(config):
            raise MigrationError("legacy Rust options do not match the exact lock")
        tools.append(record)
    return {"protocol": "pinset/3", "schema": 3, "project_id": config["project_id"],
            "generated_by": "standalone-v2-to-v3-migration", "tool": tools}


def integrity_parts(artifact):
    value = artifact.get("integrity") or artifact.get("sha256", "")
    if re.fullmatch(r"[A-Fa-f0-9]{64}", value):
        return "sha256", value.lower()
    for algorithm, size in [("sha256", 64), ("sha512", 128)]:
        prefix = algorithm + ":"
        if value.startswith(prefix) and re.fullmatch(r"[A-Fa-f0-9]{" + str(size) + r"}", value[len(prefix):]):
            return algorithm, value[len(prefix):].lower()
    if value.startswith("sha512-"):
        try:
            digest = base64.b64decode(value[7:], validate=True)
            if len(digest) == 64:
                return "sha512", digest.hex()
        except ValueError:
            pass
    raise MigrationError("invalid digest in legacy artifact lock")


def pnpm_needs_conversion(tool):
    if tool.get("name") != "pnpm":
        return False
    # 2.x locked platform @pnpm/exe packages, without the 3.x entry metadata.
    # Their wrapper/runtime layout also differs; adding an entry is insufficient.
    if any(artifact.get("canonical_url", "").startswith("https://registry.npmjs.org/@pnpm/")
           for artifact in tool.get("artifact", [])):
        return True
    version = re.fullmatch(r"(\d+)\.\d+\.\d+", str(tool.get("version", "")))
    return bool(not tool.get("metadata", {}).get("pnpm-entry") and version and int(version[1]) >= 11)


def cache_entries(home, lock):
    entries = {}
    for tool in lock["tool"]:
        for artifact in tool["artifact"]:
            for layer in [artifact, *artifact.get("overlay", [])]:
                algorithm, digest = integrity_parts(layer)
                relative = Path(algorithm) / (digest + ".archive")
                # Older 2.x releases also used a flat SHA-256 downloads directory.
                candidates = [home / "downloads" / relative]
                if algorithm == "sha256":
                    candidates.append(home / "downloads" / (digest + ".archive"))
                for source in candidates:
                    safe_path(source)
                    if source.exists():
                        entries[relative] = (source, algorithm, digest)
                        break
    return entries


def file_digest(path, algorithm="sha256"):
    safe_path(path)
    if not path.is_file():
        raise MigrationError(f"expected a regular archive: {path}")
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, algorithm).hexdigest()


class NativeCredentials:
    """Use the same native service/account mapping as Rust keyring.

    No private identity enters argv, logs, a file, or a Python keyring fallback.
    """

    def __init__(self):
        if sys.platform == "darwin":
            self.api = ctypes.CDLL("/System/Library/Frameworks/Security.framework/Security")
            self.api.SecKeychainFindGenericPassword.argtypes = [ctypes.c_void_p, ctypes.c_uint32,
                ctypes.c_char_p, ctypes.c_uint32, ctypes.c_char_p, ctypes.POINTER(ctypes.c_uint32),
                ctypes.POINTER(ctypes.c_void_p), ctypes.c_void_p]
            self.api.SecKeychainFindGenericPassword.restype = ctypes.c_int32
            self.api.SecKeychainAddGenericPassword.argtypes = [ctypes.c_void_p, ctypes.c_uint32,
                ctypes.c_char_p, ctypes.c_uint32, ctypes.c_char_p, ctypes.c_uint32,
                ctypes.c_void_p, ctypes.c_void_p]
            self.api.SecKeychainAddGenericPassword.restype = ctypes.c_int32
            self.api.SecKeychainItemFreeContent.argtypes = [ctypes.c_void_p, ctypes.c_void_p]
        elif os.name == "nt":
            from ctypes import wintypes

            class Credential(ctypes.Structure):
                _fields_ = [("Flags", wintypes.DWORD), ("Type", wintypes.DWORD),
                    ("TargetName", wintypes.LPWSTR), ("Comment", wintypes.LPWSTR),
                    ("LastWritten", wintypes.FILETIME), ("CredentialBlobSize", wintypes.DWORD),
                    ("CredentialBlob", ctypes.POINTER(ctypes.c_ubyte)), ("Persist", wintypes.DWORD),
                    ("AttributeCount", wintypes.DWORD), ("Attributes", ctypes.c_void_p),
                    ("TargetAlias", wintypes.LPWSTR), ("UserName", wintypes.LPWSTR)]

            self.credential = Credential
            self.api = ctypes.WinDLL("Advapi32", use_last_error=True)
            self.api.CredReadW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                                          ctypes.POINTER(ctypes.POINTER(Credential))]
            self.api.CredReadW.restype = wintypes.BOOL
            self.api.CredWriteW.argtypes = [ctypes.POINTER(Credential), wintypes.DWORD]
            self.api.CredWriteW.restype = wintypes.BOOL
            self.api.CredFree.argtypes = [ctypes.c_void_p]
        elif sys.platform.startswith("linux") and shutil.which("secret-tool"):
            self.api = None
        else:
            raise MigrationError("identity migration needs macOS Keychain, Windows Credential Manager, "
                                 "or Linux secret-tool; --skip-identities keeps identities in 2.x")

    def get(self, service, account):
        if sys.platform == "darwin":
            service, account = service.encode(), account.encode()
            size, pointer = ctypes.c_uint32(), ctypes.c_void_p()
            status = self.api.SecKeychainFindGenericPassword(None, len(service), service,
                len(account), account, ctypes.byref(size), ctypes.byref(pointer), None)
            if status == -25300:
                return None
            if status:
                raise MigrationError(f"Keychain lookup failed (status {status})")
            try:
                return ctypes.string_at(pointer, size.value)
            finally:
                self.api.SecKeychainItemFreeContent(None, pointer)
        if os.name == "nt":
            pointer = ctypes.POINTER(self.credential)()
            if not self.api.CredReadW(f"{account}.{service}", 1, 0, ctypes.byref(pointer)):
                code = ctypes.get_last_error()
                if code == 1168:
                    return None
                raise MigrationError(f"Credential Manager lookup failed (status {code})")
            try:
                return ctypes.string_at(pointer.contents.CredentialBlob,
                                        pointer.contents.CredentialBlobSize)
            finally:
                self.api.CredFree(pointer)
        result = subprocess.run(["secret-tool", "lookup", "service", service, "username", account],
                                capture_output=True, timeout=60)
        if result.returncode == 1 and not result.stderr:
            return None
        if result.returncode:
            raise MigrationError("Secret Service lookup failed; unlock the native credential store")
        return result.stdout.rstrip(b"\n")

    def put(self, service, account, secret):
        if sys.platform == "darwin":
            service, account = service.encode(), account.encode()
            buffer = ctypes.create_string_buffer(secret)
            status = self.api.SecKeychainAddGenericPassword(None, len(service), service,
                len(account), account, len(secret), buffer, None)
            if status:
                raise MigrationError(f"Keychain write failed (status {status})")
        elif os.name == "nt":
            buffer = (ctypes.c_ubyte * len(secret)).from_buffer_copy(secret)
            credential = self.credential(Type=1, TargetName=f"{account}.{service}",
                CredentialBlobSize=len(secret), CredentialBlob=buffer, Persist=3, UserName=account)
            if not self.api.CredWriteW(ctypes.byref(credential), 0):
                raise MigrationError(f"Credential Manager write failed (status {ctypes.get_last_error()})")
        else:
            result = subprocess.run(["secret-tool", "store", "--label=Pinset migrated identity",
                "service", service, "username", account], input=secret, capture_output=True, timeout=60)
            if result.returncode:
                raise MigrationError("Secret Service write failed; unlock the native credential store")


class Migrator:
    def __init__(self, args):
        self.args = args
        self.home = safe_path(args.home)
        self.destination = safe_path(args.v3_home)
        self.v3 = safe_path(self.destination / "v3")
        self.sources = {}
        self.notices = []
        self.scopes = []
        self.processed = set()
        self.cache = {}
        self.backup = None
        self.backup_base = self.home / "migration-backups"
        self.restore_packages = False

    def load(self, path):
        content = read_file(path)
        self.sources[Path(path)] = content
        try:
            return tomllib.loads(content.decode("utf-8"))
        except (UnicodeError, tomllib.TOMLDecodeError) as error:
            raise MigrationError(f"invalid TOML: {path}") from error

    def add_scope(self, source, root, global_scope=False, inherited=None):
        if root in self.processed:
            return
        self.processed.add(root)
        state = self.v3 / "global" if global_scope else root / ".pinset"
        safe_path(state)
        pending = state / "local/migration-v2-incomplete.json"
        if pending.exists():
            raise MigrationError(f"interrupted migration requires recovery; see {pending}")
        if (state / "config.toml").exists():
            self.notices.append(f"Skipped existing 3.x configuration: {state / 'config.toml'}")
            return
        for path in [state / "lock.toml", state / "env", state / "local"]:
            safe_path(path)
            if path.exists():
                raise MigrationError(f"destination already contains state: {path}")
        legacy = self.load(source)
        if not global_scope and inherited is None:
            inherited = self.workspace_parent(root)
        if inherited is not None and legacy.get("workspace"):
            raise MigrationError("2.x workspace members cannot declare a nested workspace")
        if global_scope and legacy.get("schema") not in [1, 2, 3]:
            raise MigrationError("unsupported 2.x global configuration schema")
        legacy = copy.deepcopy(legacy)
        env_root = root
        lock_path = source.with_name("global.lock" if global_scope else "pinset.lock")
        lock = self.load(lock_path) if lock_path.exists() else {"schema": 5, "tool": []}
        if inherited is not None:
            parent, parent_root = inherited
            for field in ["environment", "requirements", "python"]:
                if field not in legacy and field in parent:
                    legacy[field] = copy.deepcopy(parent[field])
                    if field == "environment":
                        env_root = parent_root
            options = copy.deepcopy(parent.get("tool-options", {}))
            for tool in legacy.get("tools", {}):
                options.pop(tool, None)
            options.update(legacy.get("tool-options", {}))
            legacy["tool-options"] = options
            legacy["tools"] = {**parent.get("tools", {}), **legacy.get("tools", {})}
            self.notices.append(f"Materialized workspace inheritance for {root}")
        if legacy.get("policy", {}).get("inherit-global"):
            global_path = self.home / "state/global.toml"
            global_config = self.load(global_path)
            global_lock = self.load(self.home / "state/global.lock")
            own = set(legacy.get("tools", {}))
            legacy["tools"] = {**global_config.get("tools", {}), **legacy.get("tools", {})}
            lock["tool"] = [record for record in global_lock.get("tool", [])
                            if record["name"] not in own] + lock.get("tool", [])
            self.notices.append(f"Materialized global inheritance for {root}")
        notices = []
        project_id = "global" if global_scope else legacy.get("project-id") or str(uuid.uuid4())
        config = convert_config(legacy, project_id, self.args.skip_unsupported, notices)
        locked = convert_lock(lock, config)
        if any(pnpm_needs_conversion(tool) for tool in locked["tool"]):
            notices.append("Legacy pnpm artifacts will be converted through the 3.x official resolver "
                           "at the same exact version; this step needs registry access")
            if self.args.apply and self.args.offline:
                raise MigrationError("legacy pnpm artifacts need an exact-version official conversion; "
                                     "rerun without --offline. The old environment is retained")
        files = {state / "config.toml": toml_bytes(config), state / "lock.toml": toml_bytes(locked)}
        legacy_files = [source] + ([lock_path] if lock_path.exists() else [])
        for name, profile in legacy.get("environment", {}).get("profiles", {}).items():
            relative = Path(profile["file"])
            if relative.is_absolute() or ".." in relative.parts or "\\" in profile["file"]:
                raise MigrationError("legacy encrypted profile path must stay inside its project")
            path = safe_path(env_root / relative)
            content = read_file(path)
            self.sources[path] = content
            legacy_files.append(path)
            files[state / "env" / f"{name}.env"] = convert_profile(content)
        if not global_scope:
            local_profile = self.local_profile(root, project_id)
            if local_profile:
                if local_profile not in config.get("environment", {}).get("profiles", {}):
                    raise MigrationError("local profile preference is not declared")
                files[state / "local/profile.json"] = json.dumps({"protocol": "pinset/3",
                    "profile": local_profile}, indent=2).encode()
        venv = safe_path(root / ".venv") if not global_scope and "python" in config["tools"] else None
        if venv is not None and venv.exists():
            notices.append("Existing .venv stays intact; installation needs --backup-venv to create a v3 venv")
            if self.args.apply and not self.args.no_install and not self.args.backup_venv:
                raise MigrationError(f"{venv} already exists; use --no-install or --backup-venv (keeps the old venv)")
        self.cache.update(cache_entries(self.home, locked))
        self.notices.extend(f"{source}: {notice}" for notice in notices)
        self.scopes.append({"root": root, "state": state, "global": global_scope,
                            "config": config, "files": files, "venv": venv, "legacy_files": legacy_files})
        workspace = legacy.get("workspace", {})
        for member in workspace.get("members", []):
            relative = Path(member)
            if relative.is_absolute() or ".." in relative.parts or "\\" in member:
                raise MigrationError("workspace member must stay inside its root")
            member_root = safe_path(root / relative)
            member_config = member_root / "pinset.toml"
            if member_config == source:
                raise MigrationError("workspace cannot contain itself")
            self.add_scope(member_config, member_root, inherited=(legacy, root))

    def workspace_parent(self, root):
        boundary = next((path for path in [root, *root.parents] if (path / ".git").exists()), root.anchor)
        for ancestor in root.parents:
            if not ancestor.is_relative_to(Path(boundary)):
                break
            candidate = ancestor / "pinset.toml"
            if candidate.is_file():
                parent = self.load(candidate)
                for member in parent.get("workspace", {}).get("members", []):
                    relative = Path(member)
                    if relative.is_absolute() or ".." in relative.parts or "\\" in member:
                        raise MigrationError("workspace member must stay inside its root")
                    if safe_path(ancestor / relative) == root:
                        return parent, ancestor
        return None

    def local_profile(self, root, project_id):
        matches = []
        for directory in [self.home / "state/environments", self.home / "state/environments/v2"]:
            safe_path(directory)
            if not directory.exists():
                continue
            for path in sorted(directory.glob("*.toml")):
                record = self.load(path)
                old_root = record.get("root", "")
                same_root = os.path.normcase(old_root) == os.path.normcase(str(root))
                if same_root and record.get("project_id") == project_id:
                    matches.append(record["profile"])
        if len(set(matches)) > 1:
            raise MigrationError(f"conflicting legacy local profile preferences: {root}")
        return matches[0] if matches else None

    def plan(self):
        if not Path(self.args.home).is_absolute():
            raise MigrationError("--home / PINSET_HOME must be absolute")
        if not Path(self.args.v3_home).is_absolute():
            raise MigrationError("--v3-home / PINSET_HOME must be absolute")
        if not self.args.no_global and (self.home / "state/global.toml").exists():
            self.add_scope(self.home / "state/global.toml", self.home, global_scope=True)
        roots = [safe_path(path) for path in self.args.project]
        if not roots and Path("pinset.toml").is_file():
            roots.append(safe_path(Path.cwd()))
        if self.args.registered:
            registry = safe_path(self.home / "state/projects")
            if registry.exists():
                for path in sorted(registry.glob("*.toml")):
                    record = self.load(path)
                    config = Path(record["config"])
                    if not config.is_absolute() or config.name != "pinset.toml":
                        raise MigrationError(f"invalid registered project: {path}")
                    if config.exists():
                        roots.append(safe_path(config.parent))
                    else:
                        self.notices.append(f"Skipped missing registered project: {config}")
        # Parents first so workspace inheritance is materialized before explicit member inputs.
        for root in sorted(set(roots), key=lambda path: (len(path.parts), str(path))):
            self.add_scope(root / "pinset.toml", root)
        identity_path = self.home / "state/identities.toml"
        self.identities = self.load(identity_path) if identity_path.exists() else None
        if self.identities and self.identities.get("schema") != 1:
            raise MigrationError("unsupported 2.x identity metadata schema")
        if self.identities:
            for record in self.identities.get("identities", []):
                identifier(record["id"])
                if record.get("backend") != "keyring" or not record.get("recipient", "").startswith("age1"):
                    raise MigrationError("unsupported legacy identity backend")
            if self.args.skip_identities:
                self.notices.append("Identity migration disabled; 3.x profiles require the matching PINSET_IDENTITY")
        return {"mode": "apply" if self.args.apply else "preview", "home": str(self.home),
                "v3_home": str(self.v3),
                "scopes": [{"root": str(scope["root"]), "global": scope["global"],
                            "tools": scope["config"]["tools"],
                            "files": [str(path) for path in scope["files"]]} for scope in self.scopes],
                "cached_archives": len(self.cache),
                "identities": len((self.identities or {}).get("identities", [])),
                "notices": self.notices}

    def cli_environment(self):
        environment = dict(os.environ, PINSET_HOME=str(self.destination))
        # Migration never needs decrypted values, nor a process-level profile choice.
        for name in ["PINSET_IDENTITY", "PINSET_PROFILE", "PINSET_ENV_PROFILE", "PINSET_ENV_RESOLVED"]:
            environment.pop(name, None)
        environment["PINSET_NO_ENV"] = "1"
        return environment

    def cli(self, root, *arguments):
        result = subprocess.run([self.args.pinset, "-C", str(root), "--json", *arguments],
                                env=self.cli_environment(), capture_output=True, text=True)
        if result.returncode:
            raise MigrationError(f"pinset {' '.join(arguments)} failed: {result.stderr.strip() or result.stdout.strip()}")
        try:
            return json.loads(result.stdout)
        except json.JSONDecodeError as error:
            raise MigrationError("pinset did not return a JSON report") from error

    def prepare_identities(self):
        if not self.identities or self.args.skip_identities or not self.identities.get("identities"):
            return [], None
        backend = NativeCredentials()
        path = self.v3 / "state/identities.toml"
        current = read_toml(path) if path.exists() else {"schema": 3, "identities": []}
        if current.get("schema") != 3:
            raise MigrationError("invalid existing 3.x identity metadata")
        records = {record["id"]: record for record in current.get("identities", [])}
        writes = []
        for record in self.identities["identities"]:
            account = record["id"]
            if account in records and records[account] != record:
                raise MigrationError("3.x identity metadata conflicts with a legacy identity")
            secret = backend.get(OLD_SERVICE, account)
            if secret is None or not re.fullmatch(rb"AGE-SECRET-KEY-1[A-Z0-9]+", secret):
                raise MigrationError(f"legacy private identity unavailable: {account}; use --skip-identities only with a matching PINSET_IDENTITY")
            existing = backend.get(NEW_SERVICE, account)
            if existing is not None and existing != secret:
                raise MigrationError("3.x credential conflicts with a legacy identity")
            writes.append((account, secret, existing is None))
            records[account] = record
        return writes, (backend, path, toml_bytes({"schema": 3, "identities": list(records.values())}))

    def backup_sources(self):
        # Detect edits made since planning before creating any migrated configuration.
        for path, content in self.sources.items():
            if read_file(path) != content:
                raise MigrationError(f"legacy source changed while preparing migration: {path}")
        base = safe_path(self.backup_base)
        base.mkdir(parents=True, exist_ok=True, mode=0o700)
        stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        self.backup = base / f"v2-to-v3-{stamp}-{uuid.uuid4().hex[:8]}"
        self.backup.mkdir(mode=0o700)
        inventory = []
        for index, (path, content) in enumerate(self.sources.items()):
            filename = f"{index:04d}-{path.name}"
            destination = self.backup / filename
            self.write_new(destination, content)
            inventory.append({"source": str(path), "backup": filename,
                              "sha256": hashlib.sha256(content).hexdigest()})
        self.write_new(self.backup / "manifest.json", json.dumps({"sources": inventory}, indent=2).encode())

    @staticmethod
    def write_new(path, content):
        safe_path(path)
        path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        with path.open("xb") as stream:
            if os.name != "nt":
                os.fchmod(stream.fileno(), 0o600)
            stream.write(content)

    @staticmethod
    def replace_file(path, content):
        safe_path(path)
        path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as temporary:
            temporary.write(content)
            temporary_path = Path(temporary.name)
        try:
            temporary_path.replace(path)
        finally:
            temporary_path.unlink(missing_ok=True)

    def apply_scope(self, scope):
        created = []
        moved_venv = None
        root, state = scope["root"], scope["state"]
        pending = state / "local/migration-v2-incomplete.json"
        marker_id = uuid.uuid4().hex
        requirements = None
        try:
            if self.restore_packages and scope["venv"] and scope["venv"].exists():
                requirements = self.python_packages(scope["venv"], root)
                self.write_new(self.backup / f"python-{marker_id}.txt", requirements)
            self.write_new(pending, json.dumps({"id": marker_id, "backup": str(self.backup),
                "files": {str(path): hashlib.sha256(content).hexdigest()
                          for path, content in scope["files"].items()}}, indent=2).encode())
            for path, content in scope["files"].items():
                self.write_new(path, content)
                created.append(path)
            # Pinset itself validates the translated configuration, lock, URLs and options.
            flags = ["--global"] if scope["global"] else []
            self.cli(root, "install", *flags, "--plan")
            for name in scope["config"].get("environment", {}).get("profiles", {}):
                self.cli(root, "env", "list", "--profile", name)
            venv = scope["venv"]
            if not self.args.no_install:
                if venv is not None and venv.exists():
                    moved_venv = root / f".venv.pinset-v2-{uuid.uuid4().hex[:8]}"
                    journal = json.loads(read_file(pending))
                    journal["previous_venv"] = str(moved_venv)
                    self.replace_file(pending, json.dumps(journal, indent=2).encode())
                    venv.rename(moved_venv)
                self.cli(root, "install", *flags, *(["--offline"] if self.args.offline else []))
                if requirements is not None:
                    self.install_python_packages(venv, root, self.backup / f"python-{marker_id}.txt", requirements)
                self.cli(root, "check", *flags, "--deep")
            marker = {"protocol": "pinset-migration/1", "backup": str(self.backup),
                      "previous_venv": str(moved_venv) if moved_venv else None,
                      "installed": not self.args.no_install,
                      "python_packages_restored": requirements is not None,
                      "sources": {str(path): hashlib.sha256(self.sources[path]).hexdigest()
                                  for path in scope["legacy_files"]}}
            self.write_new(state / "local/migration-v2.json", json.dumps(marker, indent=2).encode())
            pending.unlink()
            if moved_venv:
                self.notices.append(f"Old Python environment backed up at {moved_venv}"
                    + ("; packages restored in the new .venv" if self.restore_packages
                       else "; reinstall project packages in the new .venv"))
        except BaseException:
            # Remove only files this invocation created. A v3 SDK/cache remains reusable.
            if moved_venv:
                venv = scope["venv"]
                if venv.exists():
                    # Even an incomplete venv has recoverable data. Keep it, too.
                    failed = root / f".venv.pinset-v3-failed-{uuid.uuid4().hex[:8]}"
                    venv.rename(failed)
                    self.notices.append(f"Failed v3 Python environment retained at {failed}")
                moved_venv.rename(venv)
            for path in reversed(created):
                if path.exists() and read_file(path) == scope["files"][path]:
                    path.unlink()
            if pending.exists() and json.loads(read_file(pending)).get("id") == marker_id:
                # Preserve any bindings written by a failed install, and leave a clean retry path.
                local = state / "local"
                local.rename(state / f"local.pinset-v3-failed-{marker_id[:8]}")
            # Empty migration directories should not prevent a retry.
            for path in [state / "env", state / "local", state]:
                if path.is_dir():
                    try:
                        path.rmdir()
                    except OSError:
                        pass
            raise

    def python_packages(self, venv, root):
        python = venv / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        result = subprocess.run([str(python), "-I", "-m", "pip", "freeze", "--all"],
            cwd=root, env=self.cli_environment(), capture_output=True, timeout=60)
        if result.returncode:
            raise MigrationError(f"cannot inventory Python packages in {venv}; old environment is intact")
        return result.stdout

    def convert_pnpm_locks(self):
        conversions = []
        for scope in self.scopes:
            path = scope["state"] / "lock.toml"
            lock = tomllib.loads(scope["files"][path].decode())
            for index, old in enumerate(lock["tool"]):
                if not pnpm_needs_conversion(old):
                    continue
                if self.args.offline:
                    raise MigrationError(f"legacy pnpm {old['version']} needs an exact-version official "
                        "artifact conversion; rerun without --offline. The old environment is retained")
                config = copy.deepcopy(scope["config"])
                # Resolve only the exact locked pnpm, for the same locked platforms.
                config["platforms"] = sorted({artifact["target"] for artifact in old["artifact"]})
                config.pop("environment", None)
                with tempfile.TemporaryDirectory(prefix="pinset-pnpm-conversion-") as temporary:
                    root = Path(temporary).resolve()
                    self.write_new(root / ".pinset/config.toml", toml_bytes(config))
                    self.write_new(root / ".pinset/lock.toml", toml_bytes(lock))
                    report = self.cli(root, "use", f"pnpm@{old['version']}", "--no-install", "--plan")
                candidates = [tool for tool in report.get("lock", {}).get("tool", [])
                              if tool.get("name") == "pnpm"]
                if report.get("protocol") != "pinset/3" or len(candidates) != 1:
                    raise MigrationError("3.x did not return a resolved pnpm lock")
                new = copy.deepcopy(candidates[0])
                if new.get("version") != old["version"]:
                    raise MigrationError("pnpm conversion changed the exact version; refusing migration")
                if {a["target"] for a in new.get("artifact", [])} != set(config["platforms"]):
                    raise MigrationError("pnpm conversion changed the locked platforms; refusing migration")
                if new.get("metadata", {}).get("pnpm-entry", "bin/pnpm.cjs") not in {
                        "bin/pnpm.cjs", "bin/pnpm.mjs", "pnpm"}:
                    raise MigrationError("3.x resolved an unsupported pnpm entry")
                new["requested"] = old["requested"]
                lock["tool"][index] = new
                conversions.append({"scope": str(scope["root"]), "before": old, "after": new})
            scope["files"][path] = toml_bytes(lock)
        if conversions:
            self.write_new(self.backup / "pnpm-lock-conversions.json", json.dumps(conversions, indent=2).encode())
            # Import only caches belonging to the final, verified lock payloads.
            self.cache = {}
            for scope in self.scopes:
                lock = tomllib.loads(scope["files"][scope["state"] / "lock.toml"].decode())
                self.cache.update(cache_entries(self.home, lock))

    def install_python_packages(self, venv, root, requirements_path, expected):
        python = venv / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        for arguments in [["install", "--disable-pip-version-check", "-r", str(requirements_path),
                           *(["--no-index"] if self.args.offline else [])], ["check"]]:
            result = subprocess.run([str(python), "-I", "-m", "pip", *arguments],
                cwd=root, env=self.cli_environment(), capture_output=True)
            if result.returncode:
                # Pip output/requirements can contain private index URLs; do not print them.
                raise MigrationError("Python package restoration failed; restoring the original .venv")
        normalize = lambda content: {line.strip() for line in content.splitlines()
                                     if line.strip() and not line.startswith(b"#")}
        if normalize(self.python_packages(venv, root)) != normalize(expected):
            raise MigrationError("Python package inventory changed; restoring the original .venv")

    def apply(self):
        if not self.scopes and not self.identities:
            return
        info = self.cli(Path.cwd(), "self", "info")
        if info.get("protocol") != "pinset/3" or not str(info.get("version", "")).startswith("3."):
            raise MigrationError("--pinset must point to an installed Pinset 3.x CLI")
        for pending in [self.v3 / "global/local/transaction.json",
                        self.v3 / "state/self-update/transaction.json"]:
            safe_path(pending)
            if pending.exists():
                raise MigrationError(f"recover the existing 3.x transaction before migrating: {pending}")
        writes, identities = self.prepare_identities()
        # Verify old archives before any project changes. Pinset rechecks them on install.
        for source, algorithm, digest in self.cache.values():
            if file_digest(source, algorithm) != digest:
                raise MigrationError(f"legacy download cache checksum mismatch: {source}")
        if self.backup is None:
            self.backup_sources()
        # A neutral project prevents repair from discovering and mutating the caller's project.
        with tempfile.TemporaryDirectory(prefix="pinset-migration-bootstrap-") as temporary:
            bootstrap = Path(temporary).resolve()
            self.write_new(bootstrap / ".pinset/config.toml", toml_bytes({
                "protocol": "pinset/3", "schema": 3, "project_id": uuid.uuid4().hex, "tools": {}}))
            self.cli(bootstrap, "self", "repair")
        self.convert_pnpm_locks()
        for relative, (source, algorithm, digest) in self.cache.items():
            target = safe_path(self.v3 / "cache/downloads" / relative)
            if target.exists():
                if file_digest(target, algorithm) != digest:
                    raise MigrationError(f"existing v3 archive checksum mismatch: {target}")
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                with tempfile.NamedTemporaryFile(dir=target.parent, delete=False) as temporary:
                    temporary_path = Path(temporary.name)
                    with source.open("rb") as stream:
                        shutil.copyfileobj(stream, temporary)
                try:
                    if file_digest(temporary_path, algorithm) != digest:
                        raise MigrationError("legacy archive changed while copying")
                    # A same-directory hard link publishes the complete copy without replacement.
                    os.link(temporary_path, target)
                finally:
                    temporary_path.unlink()
        if identities:
            backend, path, content = identities
            if path.exists():
                self.write_new(self.backup / "v3-identities-before.toml", read_file(path))
            for account, secret, needed in writes:
                if needed:
                    backend.put(NEW_SERVICE, account, secret)
                if backend.get(NEW_SERVICE, account) != secret:
                    raise MigrationError("native credential verification failed")
            # Credentials are copied before publishing public metadata. Old entries stay intact.
            self.replace_file(path, content)
        completed = []
        try:
            for scope in self.scopes:
                self.apply_scope(scope)
                completed.append(str(scope["root"]))
        finally:
            self.write_new(self.backup / "result.json", json.dumps({"completed": completed,
                "notices": self.notices, "old_data_retained": True}, indent=2).encode())


class Upgrader(Migrator):
    """Upgrade orchestration stays outside the Pinset runtime and command surface."""

    def __init__(self, args):
        super().__init__(args)
        self.backup_base = self.destination / "migration-backups"
        self.restore_packages = True
        self.args.backup_venv = True
        self.legacy_entries = {}
        self.existing_scopes = []
        self.journal = self.destination / "upgrade-v2-pending.json"
        self.phase = "preview"
        self.cleanup_complete = False
        self.needs_home_wrapper = self.destination != Path(default_v3_home())
        self.requested_version = args.version is not None
        self.args.version = args.version or DEFAULT_VERSION

    @staticmethod
    def binary_name(name):
        return name + (".exe" if os.name == "nt" else "")

    @staticmethod
    def binary_version(binary):
        # This handshake works with both major versions; never send v3 flags to v2.
        result = subprocess.run([str(binary), "--version"], capture_output=True, text=True, timeout=30)
        match = re.fullmatch(r"pinset ([23]\.\d+\.\d+(?:-rc\.\d+)?)", result.stdout.strip())
        if result.returncode or not match:
            raise MigrationError(f"Pinset version handshake failed: {binary}")
        return match[1]

    def add_scope(self, source, root, global_scope=False, inherited=None):
        if root in self.processed:
            return
        state = self.v3 / "global" if global_scope else root / ".pinset"
        if (state / "config.toml").exists() and not (state / "local/migration-v2-incomplete.json").exists():
            marker = state / "local/migration-v2.json"
            # A prior migration must prove which old files were translated before cleanup.
            if marker.exists():
                record = json.loads(read_file(marker))
                sources = record.get("sources", {})
                complete_python = not record.get("previous_venv") or record.get("python_packages_restored")
                if sources and record.get("installed") and complete_python:
                    for path, digest in sources.items():
                        path = safe_path(path)
                        if path.exists():
                            content = read_file(path)
                            if hashlib.sha256(content).hexdigest() != digest:
                                raise MigrationError(f"legacy source changed after migration: {path}")
                            self.sources[path] = content
                    self.existing_scopes.append({"root": root, "state": state, "global": global_scope,
                                                 "config": read_toml(state / "config.toml")})
                    if source.exists():
                        legacy = self.load(source)
                        for member in legacy.get("workspace", {}).get("members", []):
                            relative = Path(member)
                            if relative.is_absolute() or ".." in relative.parts or "\\" in member:
                                raise MigrationError("workspace member must stay inside its root")
                            child = safe_path(root / relative)
                            if child == root:
                                raise MigrationError("workspace cannot contain itself")
                            self.add_scope(child / "pinset.toml", child, inherited=(legacy, root))
                elif not self.args.keep_v2:
                    raise MigrationError(f"existing migration cannot prove a complete upgrade: {marker}; use --keep-v2")
            elif not self.args.keep_v2:
                raise MigrationError(f"existing 3.x configuration has no migration receipt: {state}; use --keep-v2")
        super().add_scope(source, root, global_scope, inherited)

    def plan(self):
        version = self.args.version.removeprefix("v")
        if not re.fullmatch(r"3\.\d+\.\d+(?:-rc\.\d+)?", version):
            raise MigrationError("--version must be an exact 3.x release")
        self.args.version = version
        safe_path(self.journal)
        if self.journal.exists():
            raise MigrationError(f"interrupted activation/cleanup requires recovery: {self.journal}")
        if not self.args.keep_v2 and (self.args.no_install or self.args.no_global
                or self.args.skip_unsupported or self.args.skip_identities):
            raise MigrationError("partial migration options require --keep-v2; cleanup requires a complete, verified upgrade")
        if os.name == "nt" and self.needs_home_wrapper and not self.args.keep_v2:
            raise MigrationError("set PINSET_HOME to --v3-home before upgrading a custom Windows home; pass --home for the old directory")
        for name in ["state", "installs", "downloads", "cache"]:
            path = safe_path(self.home / name)
            if self.destination.is_relative_to(path) or self.backup_base.is_relative_to(path):
                raise MigrationError(f"3.x home/backup cannot be inside legacy {name}")
        report = super().plan()
        self.discover_legacy_entries()
        report["upgrade"] = {"version": version, "binary": str(self.v3 / "bin" / self.binary_name("pinset")),
            "source": self.args.pinset or "verified official GitHub Release",
            "activate": [str(path) for path in self.legacy_entries],
            "cleanup": not self.args.keep_v2, "backup_directory": str(self.backup_base),
            "remove_files": [] if self.args.keep_v2 else [str(path) for path in self.cleanup_files()],
            "archive_directories": [] if self.args.keep_v2 else [str(self.home / name)
                for name in ["state", "installs", "cache"] if (self.home / name).exists()],
            "remove_download_cache": str(self.home / "downloads") if not self.args.keep_v2 else None,
            "stages": ["install 3.x", "migrate", "verify"]
                + ([] if self.args.keep_v2 else ["activate", "cleanup"])}
        return report

    def remember_entry(self, path):
        # Only the final component may be a Pinset-owned symlink (legacy shims).
        safe_path(path.parent)
        if path.is_symlink():
            value = {"link": os.readlink(path)}
        else:
            value = {"content": read_file(path), "mode": path.stat().st_mode & 0o777}
        self.legacy_entries[path] = value

    def discover_legacy_entries(self):
        candidates = [Path(self.args.legacy_pinset)] if self.args.legacy_pinset else []
        if not candidates:
            directories = [Path(entry) for entry in os.environ.get("PATH", "").split(os.pathsep) if entry]
            directories += [Path.home() / ".local/bin", self.home / "bin"]
            candidates = [directory / self.binary_name("pinset") for directory in directories]
        for candidate in dict.fromkeys(candidates):
            candidate = Path(os.path.abspath(candidate))
            if not candidate.is_file():
                if self.args.legacy_pinset:
                    raise MigrationError(f"legacy CLI is missing: {candidate}")
                continue
            try:
                version = self.binary_version(candidate)
            except MigrationError:
                if self.args.legacy_pinset:
                    raise
                continue
            if not version.startswith("2."):
                if self.args.legacy_pinset:
                    raise MigrationError("--legacy-pinset must identify a 2.x executable")
                continue
            safe_path(candidate)
            self.remember_entry(candidate)
            shim = candidate.with_name(self.binary_name("pinset-shim"))
            if not shim.is_file():
                continue
            safe_path(shim)
            self.remember_entry(shim)
            shim_digest = file_digest(shim, "sha256")
            shim_size = shim.stat().st_size
            for entry in candidate.parent.iterdir():
                if entry in self.legacy_entries or not entry.is_file():
                    continue
                command = entry.stem if entry.suffix in [".cmd", ".exe"] else entry.name
                if not re.fullmatch(r"[A-Za-z0-9_.-]+", command):
                    continue
                owned = entry.is_symlink() and entry.resolve() == shim
                if not owned and not entry.is_symlink():
                    owned = entry.stat().st_size == shim_size and file_digest(entry, "sha256") == shim_digest
                    if os.name == "nt" and entry.stat().st_size < 4096:
                        expected = ('@echo off\r\nsetlocal DisableDelayedExpansion\r\n'
                            f'"{str(shim).replace(chr(37), chr(37) * 2)}" --as {command} -- %*\r\nexit /b %ERRORLEVEL%\r\n').encode()
                        posix = f'#!/bin/sh\nexec "$(dirname "$0")/{command}.cmd" "$@"\n'.encode()
                        owned = entry.read_bytes() in [expected, posix]
                if owned:
                    self.remember_entry(entry)

    def verify_entries_unchanged(self):
        for path, previous in self.legacy_entries.items():
            if "link" in previous:
                same = path.is_symlink() and os.readlink(path) == previous["link"]
            else:
                same = not path.is_symlink() and path.is_file() and read_file(path) == previous["content"]
            if not same:
                raise MigrationError(f"legacy executable changed during upgrade: {path}")

    def save_entry_backups(self):
        entries = []
        for index, (path, previous) in enumerate(self.legacy_entries.items()):
            record = {"path": str(path), **{key: value for key, value in previous.items() if key != "content"}}
            if "content" in previous:
                record["backup"] = f"binaries/{index:04d}-{path.name}"
                self.write_new(self.backup / record["backup"], previous["content"])
                (self.backup / record["backup"]).chmod(previous["mode"])
            entries.append(record)
        self.write_new(self.backup / "binaries.json", json.dumps(entries, indent=2).encode())

    def stage_v3(self, directory):
        managed = self.v3 / "bin" / self.binary_name("pinset")
        if not self.args.pinset and not self.requested_version and managed.is_file():
            if self.binary_version(managed).startswith("3."):
                # Reuse an already upgraded pair rather than accidentally downgrade it.
                self.args.pinset = str(managed)
        if self.args.pinset:
            source = safe_path(Path(self.args.pinset).resolve())
            version = self.binary_version(source)
            if not version.startswith("3."):
                raise MigrationError(f"--pinset points to Pinset {version}; omit it to install 3.x automatically")
            if self.requested_version and version != self.args.version:
                raise MigrationError("supplied CLI version does not match --version")
            self.args.version = version
            shim = safe_path(source.with_name(self.binary_name("pinset-shim")))
            for path, name in [(source, self.binary_name("pinset")), (shim, self.binary_name("pinset-shim"))]:
                shutil.copy2(path, directory / name)
        else:
            if self.args.offline:
                source = self.v3 / "bin" / self.binary_name("pinset")
                if not source.is_file():
                    raise MigrationError("offline upgrade needs an installed 3.x or --pinset with its matching shim")
                self.args.pinset = str(source)
                return self.stage_v3(directory)
            target = self.release_platform()
            name = f"pinset-v{self.args.version}-{target}.zip"
            base = f"https://github.com/Future-Element/pinset/releases/download/v{self.args.version}"
            sums, archive = directory / "SHA256SUMS", directory / name
            self.download(base + "/SHA256SUMS", sums)
            self.download(base + "/" + name, archive)
            records = [line.split() for line in sums.read_text().splitlines()]
            matches = [parts[0] for parts in records if len(parts) == 2 and parts[1].lstrip("*") == name]
            if len(matches) != 1 or not re.fullmatch(r"[a-fA-F0-9]{64}", matches[0]):
                raise MigrationError("release checksum inventory is missing, duplicated or invalid")
            if file_digest(archive, "sha256") != matches[0].lower():
                raise MigrationError("3.x release archive checksum mismatch")
            names = {self.binary_name("pinset"), self.binary_name("pinset-shim")}
            try:
                with zipfile.ZipFile(archive) as package:
                    entries = package.infolist()
                    if len(entries) != 2 or {entry.filename for entry in entries} != names:
                        raise MigrationError("release archive must contain exactly the CLI and matching shim")
                    for entry in entries:
                        if ((entry.external_attr >> 16) & 0o170000 == 0o120000
                                or entry.file_size > 128 * 1024 * 1024 or entry.flag_bits & 1):
                            raise MigrationError("unsafe release archive entry")
                        (directory / entry.filename).write_bytes(package.read(entry))
            except (zipfile.BadZipFile, RuntimeError, NotImplementedError) as error:
                raise MigrationError("invalid 3.x release ZIP") from error
            for name in names:
                (directory / name).chmod(0o755)
            if self.binary_version(directory / self.binary_name("pinset")) != self.args.version:
                raise MigrationError("downloaded CLI does not match the requested release")
        self.args.pinset = str(directory / self.binary_name("pinset"))

    @staticmethod
    def release_platform():
        machine = (os.environ.get("PROCESSOR_ARCHITEW6432") or os.environ.get("PROCESSOR_ARCHITECTURE", "")) \
            if os.name == "nt" else os.uname().machine
        machine = machine.lower()
        architecture = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}.get(machine)
        system = {"linux": "linux", "darwin": "macos", "win32": "windows"}.get(sys.platform)
        target = f"{system}-{architecture}"
        if target not in {"linux-x86_64", "linux-aarch64", "macos-aarch64", "windows-x86_64"}:
            raise MigrationError(f"no official 3.x release for {target}; supply a source build with --pinset")
        return target

    @staticmethod
    def download(url, target):
        try:
            request = urllib.request.Request(url, headers={"User-Agent": "pinset-standalone-upgrade"})
            with urllib.request.urlopen(request, timeout=30) as response, target.open("xb") as stream:
                if not response.geturl().startswith("https://"):
                    raise MigrationError("release download redirected away from HTTPS")
                shutil.copyfileobj(response, stream)
        except OSError as error:
            raise MigrationError(f"3.x download failed: {url}; {error}") from error

    def publish_entry(self, path, target):
        temporary = path.with_name(f".pinset-upgrade-{uuid.uuid4().hex}")
        try:
            if os.name != "nt":
                if self.needs_home_wrapper:
                    temporary.write_bytes(("#!/bin/sh\n"
                        f"export PINSET_HOME={shlex.quote(str(self.destination))}\n"
                        f'exec {shlex.quote(str(target))} "$@"\n').encode())
                    temporary.chmod(0o755)
                else:
                    temporary.symlink_to(target)
            elif path.suffix == ".cmd":
                escaped = str(target).replace("%", "%%")
                temporary.write_bytes(f'@echo off\r\n"{escaped}" %*\r\nexit /b %ERRORLEVEL%\r\n'.encode())
            elif not path.suffix and target.suffix == ".exe":
                # Existing Git Bash wrappers forward to the companion CMD wrapper.
                temporary.write_bytes(f'#!/bin/sh\nexec "$(dirname "$0")/{path.name}.cmd" "$@"\n'.encode())
            else:
                shutil.copy2(target, temporary)
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)

    def activate(self):
        self.verify_entries_unchanged()
        changed = []
        self.replace_file(self.journal, json.dumps({"phase": "activating", "backup": str(self.backup),
            "entries": str(self.backup / "binaries.json")}, indent=2).encode())
        try:
            # Companion first; CLI last. Only positively identified v2 entries are touched.
            paths = sorted(self.legacy_entries, key=lambda path: path.name == self.binary_name("pinset"))
            for path in paths:
                command = path.stem if path.suffix in [".cmd", ".exe"] else path.name
                target = self.v3 / "bin" / self.binary_name(command)
                changed.append(path)
                if target.is_file():
                    self.publish_entry(path, target)
                elif not self.args.keep_v2:
                    path.unlink()
                    self.notices.append(f"Removed legacy command unavailable in 3.x: {path}")
                else:
                    changed.pop()
            for path in paths:
                if path.name == self.binary_name("pinset") and not self.binary_version(path).startswith("3."):
                    raise MigrationError("activated CLI failed its 3.x handshake")
        except BaseException:
            for path in reversed(changed):
                previous = self.legacy_entries[path]
                if "link" in previous:
                    temporary = path.with_name(f".pinset-rollback-{uuid.uuid4().hex}")
                    try:
                        temporary.symlink_to(previous["link"])
                        temporary.replace(path)
                    finally:
                        temporary.unlink(missing_ok=True)
                else:
                    path.unlink(missing_ok=True)
                    self.write_new(path, previous["content"])
                    path.chmod(previous["mode"])
            self.journal.unlink(missing_ok=True)
            raise

    def cleanup_files(self):
        files = []
        roots = {scope["root"] for scope in [*self.scopes, *self.existing_scopes] if not scope["global"]}
        for path in self.sources:
            if not path.is_relative_to(self.home) and any(path.is_relative_to(root) for root in roots):
                # Workspace ancestor configuration not included as a scope stays intact.
                if path.name in {"pinset.toml", "pinset.lock"} and path.parent not in roots:
                    continue
                files.append(path)
        return files

    def cleanup(self):
        # Validate every source before deleting any old project file.
        for path, content in self.sources.items():
            if read_file(path) != content:
                raise MigrationError(f"legacy source changed before cleanup: {path}")
        files = self.cleanup_files()
        directories = [self.home / name for name in ["state", "installs", "cache"]
                       if (self.home / name).exists()]
        downloads = self.home / "downloads"
        operations = {"phase": "cleaning", "backup": str(self.backup), "files": [str(path) for path in files],
                      "directories": [str(path) for path in directories], "downloads": str(downloads)}
        self.replace_file(self.journal, json.dumps(operations, indent=2).encode())
        for path in directories:
            safe_path(path)
            target = self.backup / "legacy-home" / path.name
            target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            # Keep complete SDK/state recovery data out of active legacy paths.
            shutil.move(str(path), str(target))
        for path in files:
            path.unlink()
        if downloads.exists():
            safe_path(downloads)
            shutil.rmtree(downloads)
        try:
            self.home.rmdir()
        except OSError:
            pass
        self.cleanup_complete = True

    def verify_sources_unchanged(self):
        for path, content in self.sources.items():
            if read_file(path) != content:
                raise MigrationError(f"legacy source changed before activation: {path}")

    def apply(self):
        for pending in [self.v3 / "global/local/transaction.json", self.v3 / "state/self-update/transaction.json"]:
            if safe_path(pending).exists():
                raise MigrationError(f"recover the existing 3.x transaction before upgrading: {pending}")
        self.verify_entries_unchanged()
        # Do all read-only source/credential/cache checks before bootstrapping 3.x.
        self.prepare_identities()
        for source, algorithm, digest in self.cache.values():
            if file_digest(source, algorithm) != digest:
                raise MigrationError(f"legacy download cache checksum mismatch: {source}")
        if not self.scopes and not self.existing_scopes and not self.legacy_entries and not self.identities:
            self.notices.append("No 2.x environment or executable found; nothing to upgrade")
            return
        self.backup_sources()
        self.save_entry_backups()
        try:
            with tempfile.TemporaryDirectory(prefix="pinset-upgrade-v3-") as temporary:
                self.phase = "installing"
                self.stage_v3(Path(temporary).resolve())
                if not self.binary_version(self.args.pinset).startswith("3."):
                    raise MigrationError("upgrade requires a verified 3.x CLI")
                # Install the matching pair into the managed home before translating scopes.
                with tempfile.TemporaryDirectory(prefix="pinset-upgrade-bootstrap-") as bootstrap:
                    root = Path(bootstrap).resolve()
                    self.write_new(root / ".pinset/config.toml", toml_bytes({"protocol": "pinset/3",
                        "schema": 3, "project_id": uuid.uuid4().hex, "tools": {}}))
                    self.cli(root, "self", "repair")
                self.args.pinset = str(self.v3 / "bin" / self.binary_name("pinset"))
                self.phase = "migrating"
                super().apply()
                self.phase = "verifying"
                for scope in self.existing_scopes:
                    flags = ["--global"] if scope["global"] else []
                    self.cli(scope["root"], "check", *flags, "--deep")
                    for name in scope["config"].get("environment", {}).get("profiles", {}):
                        self.cli(scope["root"], "env", "list", "--profile", name)
                self.phase = "activating"
                if not self.args.keep_v2:
                    self.verify_sources_unchanged()
                    self.activate()
                    self.phase = "cleaning"
                    self.cleanup()
                self.phase = "complete"
                self.replace_file(self.v3 / "state/upgrade-v2.json", json.dumps({"phase": self.phase,
                    "backup": str(self.backup), "cleanup_complete": self.cleanup_complete}, indent=2).encode())
                self.journal.unlink(missing_ok=True)
                self.replace_file(self.backup / "result.json", json.dumps({
                    "completed": [str(scope["root"]) for scope in [*self.scopes, *self.existing_scopes]],
                    "notices": self.notices, "old_data_retained": self.args.keep_v2,
                    "recovery_data": str(self.backup)}, indent=2).encode())
        finally:
            self.replace_file(self.backup / "upgrade-result.json", json.dumps({"phase": self.phase,
                "cleanup_complete": self.cleanup_complete, "notices": self.notices}, indent=2).encode())


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--home", default=default_legacy_home(), help="2.x data directory (source)")
    result.add_argument("--v3-home", default=default_v3_home(),
                        help="3.x PINSET_HOME base directory (destination; v3 is appended)")
    result.add_argument("--project", action="append", default=[], help="project root; repeat for multiple projects")
    result.add_argument("--registered", action="store_true", default=True,
                        help="include projects registered by 2.x (always included during upgrade)")
    result.add_argument("--no-global", action="store_true", help="do not migrate global selections")
    mode = result.add_mutually_exclusive_group()
    mode.add_argument("--apply", action="store_true", help="install 3.x, migrate, verify, activate and clean up; default: preview")
    mode.add_argument("--dry-run", action="store_true", help="explicit read-only preview (the default)")
    result.add_argument("--version", help=f"exact official 3.x release to install (default: {DEFAULT_VERSION}, or reuse installed 3.x)")
    result.add_argument("--pinset", help="use an existing 3.x executable and adjacent shim instead of downloading")
    result.add_argument("--legacy-pinset", help="explicit 2.x CLI path to replace; default: discover on PATH")
    result.add_argument("--keep-v2", action="store_true", help="install/migrate without replacing old entries or cleaning up (required for partial migrations)")
    result.add_argument("--no-install", action="store_true", help="migrate configuration/locks/profiles; install later")
    result.add_argument("--offline", action="store_true", help="install only from verified cached archives")
    result.add_argument("--backup-venv", action="store_true", help="back up existing .venv before rebuilding (always enabled during upgrade)")
    result.add_argument("--skip-unsupported", action="store_true", help="keep tools removed from 3.x only in legacy state")
    result.add_argument("--skip-identities", action="store_true", help="keep identities in 2.x; supply matching PINSET_IDENTITY to v3")
    return result


def main():
    migrator = None
    try:
        args = parser().parse_args()
        migrator = Upgrader(args)
        plan = migrator.plan()
        print(json.dumps(plan, ensure_ascii=False, indent=2), flush=True)
        if args.apply:
            migrator.apply()
            print(json.dumps({"completed": len(migrator.scopes) + len(migrator.existing_scopes),
                "backup": str(migrator.backup) if migrator.backup else None,
                "phase": migrator.phase, "cleanup_complete": migrator.cleanup_complete,
                "notices": migrator.notices}, ensure_ascii=False, indent=2))
        return 0
    except (MigrationError, OSError, KeyError, TypeError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"Upgrade failed: {error}", file=sys.stderr)
        if migrator and migrator.backup:
            print(f"Backup: {migrator.backup}; completed scopes remain migrated. Rerun to continue.", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
