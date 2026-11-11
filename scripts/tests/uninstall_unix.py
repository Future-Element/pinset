"""Isolated POSIX full-uninstall regressions; run with python3 on Linux/macOS.

Credentials, package managers and fish are stubs. HOME/XDG/PATH are private.
Never execute against the invoking user's Pinset installation.
"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[2] / "uninstall.sh"


class UninstallTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="pinset-uninstall-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.user = self.root / "user"
        self.user.mkdir()
        self.home = self.user / ".pinset"
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.log = self.root / "actions.log"
        self.env = dict(os.environ)
        for name in list(self.env):
            if name.startswith("PINSET_"):
                self.env.pop(name)
        self.env.update(HOME=str(self.user), PINSET_HOME=str(self.home),
                        XDG_DATA_HOME=str(self.user / ".local/share"),
                        XDG_CONFIG_HOME=str(self.user / ".config"),
                        PATH=str(self.bin) + ":/usr/bin:/bin",
                        ACTION_LOG=str(self.log))
        for name, code in {
            "secret-tool": 'printf "secret-tool %s\\n" "$*" >>"$ACTION_LOG"\n',
            "brew": '[ "$1" != list ] || exit 1\nprintf "brew %s\\n" "$*" >>"$ACTION_LOG"\n',
            "fish": 'printf "fish invoked\\n" >>"$ACTION_LOG"\n',
        }.items():
            self.write(self.bin / name, "#!/bin/sh\n" + code, executable=True)
        self.write(self.home / "v3/.pinset-home.json", '{"protocol":"pinset/3"}')
        self.write(self.home / "v3/installs/node/node", "sdk")
        self.write(self.home / "v3/state/identities.toml", 'schema = 3\n[[identities]]\nid = "fixture"\n')
        self.write(self.user / ".bashrc", f'export PINSET_HOME="{self.home}"\nexport PATH=\'{self.home}/v3/bin\':"$PATH"\nexport JAVA_HOME="{self.home}/v3/installs/java"\nexport JAVA_SYSTEM="/opt/java"\n')
        self.write(self.bin / "pinset", "cli")
        self.write(self.bin / "pinset-shim", "router")
        self.write(self.bin / "node", "router")
        self.write(self.bin / "unrelated", "keep")
        # The script also inventories standard shared command locations.
        for path in (Path("/usr/local/bin/pinset"), Path("/usr/local/bin/pinset-shim"),
                     Path("/usr/bin/pinset"), Path("/usr/bin/pinset-shim")):
            if path.exists():
                self.skipTest("shared Pinset executable exists; refuse this fixture")

    def write(self, path, data, executable=False):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(data)
        if executable:
            path.chmod(0o755)
        return path

    def run_script(self, *args, expected=0, pipe=False):
        if pipe:
            result = subprocess.run(["sh", "-c", 'curl --fail --silent --show-error "$1" | sh -s -- --yes',
                                     "fixture", SCRIPT.as_uri()], env=self.env,
                                    text=True, capture_output=True)
        else:
            result = subprocess.run(["sh", str(SCRIPT), *args], env=self.env,
                                    text=True, capture_output=True)
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        return result

    def snapshot(self):
        return {str(p.relative_to(self.root)): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in self.root.rglob("*") if p.is_file() and not p.is_symlink()}

    def test_plan_is_read_only(self):
        before = self.snapshot()
        self.run_script("--plan")
        self.assertEqual(before, self.snapshot())
        self.assertFalse(self.log.exists())

    def test_complete_and_idempotent(self):
        self.run_script("--yes")
        self.assertFalse(self.home.exists())
        self.assertFalse((self.bin / "pinset").exists())
        self.assertFalse((self.bin / "pinset-shim").exists())
        self.assertFalse((self.bin / "node").exists())
        self.assertEqual((self.bin / "unrelated").read_text(), "keep")
        self.assertEqual((self.user / ".bashrc").read_text(), 'export JAVA_SYSTEM="/opt/java"\n')
        self.assertIn("clear service dev.pinset.identity.v3", self.log.read_text())
        self.run_script("--yes")

    def test_registered_projects_and_named_legacy_venvs(self):
        project = self.root / "registered project"
        self.write(project / ".pinset/config.toml", 'protocol = "pinset/3"\nproject_id = "fixture"\n')
        self.write(project / ".venv/.pinset-owner.toml", f'protocol = "pinset/3"\nproject_id = "fixture"\n[directory]\nroot = "{project}"\n')
        self.write(project / ".venv/pyvenv.cfg", "home = sdk")
        self.write(project / "nested/custom-env/.pinset-venv.toml", f'schema = 2\n[directory]\nroot = "{project}"\n')
        self.write(project / "nested/custom-env/pyvenv.cfg", "home = sdk")
        self.write(project / "source.txt", "keep")
        self.write(self.home / "v3/state/projects.json",
                   json.dumps({str(project): str(project / ".pinset/config.toml")}))
        self.run_script("--yes")
        self.assertFalse((project / ".pinset").exists())
        self.assertFalse((project / ".venv").exists())
        self.assertFalse((project / "nested/custom-env").exists())
        self.assertEqual((project / "source.txt").read_text(), "keep")

    def test_curl_pipeline(self):
        self.run_script(pipe=True)
        self.assertFalse(self.home.exists())

    def test_legacy_home_and_migration_backups(self):
        self.write(self.home / "state/global.toml", "schema = 2\n[tools]\n")
        self.write(self.home / "installs/go/go", "sdk")
        self.write(self.home / "migration-backups/legacy-home/installs/python", "sdk")
        self.run_script("--yes")
        self.assertFalse(self.home.exists())

    def test_unproven_sibling_preserved(self):
        self.write(self.home / "unrelated.txt", "keep")
        self.run_script("--yes", expected=1)
        self.assertEqual((self.home / "unrelated.txt").read_text(), "keep")

    def test_unsafe_homes_and_links(self):
        before = self.snapshot()
        self.run_script("--yes", "--pinset-home", str(self.user), expected=1)
        self.assertEqual(before, self.snapshot())
        link = self.root / "linked"
        link.symlink_to(self.home, target_is_directory=True)
        self.run_script("--yes", "--pinset-home", str(link), expected=1)
        self.assertTrue((self.home / "v3/installs/node/node").exists())

    def test_ambiguous_profile_survives(self):
        self.write(self.user / ".bashrc", 'export PINSET_NO_ENV=1; export KEEP=1\n')
        self.run_script("--yes", expected=1)
        self.assertIn("export KEEP=1", (self.user / ".bashrc").read_text())

    def test_unmarked_v3_rejected(self):
        (self.home / "v3/.pinset-home.json").unlink()
        self.run_script("--yes", expected=1)
        self.assertTrue((self.home / "v3/installs/node/node").exists())

    def test_marker_is_not_enough_to_delete_source_tree(self):
        project = self.root / "source project"
        self.write(project / ".pinset/config.toml", 'protocol = "pinset/3"\nproject_id = "fixture"\n')
        self.write(project / ".pinset-owner.toml", f'protocol = "pinset/3"\nproject_id = "fixture"\nroot = "{project}"\n')
        self.write(project / "fixture/.pinset-owner.toml", 'schema = 3\n')
        self.write(project / "source.txt", "keep")
        self.run_script("--yes", "--project", str(project), expected=1)
        self.assertEqual((project / "source.txt").read_text(), "keep")
        self.assertTrue((project / "fixture/.pinset-owner.toml").exists())

    def test_mixed_path_and_prefix_sibling_are_preserved(self):
        content = f'export PATH=\'{self.home}/v3/bin:/keep\':"$PATH"\nexport JAVA_HOME="{self.home}-other/java"\n'
        self.write(self.user / ".bashrc", content)
        self.run_script("--yes", expected=1)
        self.assertEqual((self.user / ".bashrc").read_text(), content)

    def test_custom_home_declared_only_in_profile(self):
        custom = self.root / "custom home"
        self.write(custom / "v3/.pinset-home.json", '{"protocol":"pinset/3"}')
        self.write(custom / "v3/installs/sdk", "sdk")
        self.write(self.user / ".bashrc", f'export PINSET_HOME="{custom}"\n')
        self.run_script("--yes")
        self.assertFalse(custom.exists())

    def test_empty_credential_store_is_not_an_error(self):
        self.write(self.bin / "secret-tool", "#!/bin/sh\nexit 1\n", executable=True)
        self.run_script("--yes")
        self.assertFalse(self.home.exists())

    def test_linked_binary_ancestor_rejected(self):
        external = self.root / "external"
        self.write(external / "nested/pinset", "keep")
        link = self.root / "linked"
        link.symlink_to(external, target_is_directory=True)
        self.run_script("--yes", "--install-dir", str(link / "nested"), expected=1)
        self.assertEqual((external / "nested/pinset").read_text(), "keep")

    def test_mixed_runtime_path_and_fish_arguments_preserved(self):
        content = f'export PYTHONPATH="{self.home}/v3/installs/python:/keep"\nfish_add_path \'{self.home}/v3/bin\' /keep\n'
        self.write(self.user / ".bashrc", content)
        self.run_script("--yes", expected=1)
        self.assertEqual((self.user / ".bashrc").read_text(), content)

    def test_unknown_home_expression_preserved(self):
        content = 'export PINSET_HOME="${CUSTOM_HOME}/pinset"\n'
        self.write(self.user / ".bashrc", content)
        self.run_script("--yes", expected=1)
        self.assertEqual((self.user / ".bashrc").read_text(), content)

    def test_stale_path_removed_after_data_already_gone(self):
        shutil.rmtree(self.home)
        self.run_script("--yes")
        self.assertNotIn(".pinset", (self.user / ".bashrc").read_text())

    def test_unrelated_global_file_does_not_prove_ownership(self):
        other = self.root / "other"
        self.write(other / "state/global.toml", "unrelated = true\n")
        self.write(other / "installs/keep", "keep")
        self.run_script("--yes", "--pinset-home", str(other), expected=1)
        self.assertEqual((other / "installs/keep").read_text(), "keep")

    def test_mixed_eval_hook_is_preserved(self):
        content = 'eval "$(pinset self shell)$(other-tool hook)"\n'
        self.write(self.user / ".bashrc", content)
        self.run_script("--yes", expected=1)
        self.assertEqual((self.user / ".bashrc").read_text(), content)

    def test_mac_keychain_error_preserves_metadata(self):
        self.write(self.bin / "uname", "#!/bin/sh\nprintf '%s\\n' Darwin\n", executable=True)
        self.write(self.bin / "security", "#!/bin/sh\nexit 36\n", executable=True)
        self.run_script("--yes", expected=1)
        self.assertTrue((self.home / "v3/state/identities.toml").exists())

    def test_mac_keychain_absence_is_allowed(self):
        self.write(self.bin / "uname", "#!/bin/sh\nprintf '%s\\n' Darwin\n", executable=True)
        self.write(self.bin / "security", "#!/bin/sh\nexit 44\n", executable=True)
        self.run_script("--yes")
        self.assertFalse(self.home.exists())

    def test_missing_credentials_tool_preserves_metadata(self):
        (self.bin / "secret-tool").unlink()
        if shutil.which("secret-tool", path="/usr/bin:/bin"):
            self.skipTest("system secret-tool exists")
        self.run_script("--yes", expected=1)
        self.assertTrue((self.home / "v3/state/identities.toml").exists())


if __name__ == "__main__":
    unittest.main(verbosity=2)
