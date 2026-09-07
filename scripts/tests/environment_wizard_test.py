#!/usr/bin/env python3
"""Exercise the real environment wizard in an isolated POSIX pseudo-terminal."""
import errno
import os
from pathlib import Path
import pty
import select
import signal
import subprocess
import sys
import tempfile
import time
import tomllib


def interact(binary, project, environment, arguments, replies):
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
    interact(binary, project, environment, [
        "--", "sh", "-c", 'test "$APP_WIZARD_VALUE" = "wizard-variable-fixture"',
    ], [("Identity file passphrase: ", "wizard-device-fixture")])

print("Interactive environment initialization and short execution passed")
