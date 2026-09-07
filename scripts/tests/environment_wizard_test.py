#!/usr/bin/env python3
"""Exercise the real wizard in a native terminal on disposable CI runners."""
import errno
import os
from pathlib import Path
import re
import select
import signal
import subprocess
import sys
import tempfile
import time
import tomllib

if os.name != "nt":
    import pty


def interact_windows(binary, project, environment, arguments, replies):
    from winpty import PtyProcess

    process = PtyProcess.spawn(
        [str(binary), *arguments], cwd=str(project), env=environment,
        dimensions=(40, 240), backend=1,
    )
    pending = ""
    replies = list(replies)
    deadline = time.monotonic() + 90
    try:
        while time.monotonic() < deadline:
            readable, _, _ = select.select([process.fileobj], [], [], 0.2)
            if not readable:
                if not process.isalive():
                    break
                continue
            try:
                pending += process.read(4096)
            except EOFError:
                break
            pending = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", pending)
            if len(pending) > 65536:
                raise AssertionError("wizard produced excessive terminal output")
            while replies and replies[0][0] in pending:
                prompt, response = replies.pop(0)
                pending = pending.split(prompt, 1)[1]
                process.write(response + "\r\n")
        else:
            raise AssertionError("Windows wizard timed out")
        while process.isalive() and time.monotonic() < deadline:
            time.sleep(0.05)
        assert process.exitstatus == 0, "interactive Windows command failed"
        assert not replies, "command exited before all expected prompts"
    finally:
        process.close(force=True)


def interact(binary, project, environment, arguments, replies):
    if os.name == "nt":
        return interact_windows(binary, project, environment, arguments, replies)
    pid, terminal = pty.fork()
    if pid == 0:
        os.chdir(project)
        os.execve(str(binary), [str(binary), *arguments], environment)
    pending = bytearray()
    replies = list(replies)
    deadline = time.monotonic() + 90
    finished = False
    try:
        while time.monotonic() < deadline:
            readable, _, _ = select.select([terminal], [], [], 1)
            if not readable:
                continue
            try:
                chunk = os.read(terminal, 4096)
            except OSError as error:
                if error.errno == errno.EIO:
                    break
                raise
            if not chunk:
                break
            pending.extend(chunk)
            if len(pending) > 65536:
                raise AssertionError("wizard produced excessive terminal output")
            if replies and replies[0][0].encode() in pending:
                prompt, response = replies.pop(0)
                position = pending.index(prompt.encode()) + len(prompt.encode())
                del pending[:position]
                os.write(terminal, response.encode() + b"\n")
        else:
            raise AssertionError("wizard timed out")
        _, status = os.waitpid(pid, 0)
        finished = True
        assert os.waitstatus_to_exitcode(status) == 0, "interactive command failed"
        assert not replies, "command exited before all expected prompts"
    finally:
        if not finished:
            os.kill(pid, signal.SIGKILL)
            os.waitpid(pid, 0)
        os.close(terminal)


