"""Standalone upgrade contracts; fixtures never access the user's state or credentials."""

import base64
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest
import zipfile
from unittest.mock import patch
from unittest.mock import Mock

SCRIPT = Path(__file__).resolve().parents[1] / "migrate_v2_to_v3.py"
SPEC = importlib.util.spec_from_file_location("migration", SCRIPT)
migration = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(migration)


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(migration.toml_bytes(value))


def locked(name="node", requested="lts", version="22.12.0", payload=b"fixture archive"):
    return {"name": name, "requested": requested, "version": version,
            "provider": "nodejs-official", "metadata": {}, "options": {}, "artifact": [{
                "target": "linux-x86_64", "canonical_url": "https://nodejs.org/dist/v22.12.0/node-v22.12.0-linux-x64.tar.xz",
                "artifact_path": "v22.12.0/node-v22.12.0-linux-x64.tar.xz",
                "sha256": hashlib.sha256(payload).hexdigest(), "format": "tar.xz",
                "archive_root": "node-v22.12.0-linux-x64", "verification": "checksum"}]}


def pnpm_locked(targets=("linux-x86_64",), version="11.21.0", requested="latest",
                native=b"native fixture", wrapper=b"wrapper fixture", javascript=False):
    packages = {"linux-x86_64": "linux-x64", "linux-aarch64": "linux-arm64",
                "macos-aarch64": "macos-arm64", "windows-x86_64": "win-x64"}
    def artifact(package, payload):
        name = package.rsplit("/", 1)[-1]
        return {"canonical_url": f"https://registry.npmjs.org/{package}/-/{name}-{version}.tgz",
                "artifact_path": f"{package}/-/{name}-{version}.tgz", "format": "tar.gz",
                "archive_root": "package", "verification": "npm-registry-signature-sha512",
                "integrity": "sha512-" + base64.b64encode(hashlib.sha512(payload).digest()).decode()}
    return {"name": "pnpm", "requested": requested, "version": version, "provider": "pnpm-npm",
            "metadata": {"pnpm-entry": "bin/pnpm.mjs"} if javascript else {}, "options": {},
            "artifact": [{"target": target, **artifact("pnpm" if javascript else "@pnpm/" + packages[target],
                                                      wrapper if javascript else native),
                          **({} if javascript else {"overlay": [artifact("@pnpm/exe", wrapper)]})}
                         for target in targets]}


class MigrationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="pinset-migration-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.home = self.root / "home"
        self.project = self.root / "project"
        self.project.mkdir()
        self.config = {"schema": 4, "project-id": "project-one", "tools": {"node": "lts"}}
        save(self.project / "pinset.toml", self.config)
        save(self.project / "pinset.lock", {"schema": 3, "generated_by": "2.0.0", "tool": [locked()]})
        self.calls = []

    def migrator(self, *flags):
        args = migration.parser().parse_args(["--home", str(self.home), "--v3-home", str(self.home),
                                             "--project", str(self.project), *flags])
        return migration.Migrator(args)

    def fake_cli(self, root, *arguments):
        self.calls.append(arguments)
        if arguments == ("self", "info"):
            return {"protocol": "pinset/3", "version": migration.DEFAULT_VERSION}
        if arguments == ("self", "repair"):
            self.home.joinpath("v3").mkdir(parents=True, exist_ok=True)
            self.home.joinpath("v3/.pinset-home.json").write_text('{"protocol":"pinset/3"}')
        return {"protocol": "pinset/3"}

    def snapshot(self):
        return {str(path.relative_to(self.root)): path.read_bytes()
                for path in self.root.rglob("*") if path.is_file()}

    def apply(self, migrator, cli=None):
        migrator.plan()
        with patch.object(migrator, "cli", side_effect=cli or self.fake_cli):
            migrator.apply()

    def test_preview_preserves_every_byte_and_does_not_require_v3(self):
        before = self.snapshot()
        migrator = self.migrator("--pinset", "/missing/pinset")
        report = migrator.plan()
        self.assertEqual(report["mode"], "preview")
        self.assertEqual(len(report["scopes"]), 1)
        self.assertEqual(self.snapshot(), before)

    def test_apply_preserves_floating_selector_exact_lock_and_original_files(self):
        originals = self.snapshot()
        migrator = self.migrator("--apply", "--no-install")
        self.apply(migrator)
        config = migration.read_toml(self.project / ".pinset/config.toml")
        lock = migration.read_toml(self.project / ".pinset/lock.toml")
        self.assertEqual(config["tools"]["node"], "lts")
        self.assertEqual(lock["tool"][0]["version"], "22.12.0")
        self.assertEqual(lock["project_id"], config["project_id"])
        self.assertEqual(lock["tool"][0], locked())
        for path, content in originals.items():
            self.assertEqual((self.root / path).read_bytes(), content)
        manifest = json.loads((migrator.backup / "manifest.json").read_text())
        self.assertEqual(len(manifest["sources"]), 2)
        for item in manifest["sources"]:
            self.assertEqual(hashlib.sha256((migrator.backup / item["backup"]).read_bytes()).hexdigest(), item["sha256"])

    def test_repeat_run_does_not_overwrite_v3_edits(self):
        migrator = self.migrator("--apply", "--no-install")
        self.apply(migrator)
        target = self.project / ".pinset/config.toml"
        target.write_bytes(target.read_bytes() + b"# edited after migration\n")
        before = self.snapshot()
        second = self.migrator("--apply", "--no-install")
        self.apply(second)
        self.assertEqual(self.snapshot(), before)
        self.assertEqual(second.scopes, [])

    def test_mismatched_and_missing_locks_fail_without_writes(self):
        for lock in [{"schema": 3, "tool": []},
                     {"schema": 3, "tool": [locked(requested="latest")]}]:
            save(self.project / "pinset.lock", lock)
            before = self.snapshot()
            with self.assertRaises(migration.MigrationError):
                self.migrator().plan()
            self.assertEqual(self.snapshot(), before)

    def test_unsupported_tools_require_explicit_skip(self):
        self.config["tools"]["dotnet"] = "9.0.1"
        save(self.project / "pinset.toml", self.config)
        with self.assertRaisesRegex(migration.MigrationError, "dotnet"):
            self.migrator().plan()
        migrator = self.migrator("--skip-unsupported")
        self.assertEqual(migrator.plan()["scopes"][0]["tools"], {"node": "lts"})

    def pnpm_migrator(self, record, *flags):
        self.config["tools"]["pnpm"] = record["requested"]
        save(self.project / "pinset.toml", self.config)
        save(self.project / "pinset.lock", {"schema": 5, "tool": [locked(), record]})
        return self.migrator(*flags)

    def test_native_pnpm_conversion_keeps_exact_version_selector_and_platforms(self):
        targets = ("linux-aarch64", "linux-x86_64", "macos-aarch64", "windows-x86_64")
        old = pnpm_locked(targets)
        new = pnpm_locked(targets, requested="11.21.0", javascript=True)
        migrator = self.pnpm_migrator(old, "--apply", "--no-install")
        def resolve(root, *arguments):
            if arguments[0] == "use":
                self.assertEqual(arguments, ("use", "pnpm@11.21.0", "--no-install", "--plan"))
                config = migration.read_toml(root / ".pinset/config.toml")
                self.assertEqual(set(config["platforms"]), set(targets))
                self.assertEqual(config["tools"]["node"], "lts")
                return {"protocol": "pinset/3", "lock": {"tool": [new]}}
            return self.fake_cli(root, *arguments)
        self.apply(migrator, resolve)
        converted = migration.read_toml(self.project / ".pinset/lock.toml")["tool"][1]
        self.assertEqual(converted["version"], "11.21.0")
        self.assertEqual(converted["requested"], "latest")
        self.assertEqual(converted["metadata"]["pnpm-entry"], "bin/pnpm.mjs")
        self.assertTrue(all(not artifact.get("overlay") for artifact in converted["artifact"]))
        self.assertEqual(migration.read_toml(self.project / "pinset.lock")["tool"][1], old)
        report = json.loads((migrator.backup / "pnpm-lock-conversions.json").read_text())
        self.assertEqual(report[0]["before"], old)
        self.assertEqual(report[0]["after"], converted)

    def test_pnpm_conversion_rejects_changed_version_or_missing_platform(self):
        targets = ("linux-x86_64", "windows-x86_64")
        old = pnpm_locked(targets)
        for new, message in [(pnpm_locked(targets, version="12.0.0", javascript=True), "exact version"),
                             (pnpm_locked(javascript=True), "locked platforms")]:
            migrator = self.pnpm_migrator(old, "--apply", "--no-install")
            def resolve(root, *arguments):
                return {"protocol": "pinset/3", "lock": {"tool": [new]}} if arguments[0] == "use" \
                    else self.fake_cli(root, *arguments)
            with self.assertRaisesRegex(migration.MigrationError, message):
                self.apply(migrator, resolve)
            self.assertFalse((self.project / ".pinset/config.toml").exists())
            self.assertEqual(migration.read_toml(self.project / "pinset.lock")["tool"][1], old)

    def test_offline_legacy_pnpm_conversion_fails_before_writes(self):
        migrator = self.pnpm_migrator(pnpm_locked(), "--apply", "--offline")
        before = self.snapshot()
        with self.assertRaisesRegex(migration.MigrationError, "without --offline"):
            migrator.plan()
        self.assertEqual(before, self.snapshot())

    def test_compatible_javascript_pnpm_lock_is_preserved_without_resolution(self):
        old = pnpm_locked(javascript=True)
        migrator = self.pnpm_migrator(old, "--apply", "--no-install", "--offline")
        self.apply(migrator)
        self.assertEqual(migration.read_toml(self.project / ".pinset/lock.toml")["tool"][1], old)
        self.assertFalse((migrator.backup / "pnpm-lock-conversions.json").exists())

    def test_native_pnpm_10_without_overlay_also_requires_conversion(self):
        old = pnpm_locked(version="10.34.5")
        for artifact in old["artifact"]:
            artifact.pop("overlay")
        self.assertTrue(migration.pnpm_needs_conversion(old))
        self.assertFalse(migration.pnpm_needs_conversion(locked()))

    def test_global_and_registered_projects(self):
        save(self.home / "state/global.toml", {"schema": 3, "tools": {"node": "lts"}})
        save(self.home / "state/global.lock", {"schema": 5, "tool": [locked()]})
        save(self.home / "state/projects/record.toml", {"schema": 1, "config": str(self.project / "pinset.toml")})
        save(self.home / "state/projects/missing.toml", {"schema": 1, "config": str(self.root / "missing/pinset.toml")})
        migrator = self.migrator("--registered")
        report = migrator.plan()
        self.assertEqual(len(report["scopes"]), 2)
        self.assertEqual(migrator.scopes[0]["config"]["project_id"], "global")
        self.assertTrue(any("missing registered" in value for value in report["notices"]))

    def test_workspace_inheritance_is_materialized_for_each_member(self):
        self.config["workspace"] = {"members": ["member"]}
        self.config["schema"] = 5
        save(self.project / "pinset.toml", self.config)
        member = self.project / "member"
        save(member / "pinset.toml", {"schema": 5, "project-id": "member-one", "tools": {}})
        save(member / "pinset.lock", {"schema": 5, "tool": [locked()]})
        migrator = self.migrator()
        self.assertEqual(len(migrator.plan()["scopes"]), 2)
        self.assertEqual(migrator.scopes[1]["config"]["tools"], {"node": "lts"})
        self.assertEqual(migrator.scopes[1]["config"]["project_id"], "member-one")

    def test_global_inheritance_keeps_project_overrides(self):
        save(self.home / "state/global.toml", {"schema": 3, "tools": {"node": "latest", "go": "1.24.0"}})
        save(self.home / "state/global.lock", {"schema": 5,
            "tool": [locked(requested="latest"), locked("go", "1.24.0", "1.24.0")]})
        self.config["policy"] = {"inherit-global": True}
        save(self.project / "pinset.toml", self.config)
        migrator = self.migrator("--no-global")
        migrator.plan()
        self.assertEqual(migrator.scopes[0]["config"]["tools"], {"node": "lts", "go": "1.24.0"})

    def test_encrypted_profile_envelope_changes_without_touching_age_payload(self):
        encoded = base64.b64encode(b"age-encryption.org/v1\nfixture encrypted bytes").decode()
        content = ('# pinset-encrypted-env v1\n# keep comment\nTOKEN="encrypted:pinset:v1:' + encoded + '"\n').encode()
        self.config["environment"] = {"auto-profile": "dev", "profiles": {
            "dev": {"file": ".env.dev", "recipients": ["age1publicrecipient"]}}}
        save(self.project / "pinset.toml", self.config)
        (self.project / ".env.dev").write_bytes(content)
        save(self.home / "state/environments/v2/local.toml", {"schema": 2, "root": str(self.project),
            "project_id": "project-one", "profile": "dev"})
        migrator = self.migrator("--apply", "--no-install")
        self.apply(migrator)
        translated = (self.project / ".pinset/env/dev.env").read_text()
        self.assertIn('encrypted:pinset:v3:' + encoded, translated)
        self.assertEqual((self.project / ".env.dev").read_bytes(), content)
        self.assertIn("# keep comment", translated)
        self.assertEqual(json.loads((self.project / ".pinset/local/profile.json").read_text())["profile"], "dev")
        self.assertFalse((self.home / "v3/state/trust").exists())

    def test_plaintext_reserved_variables_and_traversal_are_rejected(self):
        for assignment in ["TOKEN=plaintext", "JAVA_HOME=encrypted:pinset:v1:YQ==",
                           "PATH=encrypted:pinset:v1:YQ==", "PINSET_HOME=secret", "bad-name=value"]:
            with self.assertRaises(migration.MigrationError):
                migration.convert_profile(("# pinset-encrypted-env v1\n" + assignment).encode())
        self.config["environment"] = {"profiles": {"dev": {"file": "../secret.env", "recipients": ["age1recipient"]}}}
        save(self.project / "pinset.toml", self.config)
        with self.assertRaises(migration.MigrationError):
            self.migrator().plan()

    @unittest.skipIf(os.name == "nt", "symlink fixture needs Windows symlink privilege")
    def test_symlinked_state_and_source_are_rejected(self):
        target = self.root / "external"
        target.mkdir()
        (self.project / ".pinset").symlink_to(target, target_is_directory=True)
        with self.assertRaises(migration.MigrationError):
            self.migrator().plan()
        (self.project / ".pinset").unlink()
        (self.project / "pinset.toml").unlink()
        (self.project / "pinset.toml").symlink_to(target / "config.toml")
        with self.assertRaises(migration.MigrationError):
            self.migrator().plan()

    def test_old_cli_is_rejected_before_backup_or_state_changes(self):
        migrator = self.migrator("--apply", "--no-install")
        before = self.snapshot()
        with self.assertRaises(migration.MigrationError):
            self.apply(migrator, lambda *_: {"version": "2.16.2"})
        self.assertEqual(self.snapshot(), before)

    def test_sha256_and_sha512_cache_import_is_verified_and_old_cache_retained(self):
        payload = b"fixture archive"
        sha256 = hashlib.sha256(payload).hexdigest()
        sha512 = hashlib.sha512(payload).hexdigest()
        tool = locked(payload=payload)
        tool["artifact"][0]["overlay"] = [{"integrity": "sha512-" + base64.b64encode(bytes.fromhex(sha512)).decode()}]
        save(self.project / "pinset.lock", {"schema": 5, "tool": [tool]})
        for algorithm, digest in [("sha256", sha256), ("sha512", sha512)]:
            archive = self.home / "downloads" / algorithm / (digest + ".archive")
            archive.parent.mkdir(parents=True)
            archive.write_bytes(payload)
        migrator = self.migrator("--apply", "--no-install")
        self.apply(migrator)
        for algorithm, digest in [("sha256", sha256), ("sha512", sha512)]:
            original = self.home / "downloads" / algorithm / (digest + ".archive")
            imported = self.home / "v3/cache/downloads" / algorithm / original.name
            self.assertEqual(imported.read_bytes(), payload)
            self.assertEqual(original.read_bytes(), payload)
            self.assertNotEqual(original.stat().st_ino, imported.stat().st_ino)

    def test_corrupt_cache_fails_before_project_or_home_changes(self):
        archive = self.home / "downloads/sha256" / (locked()["artifact"][0]["sha256"] + ".archive")
        archive.parent.mkdir(parents=True)
        archive.write_bytes(b"corrupt")
        migrator = self.migrator("--apply", "--no-install")
        before = self.snapshot()
        with self.assertRaisesRegex(migration.MigrationError, "checksum mismatch"):
            self.apply(migrator)
        self.assertEqual(self.snapshot(), before)

    def test_failed_install_restores_old_venv_and_allows_retry(self):
        self.config["tools"] = {"python": "3.13.0"}
        save(self.project / "pinset.toml", self.config)
        save(self.project / "pinset.lock", {"schema": 5, "tool": [locked("python", "3.13.0", "3.13.0")]})
        old = self.project / ".venv"
        old.mkdir()
        (old / "package.txt").write_text("old installed packages")
        migrator = self.migrator("--apply", "--backup-venv")

        def fail(root, *arguments):
            if arguments == ("install",):
                old.mkdir()
                (old / "partial").write_text("failed v3 package")
                raise migration.MigrationError("fixture install failure")
            return self.fake_cli(root, *arguments)

        with self.assertRaisesRegex(migration.MigrationError, "fixture install failure"):
            self.apply(migrator, fail)
        self.assertEqual((old / "package.txt").read_text(), "old installed packages")
        self.assertFalse((self.project / ".pinset/config.toml").exists())
        self.assertTrue(list(self.project.glob(".venv.pinset-v3-failed-*")))
        retry = self.migrator("--apply", "--no-install")
        self.apply(retry)
        self.assertTrue((self.project / ".pinset/config.toml").exists())

    def test_existing_venv_requires_explicit_choice(self):
        self.config["tools"] = {"python": "3.13.0"}
        save(self.project / "pinset.toml", self.config)
        save(self.project / "pinset.lock", {"schema": 5, "tool": [locked("python", "3.13.0", "3.13.0")]})
        (self.project / ".venv").mkdir()
        with self.assertRaisesRegex(migration.MigrationError, "--backup-venv"):
            self.migrator("--apply").plan()
        self.migrator("--apply", "--no-install").plan()

    def test_interruption_marker_is_not_mistaken_for_completed_migration(self):
        state = self.project / ".pinset"
        save(state / "config.toml", {"protocol": "pinset/3", "schema": 3})
        (state / "local").mkdir()
        (state / "local/migration-v2-incomplete.json").write_text("{}")
        with self.assertRaisesRegex(migration.MigrationError, "interrupted migration"):
            self.migrator().plan()

    def test_changed_legacy_file_is_detected_before_backup(self):
        migrator = self.migrator("--apply", "--no-install")
        migrator.plan()
        with (self.project / "pinset.toml").open("a") as stream:
            stream.write("# changed\n")
        with patch.object(migrator, "cli", side_effect=self.fake_cli):
            with self.assertRaisesRegex(migration.MigrationError, "source changed"):
                migrator.apply()
        self.assertIsNone(migrator.backup)

    def test_existing_v3_transaction_is_not_implicitly_recovered(self):
        pending = self.home / "v3/global/local/transaction.json"
        pending.parent.mkdir(parents=True)
        pending.write_text('{"phase":"prepared"}')
        migrator = self.migrator("--apply", "--no-install")
        before = self.snapshot()
        with self.assertRaisesRegex(migration.MigrationError, "existing 3.x transaction"):
            self.apply(migrator)
        self.assertEqual(self.snapshot(), before)

    def test_credentials_are_copied_in_memory_and_never_appear_in_files(self):
        record = {"id": "identity-one", "recipient": "age1publicrecipient", "backend": "keyring"}
        save(self.home / "state/identities.toml", {"schema": 1, "identities": [record]})
        secret = b"AGE-SECRET-KEY-1FIXTUREPRIVATEKEY"
        entries = {(migration.OLD_SERVICE, record["id"]): secret}

        class Credentials:
            def get(self, service, account):
                return entries.get((service, account))

            def put(self, service, account, value):
                entries[service, account] = value

        migrator = self.migrator("--apply", "--no-install")
        with patch.object(migration, "NativeCredentials", Credentials):
            self.apply(migrator)
        self.assertEqual(entries[migration.NEW_SERVICE, record["id"]], secret)
        self.assertEqual(entries[migration.OLD_SERVICE, record["id"]], secret)
        self.assertEqual(migration.read_toml(self.home / "v3/state/identities.toml")["schema"], 3)
        self.assertTrue(all(secret not in content for content in self.snapshot().values()))

    def test_missing_native_identity_fails_before_writes(self):
        save(self.home / "state/identities.toml", {"schema": 1, "identities": [
            {"id": "identity-one", "recipient": "age1recipient", "backend": "keyring"}]})
        migrator = self.migrator("--apply", "--no-install")
        before = self.snapshot()
        with patch.object(migration.NativeCredentials, "get", return_value=None), \
                patch.object(migration.NativeCredentials, "__init__", return_value=None):
            with self.assertRaisesRegex(migration.MigrationError, "private identity unavailable"):
                self.apply(migrator)
        self.assertEqual(self.snapshot(), before)

    def test_relative_home_is_rejected(self):
        args = migration.parser().parse_args(["--home", "relative"])
        with self.assertRaisesRegex(migration.MigrationError, "must be absolute"):
            migration.Migrator(args).plan()

    def test_distinct_source_and_destination_homes(self):
        destination = self.root / "new-home"
        migrator = self.migrator("--apply", "--no-install", "--v3-home", str(destination))
        self.apply(migrator)
        self.assertEqual(migrator.v3, destination / "v3")
        self.assertTrue((self.project / ".pinset/config.toml").is_file())
        self.assertTrue(migrator.backup.is_relative_to(self.home))

    @unittest.skipIf(os.name == "nt", "Unix defaults")
    def test_unix_defaults_follow_each_major_versions_layout(self):
        with patch.dict(os.environ, {}, clear=True), patch.object(Path, "home", return_value=self.root):
            self.assertEqual(migration.default_legacy_home(), str(self.root / ".local/share/pinset"))
            self.assertEqual(migration.default_v3_home(), str(self.root / ".pinset"))
        with patch.dict(os.environ, {"XDG_DATA_HOME": str(self.root / "xdg")}, clear=True):
            self.assertEqual(migration.default_legacy_home(), str(self.root / "xdg/pinset"))
        with patch.dict(os.environ, {"PINSET_HOME": str(self.home)}, clear=True):
            self.assertEqual(migration.default_legacy_home(), str(self.home))
            self.assertEqual(migration.default_v3_home(), str(self.home))

    def test_rust_components_targets_and_date_are_kept_in_config_and_lock(self):
        options = {"profile": "minimal", "components": ["rustfmt", "clippy"],
                   "targets": ["wasm32-unknown-unknown"], "date": "2026-07-16"}
        self.config["schema"] = 5
        self.config["tools"] = {"rust": "nightly-2026-07-16"}
        self.config["tool-options"] = {"rust": options}
        record = locked("rust", "nightly-2026-07-16", "1.97.1")
        record["options"] = {"profile": "minimal", "components": "clippy,rustfmt",
                             "targets": "wasm32-unknown-unknown", "date": "2026-07-16"}
        save(self.project / "pinset.toml", self.config)
        save(self.project / "pinset.lock", {"schema": 5, "tool": [record]})
        migrator = self.migrator()
        migrator.plan()
        self.assertEqual(migrator.scopes[0]["config"]["rust"], options)
        record["options"]["components"] = "rustfmt"
        save(self.project / "pinset.lock", {"schema": 5, "tool": [record]})
        with self.assertRaisesRegex(migration.MigrationError, "Rust options"):
            self.migrator().plan()

    def test_linux_native_credentials_use_service_username_and_stdin(self):
        with patch.object(sys, "platform", "linux"), patch.object(migration.shutil, "which", return_value="/usr/bin/secret-tool"), \
                patch.object(migration.subprocess, "run") as run:
            run.return_value = subprocess.CompletedProcess([], 0, stdout=b"private-fixture\n", stderr=b"")
            backend = migration.NativeCredentials()
            self.assertEqual(backend.get("service", "account"), b"private-fixture")
            self.assertEqual(run.call_args.args[0], ["secret-tool", "lookup", "service", "service", "username", "account"])
            backend.put("service", "account", b"private-fixture")
            self.assertNotIn("private-fixture", " ".join(run.call_args.args[0]))
            self.assertEqual(run.call_args.kwargs["input"], b"private-fixture")

    def test_macos_native_credentials_use_bytes_and_free_native_buffers(self):
        api = Mock()
        buffer = migration.ctypes.create_string_buffer(b"private-fixture")

        def lookup(_keychain, _service_len, _service, _account_len, _account, size, pointer, _item):
            size._obj.value = len(b"private-fixture")
            pointer._obj.value = migration.ctypes.addressof(buffer)
            return 0

        api.SecKeychainFindGenericPassword.side_effect = lookup
        api.SecKeychainAddGenericPassword.return_value = 0
        with patch.object(sys, "platform", "darwin"), patch.object(migration.ctypes, "CDLL", return_value=api):
            backend = migration.NativeCredentials()
            self.assertEqual(backend.get("service", "account"), b"private-fixture")
            api.SecKeychainItemFreeContent.assert_called_once()
            backend.put("service", "account", b"private-fixture")
            self.assertEqual(api.SecKeychainAddGenericPassword.call_args.args[5], len(b"private-fixture"))

    def test_explicit_workspace_member_inherits_unselected_parent(self):
        self.config["schema"] = 5
        self.config["workspace"] = {"members": ["member"]}
        save(self.project / "pinset.toml", self.config)
        member = self.project / "member"
        save(member / "pinset.toml", {"schema": 5, "project-id": "member-one", "tools": {}})
        save(member / "pinset.lock", {"schema": 5, "tool": [locked()]})
        args = migration.parser().parse_args(["--home", str(self.home), "--v3-home", str(self.home),
                                             "--project", str(member)])
        migrator = migration.Migrator(args)
        migrator.plan()
        self.assertEqual(len(migrator.scopes), 1)
        self.assertEqual(migrator.scopes[0]["config"]["tools"], {"node": "lts"})


