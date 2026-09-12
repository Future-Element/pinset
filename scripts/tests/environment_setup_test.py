#!/usr/bin/env python3
"""Native environment acceptance. Run only in a disposable GitHub Actions VM."""
from __future__ import annotations
import json
import os
import platform
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time


def main() -> None:
    cli = Path(sys.argv[1]).resolve()
    # macOS AF_UNIX socket names are limited to 103 bytes. Keep VS Code's user-data
    # root short while still exercising project/runtime paths containing spaces.
    with tempfile.TemporaryDirectory(prefix="pinset-env-", dir="/tmp" if sys.platform == "darwin" else None) as temporary:
        root = Path(temporary)
        project = root / "project with spaces"
        project.mkdir()
        env = dict(os.environ, PINSET_HOME=str(root / "home"), PINSET_TEST_CLI=str(cli))
        for key in ("PINSET_ENV_PROFILE", "PINSET_ENV_DISABLE", "VIRTUAL_ENV", "PYTHONHOME"):
            env.pop(key, None)

        def run(*args: str, json_output: bool = False) -> object:
            result = subprocess.run([str(cli), *args], cwd=project, env=env, text=True,
                                    capture_output=True, timeout=1200)
            if result.returncode:
                raise AssertionError(f"{args}: {result.returncode}\n{result.stdout}\n{result.stderr}")
            return json.loads(result.stdout)["data"] if json_output else result.stdout

        run("init")
        run("use", "node@24.1.0", "python@3.13", "--no-install")
        plan = run("setup", "--plan", "--json", json_output=True)
        assert not plan["blockers"]
        prepared = run("setup", "--yes", "--json", json_output=True)
        assert prepared["report"]["environment_ready"], prepared
        assert not prepared["report"]["execution_verified"]
        run("setup", "--resume", prepared["run"]["id"], "--yes", "--offline", "--json", json_output=True)
        report = run("check", "--probe", "--json", json_output=True)["report"]
        assert report["environment_ready"] and report["execution_verified"], report
        assert {item["tool"] for item in report["evidence"]} == {"node", "python"}
        assert all(item["observed_executable"] is None for item in report["evidence"])
        expected = run("which", "node").strip()
        expression = "console.log(process.execPath);console.log(require('child_process').execFileSync('python',['-c','import sys; print(sys.executable)'],{encoding:'utf8'}))"
        actual = run("--", "node", "-e", expression).strip().splitlines()
        assert Path(actual[0]).resolve() == Path(expected).resolve(), actual
        assert Path(actual[1]).resolve() == Path(run("which", "python").strip()).resolve(), actual
        assert run("--", "npm", "--version").strip()
        # An unrelated startup hook must never execute during a controlled probe.
        hook = project / "dangerous-hook.cjs"
        marker = project / "startup-executed"
        hook.write_text("require('fs').writeFileSync('startup-executed','bad')", encoding="utf-8")
        env["NODE_OPTIONS"] = f'--require "{hook}"'
        run("check", "--probe", "--json", json_output=True)
        assert not marker.exists()
        env.pop("NODE_OPTIONS")
        if os.name == "nt":
            for shell in ("powershell.exe", "pwsh.exe"):
                assert shutil.which(shell), f"Required shell unavailable: {shell}"
                result = subprocess.run([shell, "-NoProfile", "-NonInteractive", "-Command", "& $env:PINSET_TEST_CLI -- node -p process.execPath; exit $LASTEXITCODE"],
                                        cwd=project, env=env, capture_output=True, text=True, timeout=30, check=True)
                assert Path(result.stdout.strip()).resolve() == Path(expected).resolve()
            batch = project / "probe.cmd"
            batch.write_text('@"%PINSET_TEST_CLI%" -- node -p process.execPath\r\n', encoding="utf-8")
            result = subprocess.run(["cmd.exe", "/d", "/c", str(batch)], cwd=project, env=env, capture_output=True, text=True, timeout=30, check=True)
            assert Path(result.stdout.strip()).resolve() == Path(expected).resolve()
        shell = shutil.which("bash")
        if shell:
            env["PINSET_TEST_CLI"] = cli.as_posix()
            result = subprocess.run([shell, "--noprofile", "--norc", "-c", '"$PINSET_TEST_CLI" -- node -p process.execPath'], cwd=project, env=env, capture_output=True, text=True, timeout=30, check=True)
            assert Path(result.stdout.strip()).resolve() == Path(expected).resolve()
        legacy_samples = []
        for _ in range(3):
            start = time.monotonic()
            run("editor", "context", "--json", json_output=True)
            legacy_samples.append(round((time.monotonic() - start) * 1000, 2))
        samples = []
        for _ in range(5):
            start = time.monotonic()
            run("editor", "context", "--protocol", "2", "--json", json_output=True)
            samples.append(round((time.monotonic() - start) * 1000, 2))
        print(json.dumps({"platform": sys.platform, "prepared": True, "native_shells": True,
                          "managed_node_python_probes": True, "legacy_editor_refresh_ms": legacy_samples, "editor_refresh_ms": samples,
                          "native_ide_debug_test": "not covered by CLI acceptance"}))
        if os.environ.get("PINSET_EDITOR_TEST_MODULES"):
            subprocess.run(["node", str(Path(__file__).with_name("editor_environment_test.cjs")), str(cli), str(project), env["PINSET_HOME"]],
                           env=env, check=True, timeout=600)
        # Flutter has no built-in Linux ARM64 archive. Keep that boundary explicit.
        if not (sys.platform == "linux" and platform.machine().lower() in ("aarch64", "arm64")):
            run("use", "flutter@3.35.3", "java@21", "--no-install")
            flutter_plan = run("setup", "--plan", "--json", json_output=True)
            assert not flutter_plan["blockers"]
            flutter_setup = run("setup", "--yes", "--json", json_output=True)
            assert flutter_setup["report"]["environment_ready"], flutter_setup
            sdk = Path(run("which", "flutter").strip()).parent.parent
            dart_probe = project / "sdk_probe.dart"
            dart_probe.write_text("import 'dart:io'; void main() { print(Platform.resolvedExecutable); }", encoding="utf-8")
            dart_output = run("--", "dart", str(dart_probe)).strip()
            assert Path(dart_output).resolve().is_relative_to(sdk.resolve()), dart_output
            java_output = subprocess.run([str(cli), "--", "java", "-XshowSettings:properties", "-version"],
                                         cwd=project, env=env, capture_output=True, text=True, check=True, timeout=30)
            assert "java.home =" in java_output.stderr
            if os.environ.get("PINSET_EDITOR_TEST_MODULES"):
                flutter_env = dict(env, PINSET_EDITOR_TEST_TOOLS="flutter", PINSET_EDITOR_TEST_FLUTTER_SDK=str(sdk))
                subprocess.run(["node", str(Path(__file__).with_name("editor_environment_test.cjs")), str(cli), str(project), env["PINSET_HOME"]],
                               env=flutter_env, check=True, timeout=600)
            print(json.dumps({"flutter_sdk": "native dart process observed", "java": "native version process observed", "application_build": "not verified"}))


if __name__ == "__main__":
    main()