binary = Path(sys.argv[1]).resolve(strict=True)
with tempfile.TemporaryDirectory(prefix="pinset-wizard-") as temporary:
    root = Path(temporary)
    project = root / "project with spaces"
    project.mkdir()
    environment = os.environ.copy()
    for name in (
        "PINSET_IDENTITY", "PINSET_IDENTITY_FILE", "PINSET_ENV_PROFILE",
        "PINSET_ENV_DISABLE", "CI", "GITHUB_ACTIONS", "GITLAB_CI", "TF_BUILD",
    ):
        environment.pop(name, None)
    environment.update(PINSET_HOME=str(root / "home"), PINSET_LANG="en")
    subprocess.run([str(binary), "init"], cwd=project, env=environment, check=True,
                   stdout=subprocess.DEVNULL)
    config_path = project / "pinset.toml"
    config_path.write_text(config_path.read_text().replace(
        "system-fallback = false", "system-fallback = true"))
    identity = root / "identity.age"
    recovery = root / "recovery.age"
    # These passphrases protect disposable test identities only.
    interact(binary, project, environment, ["env", "init", "--identity-file", str(identity)], [
        ("Profile [dev]: ", ""),
        ("skip recovery): ", str(recovery)),
        ("Device identity passphrase: ", "wizard-device-fixture"),
        ("Confirm passphrase: ", "wizard-device-fixture"),
        ("Recovery passphrase: ", "wizard-recovery-fixture"),
        ("Confirm passphrase: ", "wizard-recovery-fixture"),
        ("[y/N]: ", "y"),
    ])
    config = tomllib.loads((project / "pinset.toml").read_text())
    assert config["schema"] == 4
    assert "auto-profile" not in config["environment"]
    assert len(config["environment"]["profiles"]["dev"]["recipients"]) == 2
    assert identity.is_file() and recovery.is_file()
    assert not (project / "pinset.lock").exists()
    status = subprocess.check_output([str(binary), "env"], cwd=project, env=environment, text=True)
    assert "profile=dev source=local" in status and "trust=trusted" in status
    environment["PINSET_IDENTITY_FILE"] = str(identity)
    interact(binary, project, environment, ["env", "set", "APP_WIZARD_VALUE"], [
        ("Value for APP_WIZARD_VALUE: ", "wizard-variable-fixture"),
        ("Identity file passphrase: ", "wizard-device-fixture"),
    ])
    probe = [sys.executable, "-c",
             'import os; assert os.environ["APP_WIZARD_VALUE"] == "wizard-variable-fixture"']
    interact(binary, project, environment, ["--", *probe],
             [("Identity file passphrase: ", "wizard-device-fixture")])

    if "--keyring" in sys.argv[2:]:
        environment.pop("PINSET_IDENTITY_FILE")
        subprocess.run([str(binary), "env", "identity", "create"], cwd=project,
                       env=environment, check=True, stdout=subprocess.DEVNULL)
        metadata_path = root / "home" / "state" / "identities.toml"
        metadata = tomllib.loads(metadata_path.read_text())
        record, = metadata["identities"]
        assert record["backend"] == "keyring"
        interact(binary, project, environment, ["env", "init"], [
            ("Profile [dev]: ", "keyring"),
            ("skip recovery): ", "none"),
            ("Identity ID to reuse [new]: ", record["id"]),
            ("[y/N]: ", "y"),
        ])
        assert tomllib.loads(metadata_path.read_text()) == metadata
        config = tomllib.loads((project / "pinset.toml").read_text())
        assert config["environment"]["profiles"]["keyring"]["recipients"] == [record["recipient"]]
        subprocess.run([str(binary), "env", "set", "APP_WIZARD_VALUE", "--stdin"],
                       input="wizard-variable-fixture\n", text=True, cwd=project,
                       env=environment, check=True, stdout=subprocess.DEVNULL)
        subprocess.run([str(binary), "--", *probe], cwd=project,
                       env=environment, check=True)
        system = root / "system"
        system.mkdir()
        if os.name == "nt":
            (system / "node.cmd").write_text("@echo off\necho %APP_WIZARD_VALUE%\n")
        else:
            fake_node = system / "node"
            fake_node.write_text('#!/bin/sh\nprintf "%s\\n" "$APP_WIZARD_VALUE"\n')
            fake_node.chmod(0o755)
        shim_environment = environment | {"PATH": str(system)}
        shim = binary.with_name("pinset-shim.exe" if os.name == "nt" else "pinset-shim")
        injected = subprocess.check_output(
            [str(shim), "--as", "node", "--cwd", str(project)],
            cwd=project, env=shim_environment, text=True,
        )
        assert injected.strip() == "wizard-variable-fixture"
        rejected = subprocess.run(
            [str(binary), "__env-resolve", "--cwd", str(project), "--shim-version", "0.0.0"],
            cwd=project, env=environment, capture_output=True,
        )
        assert rejected.returncode != 0 and not rejected.stdout
        print("System keyring identity reuse and environment injection passed")

print("Interactive environment initialization and short execution passed")