@unittest.skipIf(os.name == "nt", "fake executable fixtures use a Unix shell")
class UpgradeTests(unittest.TestCase):
    setUp = MigrationTests.setUp
    snapshot = MigrationTests.snapshot

    def setUp(self):
        MigrationTests.setUp(self)
        self.destination = self.root / "new-home"
        self.old_bin = self.root / "old-bin"
        self.old_bin.mkdir()
        self.source_bin = self.root / "v3-source"
        self.source_bin.mkdir()
        self.old_cli = self.executable(self.old_bin / "pinset", b"#!/bin/sh\nprintf 'pinset 2.16.0\\n'\n")
        self.old_shim = self.executable(self.old_bin / "pinset-shim", b"old shim bytes")
        (self.old_bin / "node").symlink_to(self.old_shim)
        (self.old_bin / "dotnet").symlink_to(self.old_shim)
        self.foreign = self.executable(self.old_bin / "custom", b"user executable")
        self.new_cli = self.executable(self.source_bin / "pinset",
            f"#!/bin/sh\nprintf 'pinset {migration.DEFAULT_VERSION}\\n'\n".encode())
        self.new_shim = self.executable(self.source_bin / "pinset-shim", b"new shim bytes")
        save(self.home / "state/projects/one.toml", {"schema": 1, "config": str(self.project / "pinset.toml")})
        self.home.joinpath("installs/node/old/bin").mkdir(parents=True)
        self.home.joinpath("installs/node/old/bin/node").write_bytes(b"SDK recovery data")

    @staticmethod
    def executable(path, content):
        path.write_bytes(content)
        path.chmod(0o755)
        return path

    def upgrader(self, *flags):
        args = migration.parser().parse_args(["--home", str(self.home), "--v3-home", str(self.destination),
            "--legacy-pinset", str(self.old_cli), "--pinset", str(self.new_cli), *flags])
        return migration.Upgrader(args)

    def fake_cli(self, upgrader, root, *arguments):
        self.calls.append(arguments)
        if arguments == ("self", "info"):
            return {"protocol": "pinset/3", "version": migration.DEFAULT_VERSION}
        if arguments == ("self", "repair"):
            directory = upgrader.v3 / "bin"
            directory.mkdir(parents=True, exist_ok=True)
            for name in ["pinset", "pinset-shim"]:
                migration.shutil.copy2(self.source_bin / name, directory / name)
            (directory / "node").write_bytes(self.new_shim.read_bytes())
            (upgrader.v3 / ".pinset-home.json").write_text('{"protocol":"pinset/3"}')
        return {"protocol": "pinset/3"}

    def apply(self, upgrader, cli=None):
        upgrader.plan()
        with patch.object(upgrader, "cli", side_effect=cli or (lambda root, *args: self.fake_cli(upgrader, root, *args))):
            upgrader.apply()

    def test_default_apply_installs_migrates_checks_activates_and_cleans(self):
        payload = b"fixture archive"
        archive = self.home / "downloads/sha256" / (hashlib.sha256(payload).hexdigest() + ".archive")
        archive.parent.mkdir(parents=True)
        archive.write_bytes(payload)
        before = (self.project / "pinset.toml").read_bytes()
        upgrader = self.upgrader()
        self.apply(upgrader)
        self.assertEqual(upgrader.phase, "complete")
        self.assertTrue(upgrader.cleanup_complete)
        self.assertIn(("install",), self.calls)
        self.assertIn(("check", "--deep"), self.calls)
        self.assertLess(self.calls.index(("self", "repair")), self.calls.index(("install",)))
        self.assertEqual(migration.Upgrader.binary_version(self.old_cli), migration.DEFAULT_VERSION)
        self.assertIn(str(upgrader.v3 / "bin/node"), (self.old_bin / "node").read_text())
        self.assertFalse((self.old_bin / "dotnet").exists())
        self.assertEqual(self.foreign.read_bytes(), b"user executable")
        self.assertFalse((self.project / "pinset.toml").exists())
        self.assertFalse((self.project / "pinset.lock").exists())
        self.assertFalse(self.home.exists())
        self.assertEqual((upgrader.backup / "legacy-home/installs/node/old/bin/node").read_bytes(), b"SDK recovery data")
        self.assertTrue(any(path.read_bytes() == before for path in upgrader.backup.glob("*-pinset.toml")))
        self.assertEqual((upgrader.v3 / "cache/downloads/sha256" / archive.name).read_bytes(), payload)
        self.assertFalse(upgrader.journal.exists())

    def test_preview_does_not_install_or_change_anything(self):
        before = self.snapshot()
        upgrader = self.upgrader()
        with patch.object(upgrader, "download") as download:
            report = upgrader.plan()
        download.assert_not_called()
        self.assertTrue(report["upgrade"]["cleanup"])
        self.assertEqual(self.snapshot(), before)

    def test_install_or_deep_check_failure_preserves_v2_entries_and_sources(self):
        for failed_command in [("install",), ("check", "--deep")]:
            with self.subTest(failed_command=failed_command):
                upgrader = self.upgrader()
                def fail(root, *args):
                    if args == failed_command:
                        raise migration.MigrationError("fixture validation failure")
                    return self.fake_cli(upgrader, root, *args)
                with self.assertRaisesRegex(migration.MigrationError, "fixture validation failure"):
                    self.apply(upgrader, fail)
                self.assertEqual(migration.Upgrader.binary_version(self.old_cli), "2.16.0")
                self.assertTrue((self.project / "pinset.toml").exists())
                self.assertTrue((self.home / "installs/node/old/bin/node").exists())
                self.assertFalse((self.project / ".pinset/config.toml").exists())
                self.assertFalse(upgrader.journal.exists())

    def test_activation_failure_rolls_back_all_old_entries(self):
        upgrader = self.upgrader()
        original = upgrader.publish_entry
        def fail(path, target):
            if path == self.old_cli:
                raise migration.MigrationError("fixture publication failure")
            original(path, target)
        with patch.object(upgrader, "publish_entry", side_effect=fail):
            with self.assertRaisesRegex(migration.MigrationError, "fixture publication failure"):
                self.apply(upgrader)
        self.assertEqual(migration.Upgrader.binary_version(self.old_cli), "2.16.0")
        self.assertEqual(self.old_shim.read_bytes(), b"old shim bytes")
        self.assertEqual(os.readlink(self.old_bin / "node"), str(self.old_shim))
        self.assertTrue((self.home / "state/projects/one.toml").exists())

    def test_retry_after_activation_failure_uses_verified_scope_receipts(self):
        upgrader = self.upgrader()
        with patch.object(upgrader, "activate", side_effect=migration.MigrationError("activation failed")):
            with self.assertRaises(migration.MigrationError):
                self.apply(upgrader)
        second = self.upgrader()
        self.apply(second)
        self.assertEqual(second.phase, "complete")
        self.assertEqual(len(second.existing_scopes), 1)
        self.assertTrue(second.cleanup_complete)
        self.assertFalse((self.project / "pinset.toml").exists())

    def test_workspace_retry_does_not_skip_unfinished_members_before_cleanup(self):
        self.config["schema"] = 5
        self.config["workspace"] = {"members": ["member"]}
        save(self.project / "pinset.toml", self.config)
        member = self.project / "member"
        save(member / "pinset.toml", {"schema": 5, "project-id": "member-one", "tools": {}})
        save(member / "pinset.lock", {"schema": 5, "tool": [locked()]})
        upgrader = self.upgrader()
        def fail(root, *arguments):
            if root == member and arguments == ("install",):
                raise migration.MigrationError("member install failed")
            return self.fake_cli(upgrader, root, *arguments)
        with self.assertRaisesRegex(migration.MigrationError, "member install failed"):
            self.apply(upgrader, fail)
        self.assertTrue((self.project / ".pinset/config.toml").exists())
        self.assertFalse((member / ".pinset/config.toml").exists())
        retry = self.upgrader()
        self.apply(retry)
        self.assertTrue((member / ".pinset/config.toml").exists())
        self.assertFalse((member / "pinset.toml").exists())
        self.assertFalse((self.project / "pinset.toml").exists())
        self.assertTrue(retry.cleanup_complete)

    def test_source_changes_during_validation_block_activation_and_cleanup(self):
        upgrader = self.upgrader()
        def change(root, *arguments):
            if arguments == ("check", "--deep"):
                with (self.project / "pinset.toml").open("ab") as stream:
                    stream.write(b"# user edit during upgrade\n")
            return self.fake_cli(upgrader, root, *arguments)
        with self.assertRaisesRegex(migration.MigrationError, "changed before activation"):
            self.apply(upgrader, change)
        self.assertEqual(migration.Upgrader.binary_version(self.old_cli), "2.16.0")
        self.assertTrue((self.home / "installs/node/old/bin/node").exists())

    def test_partial_options_require_keep_v2_and_keep_mode_does_not_cleanup(self):
        for flag in ["--no-install", "--no-global", "--skip-identities", "--skip-unsupported"]:
            with self.assertRaisesRegex(migration.MigrationError, "require --keep-v2"):
                self.upgrader(flag).plan()
        upgrader = self.upgrader("--keep-v2", "--no-install")
        self.apply(upgrader)
        self.assertFalse(upgrader.cleanup_complete)
        self.assertTrue((self.project / "pinset.toml").exists())
        self.assertTrue((self.home / "installs/node/old/bin/node").exists())
        self.assertEqual(migration.Upgrader.binary_version(self.old_cli), "2.16.0")

    def test_explicit_v2_binary_is_rejected_with_clear_version_error(self):
        upgrader = self.upgrader("--pinset", str(self.old_cli))
        with self.assertRaisesRegex(migration.MigrationError, "Pinset 2.16.0.*install 3.x automatically"):
            self.apply(upgrader)
        self.assertEqual(self.calls, [])
        self.assertTrue((self.project / "pinset.toml").exists())

    def test_release_checksum_and_archive_paths_are_checked_before_execution(self):
        upgrader = self.upgrader()
        upgrader.args.pinset = None
        def download(_url, target):
            if target.name == "SHA256SUMS":
                target.write_text("0" * 64 + f"  pinset-v{migration.DEFAULT_VERSION}-linux-x86_64.zip\n")
            else:
                target.write_bytes(b"corrupt ZIP")
        with tempfile.TemporaryDirectory() as temporary, patch.object(upgrader, "download", side_effect=download), \
                patch.object(upgrader, "release_platform", return_value="linux-x86_64"):
            with self.assertRaisesRegex(migration.MigrationError, "checksum mismatch"):
                upgrader.stage_v3(Path(temporary).resolve())
        def unsafe_download(_url, target):
            stream = io.BytesIO()
            with zipfile.ZipFile(stream, "w") as archive:
                archive.writestr("../pinset", b"unsafe")
                archive.writestr("pinset-shim", b"shim")
            content = stream.getvalue()
            if target.name == "SHA256SUMS":
                target.write_text(hashlib.sha256(content).hexdigest() + f"  pinset-v{migration.DEFAULT_VERSION}-linux-x86_64.zip\n")
            else:
                target.write_bytes(content)
        with tempfile.TemporaryDirectory() as temporary, patch.object(upgrader, "download", side_effect=unsafe_download), \
                patch.object(upgrader, "release_platform", return_value="linux-x86_64"):
            with self.assertRaisesRegex(migration.MigrationError, "exactly the CLI"):
                upgrader.stage_v3(Path(temporary).resolve())

    def test_default_download_stages_verified_pair_without_calling_old_cli_with_json(self):
        upgrader = self.upgrader()
        upgrader.args.pinset = None
        stream = io.BytesIO()
        with zipfile.ZipFile(stream, "w") as archive:
            archive.writestr("pinset", self.new_cli.read_bytes())
            archive.writestr("pinset-shim", self.new_shim.read_bytes())
        content = stream.getvalue()
        def download(url, target):
            self.assertTrue(url.startswith(f"https://github.com/Future-Element/pinset/releases/download/v{migration.DEFAULT_VERSION}/"))
            if target.name == "SHA256SUMS":
                target.write_text(hashlib.sha256(content).hexdigest() + f"  pinset-v{migration.DEFAULT_VERSION}-linux-x86_64.zip\n")
            else:
                target.write_bytes(content)
        with patch.object(upgrader, "download", side_effect=download), \
                patch.object(upgrader, "release_platform", return_value="linux-x86_64"):
            self.apply(upgrader)
        self.assertEqual(migration.Upgrader.binary_version(self.old_cli), migration.DEFAULT_VERSION)
        self.assertTrue(upgrader.cleanup_complete)

    def test_unknown_existing_v3_configuration_and_changed_receipts_block_cleanup(self):
        save(self.project / ".pinset/config.toml", {"protocol": "pinset/3", "schema": 3})
        with self.assertRaisesRegex(migration.MigrationError, "no migration receipt"):
            self.upgrader().plan()
        self.upgrader("--keep-v2").plan()

    def test_interrupted_activation_cleanup_is_reported_before_new_work(self):
        upgrader = self.upgrader()
        upgrader.journal.parent.mkdir(parents=True)
        upgrader.journal.write_text('{"phase":"cleaning"}')
        with self.assertRaisesRegex(migration.MigrationError, "interrupted activation/cleanup"):
            upgrader.plan()

    def test_python_package_failure_restores_original_and_does_not_activate(self):
        self.config["tools"] = {"python": "3.13.0"}
        save(self.project / "pinset.toml", self.config)
        save(self.project / "pinset.lock", {"schema": 5, "tool": [locked("python", "3.13.0", "3.13.0")]})
        (self.project / ".venv").mkdir()
        (self.project / ".venv/package.txt").write_text("original package")
        upgrader = self.upgrader()
        with patch.object(upgrader, "python_packages", return_value=b"example==1.0\n"), \
                patch.object(upgrader, "install_python_packages", side_effect=migration.MigrationError("package restoration failed")):
            with self.assertRaisesRegex(migration.MigrationError, "package restoration failed"):
                self.apply(upgrader)
        self.assertEqual((self.project / ".venv/package.txt").read_text(), "original package")
        self.assertTrue((self.project / "pinset.toml").exists())
        self.assertFalse((self.project / ".pinset/config.toml").exists())
        self.assertEqual(migration.Upgrader.binary_version(self.old_cli), "2.16.0")

    def test_package_inventory_change_and_private_pip_error_output_are_not_accepted(self):
        upgrader = self.upgrader()
        results = [subprocess.CompletedProcess([], 0, stdout=b"", stderr=b""),
                   subprocess.CompletedProcess([], 0, stdout=b"", stderr=b""),
                   subprocess.CompletedProcess([], 0, stdout=b"example==2.0\n", stderr=b"")]
        with patch.object(migration.subprocess, "run", side_effect=results):
            with self.assertRaisesRegex(migration.MigrationError, "inventory changed"):
                upgrader.install_python_packages(self.project / ".venv", self.project, self.root / "requirements", b"example==1.0\n")
        with patch.object(migration.subprocess, "run", return_value=subprocess.CompletedProcess([], 1,
                stdout=b"https://private:password@index.invalid", stderr=b"private credentials")):
            with self.assertRaisesRegex(migration.MigrationError, "restoration failed") as error:
                upgrader.install_python_packages(self.project / ".venv", self.project, self.root / "requirements", b"example==1.0\n")
        self.assertNotIn("password", str(error.exception))

    def test_existing_v3_pair_is_reused_and_explicit_version_must_match_supplied_pair(self):
        upgrader = self.upgrader()
        upgrader.args.pinset = None
        directory = upgrader.v3 / "bin"
        directory.mkdir(parents=True)
        self.executable(directory / "pinset", b"#!/bin/sh\nprintf 'pinset 3.1.0\\n'\n")
        self.executable(directory / "pinset-shim", b"matching fixture shim")
        with tempfile.TemporaryDirectory() as temporary, patch.object(upgrader, "download") as download:
            upgrader.stage_v3(Path(temporary).resolve())
            self.assertEqual(upgrader.binary_version(upgrader.args.pinset), "3.1.0")
        download.assert_not_called()
        upgrader = self.upgrader("--version", "3.1.0")
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(migration.MigrationError, "does not match --version"):
                upgrader.stage_v3(Path(temporary).resolve())

    def test_python_packages_are_restored_and_checked_before_old_environment_cleanup(self):
        self.config["tools"] = {"python": "3.13.0"}
        save(self.project / "pinset.toml", self.config)
        save(self.project / "pinset.lock", {"schema": 5, "tool": [locked("python", "3.13.0", "3.13.0")]})
        (self.project / ".venv").mkdir()
        (self.project / ".venv/package.txt").write_text("recoverable packages")
        upgrader = self.upgrader()
        with patch.object(upgrader, "python_packages", return_value=b"example==1.0\n"), \
                patch.object(upgrader, "install_python_packages") as restore:
            self.apply(upgrader)
        restore.assert_called_once()
        self.assertEqual(restore.call_args.args[3], b"example==1.0\n")
        self.assertTrue(list(self.project.glob(".venv.pinset-v2-*/package.txt")))
        self.assertTrue(upgrader.cleanup_complete)


