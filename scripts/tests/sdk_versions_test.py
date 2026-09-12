#!/usr/bin/env python3
"""Opt-in real Go/Rust/.NET SDK acceptance; run in the local verification container."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

binary = Path(sys.argv[1]).resolve(strict=True)
with tempfile.TemporaryDirectory(prefix="pinset-sdk-probes-") as temporary:
    project = Path(temporary)
    home = Path(os.environ.get("PINSET_SDK_PROBE_HOME", str(project / "home")))
    env = os.environ.copy()
    for name in ("PINSET_IDENTITY", "PINSET_IDENTITY_FILE", "PINSET_ENV_PROFILE", "PINSET_ENV_DISABLE",
                 "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "GOTOOLCHAIN", "GOWORK",
                 "DOTNET_STARTUP_HOOKS", "DOTNET_ADDITIONAL_DEPS"):
        env.pop(name, None)
    env.update(PINSET_HOME=str(home), PINSET_LANG="en")

    def run(*args, timeout=900):
        result = subprocess.run([str(binary), *args], cwd=project, env=env,
                                capture_output=True, text=True, timeout=timeout)
        assert result.returncode == 0, (args, result.stdout[-4000:], result.stderr[-4000:])
        return json.loads(result.stdout) if "--json" in args else result.stdout

    run("init")
    run("use", "go@1.24.0", "rust@1.86.0", "dotnet@8.0.303", "--no-install")
    run("install", "--locked")
    data = run("check", "--probe", "--json", timeout=60)["data"]["report"]
    assert data["environment_ready"] and data["execution_verified"], data
    observed = {entry["tool"]: entry for entry in data["evidence"]}
    for tool, version in (("go", "1.24.0"), ("rust", "1.86.0"), ("dotnet", "8.0.303")):
        entry = observed[tool]
        assert entry["state"] == "pass" and entry["observed_version"] == version, entry
        assert entry["entry"] == "managed-version" and entry["observed_executable"] is None
    print(json.dumps({"real_sdk_versions": {tool: entry["observed_version"] for tool, entry in observed.items()},
                      "self_observed_paths": False, "project_builds_verified": False}))