@unittest.skipUnless(os.environ.get("PINSET_TEST_CLI"), "real v3 CLI is supplied by the Docker verifier")
class RealCliTests(unittest.TestCase):
    setUp = MigrationTests.setUp
    migrator = MigrationTests.migrator

    def test_real_v3_accepts_translated_locks_and_preserves_exact_artifacts(self):
        migrator = self.migrator("--apply", "--no-install", "--pinset", os.environ["PINSET_TEST_CLI"])
        migrator.plan()
        migrator.apply()
        environment = dict(os.environ, PINSET_HOME=str(self.home))
        result = subprocess.run([os.environ["PINSET_TEST_CLI"], "-C", str(self.project),
            "--json", "install", "--plan"], env=environment, capture_output=True, text=True, check=True)
        self.assertTrue(json.loads(result.stdout)["plan"])
        converted = tomllib.loads((self.project / ".pinset/lock.toml").read_text())
        self.assertEqual(converted["tool"][0]["artifact"], locked()["artifact"])

    @unittest.skipIf(os.name == "nt", "fixture Node runtime uses a Unix shell")
    def test_real_global_pnpm_overlay_failure_can_retry_with_same_version_conversion(self):
        binary = os.environ["PINSET_TEST_CLI"]
        destination = self.root / "new-home"
        environment = dict(os.environ, PINSET_HOME=str(destination))
        info = json.loads(subprocess.check_output([binary, "--json", "self", "info"], env=environment))
        target = info["platform"]
        def package(files, compression, prefix):
            stream = io.BytesIO()
            with tarfile.open(fileobj=stream, mode="w:" + compression) as archive:
                for name, content in files.items():
                    entry = tarfile.TarInfo(prefix + "/" + name)
                    entry.size, entry.mode = len(content), 0o755
                    archive.addfile(entry, io.BytesIO(content))
            return stream.getvalue()
        node_payload = package({"bin/node": b"#!/bin/sh\ncase \"$1\" in\n*pnpm.mjs) printf '11.21.0\\n';;\n*) printf 'v22.12.0\\n';;\nesac\n"}, "xz", "node-fixture")
        pnpm_payload = package({"package.json": b'{"name":"pnpm","version":"11.21.0"}',
                                "bin/pnpm.mjs": b"process.stdout.write('11.21.0\\n');\n"}, "gz", "package")
        node = locked(payload=node_payload)
        node["artifact"][0]["target"] = target
        old = pnpm_locked((target,))
        new = pnpm_locked((target,), requested="11.21.0", wrapper=pnpm_payload, javascript=True)
        save(self.home / "state/global.toml", {"schema": 3, "tools": {"node": "lts", "pnpm": "latest"}})
        save(self.home / "state/global.lock", {"schema": 5, "tool": [node, old]})
        for algorithm, payload in [("sha256", node_payload), ("sha512", pnpm_payload)]:
            path = self.home / "downloads" / algorithm / (hashlib.new(algorithm, payload).hexdigest() + ".archive")
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(payload)
        def migrator():
            args = migration.parser().parse_args(["--home", str(self.home), "--v3-home", str(destination),
                                                  "--pinset", binary, "--apply"])
            return migration.Migrator(args)
        first = migrator()
        first.plan()
        with patch.object(first, "convert_pnpm_locks"):
            with self.assertRaisesRegex(migration.MigrationError, "JavaScript pnpm cannot contain native overlays"):
                first.apply()
        self.assertFalse((destination / "v3/global/config.toml").exists())
        self.assertEqual(migration.read_toml(self.home / "state/global.lock")["tool"][1], old)
        retry = migrator()
        retry.plan()
        real_cli = retry.cli
        def resolve(root, *arguments):
            if arguments[0] == "use":
                self.assertEqual(arguments, ("use", "pnpm@11.21.0", "--no-install", "--plan"))
                return {"protocol": "pinset/3", "lock": {"tool": [new]}}
            return real_cli(root, *arguments)
        with patch.object(retry, "cli", side_effect=resolve):
            retry.apply()
        converted = migration.read_toml(destination / "v3/global/lock.toml")["tool"][1]
        self.assertEqual(converted["version"], "11.21.0")
        self.assertEqual(converted["requested"], "latest")
        self.assertFalse(converted["artifact"][0].get("overlay"))
        report = json.loads(subprocess.check_output([binary, "-C", str(self.root), "--json",
            "which", "pnpm", "--global"], env=environment))
        self.assertTrue(any(str(argument).endswith("bin/pnpm.mjs") for argument in report["prefix"]))
        output = subprocess.check_output([binary, "-C", str(self.root), "exec", "--no-env", "--", "pnpm", "--version"], env=environment)
        self.assertEqual(output.strip(), b"11.21.0")
        subprocess.run([binary, "-C", str(self.root), "--json", "check", "--global", "--deep"],
            env=environment, capture_output=True, check=True)

    def test_real_v3_installs_from_migrated_cache_and_routes_exact_version(self):
        binary = os.environ["PINSET_TEST_CLI"]
        environment = dict(os.environ, PINSET_HOME=str(self.home))
        info = json.loads(subprocess.check_output([binary, "--json", "self", "info"], env=environment))
        target = info["platform"]
        if target.startswith("windows-"):
            self.skipTest("fixture runtime uses a Unix shell")
        contents = b"#!/bin/sh\nprintf 'v22.12.0\\n'\n"
        stream = io.BytesIO()
        with tarfile.open(fileobj=stream, mode="w:xz") as archive:
            entry = tarfile.TarInfo("node-fixture/bin/node")
            entry.size = len(contents)
            entry.mode = 0o755
            archive.addfile(entry, io.BytesIO(contents))
        payload = stream.getvalue()
        record = locked(payload=payload)
        record["artifact"][0]["target"] = target
        save(self.project / "pinset.lock", {"schema": 5, "tool": [record]})
        cached = self.home / "downloads/sha256" / (hashlib.sha256(payload).hexdigest() + ".archive")
        cached.parent.mkdir(parents=True)
        cached.write_bytes(payload)
        migrator = self.migrator("--apply", "--offline", "--pinset", binary)
        migrator.plan()
        migrator.apply()
        result = subprocess.check_output([binary, "-C", str(self.project), "exec", "--no-env", "--", "node", "--version"], env=environment)
        self.assertEqual(result.strip(), b"v22.12.0")
        subprocess.run([binary, "-C", str(self.project), "--json", "check", "--deep"],
                       env=environment, capture_output=True, check=True)
        self.assertEqual(cached.read_bytes(), payload)

    @unittest.skipIf(os.name == "nt", "old CLI fixture uses a Unix shell")
    def test_real_standalone_upgrade_activates_old_path_and_cleans_after_routing_check(self):
        binary = os.environ["PINSET_TEST_CLI"]
        destination = self.root / "new-home"
        environment = dict(os.environ, PINSET_HOME=str(destination))
        info = json.loads(subprocess.check_output([binary, "--json", "self", "info"], env=environment))
        contents = b"#!/bin/sh\nprintf 'v22.12.0\\n'\n"
        stream = io.BytesIO()
        with tarfile.open(fileobj=stream, mode="w:xz") as archive:
            entry = tarfile.TarInfo("node-fixture/bin/node")
            entry.size, entry.mode = len(contents), 0o755
            archive.addfile(entry, io.BytesIO(contents))
        payload = stream.getvalue()
        record = locked(payload=payload)
        record["artifact"][0]["target"] = info["platform"]
        save(self.project / "pinset.lock", {"schema": 5, "tool": [record]})
        save(self.home / "state/projects/one.toml", {"schema": 1, "config": str(self.project / "pinset.toml")})
        save(self.home / "state/global.toml", {"schema": 3, "tools": {"node": "lts"}})
        save(self.home / "state/global.lock", {"schema": 5, "tool": [record]})
        cached = self.home / "downloads/sha256" / (hashlib.sha256(payload).hexdigest() + ".archive")
        cached.parent.mkdir(parents=True)
        cached.write_bytes(payload)
        old_bin = self.root / "old-bin"
        old_bin.mkdir()
        old_cli = UpgradeTests.executable(old_bin / "pinset", b"#!/bin/sh\nprintf 'pinset 2.16.0\\n'\n")
        old_shim = UpgradeTests.executable(old_bin / "pinset-shim", b"old shim")
        (old_bin / "node").symlink_to(old_shim)
        result = subprocess.run([sys.executable, "-B", str(SCRIPT), "--home", str(self.home),
            "--v3-home", str(destination), "--legacy-pinset", str(old_cli), "--pinset", binary,
            "--offline", "--apply"], cwd=self.root, env=environment, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(migration.Upgrader.binary_version(old_cli), migration.DEFAULT_VERSION)
        routed = subprocess.check_output([str(old_bin / "node"), "--version"],
            cwd=self.project, env=environment)
        self.assertEqual(routed.strip(), b"v22.12.0")
        subprocess.run([str(old_cli), "-C", str(self.project), "--json", "check", "--deep"],
            env=environment, capture_output=True, check=True)
        subprocess.run([str(old_cli), "-C", str(self.root), "--json", "check", "--global", "--deep"],
            env=environment, capture_output=True, check=True)
        self.assertFalse((self.project / "pinset.toml").exists())
        self.assertFalse((self.project / "pinset.lock").exists())
        self.assertFalse(self.home.exists())
        self.assertTrue((destination / "migration-backups").is_dir())


if __name__ == "__main__":
    unittest.main()
