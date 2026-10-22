#!/usr/bin/env python3
"""Explicit GitHub Actions native smoke; downloads only Node 22 and Terraform 1.9.0.

Python >=3.11 is required. Run on a native runner, with a job-scoped PINSET_HOME:
  python3 scripts/tests/ci_platform.py --cli pinset --output output/ci-platform \
    --expected-target linux-x86_64

The project and public JSON/TOML reports remain in --output. Temporary HOME/XDG
directories are private to child processes; the job PINSET_HOME remains available
for the following setup-pinset Action. No identity/profile commands are invoked.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time
import tomllib


TARGETS = ("linux-x86_64", "linux-aarch64", "macos-aarch64", "windows-x86_64")


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def safe_text(value):
    value = re.sub(r"(?i)(https?://)[^/\s@]+@", r"\1[redacted]@", value)
    return re.sub(r"(?im)\b(authorization|password|token|secret)\s*[:=]\s*[^\s,;]+", r"\1=[redacted]", value)[-16000:]


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def toml(path):
    with path.open("rb") as stream:
        return tomllib.load(stream)


def native_target():
    operating_system = {"Linux": "linux", "Darwin": "macos", "Windows": "windows"}.get(platform.system())
    arch = {"x86_64": "x86_64", "amd64": "x86_64", "aarch64": "aarch64", "arm64": "aarch64"}.get(platform.machine().lower())
    require(operating_system is not None and arch is not None, "unsupported native Python OS/architecture")
    require("microsoft" not in platform.release().lower() and not os.environ.get("WSL_DISTRO_NAME"), "WSL is outside this native-runner verification")
    return f"{operating_system}-{arch}"


class Smoke:
    def __init__(self, cli, project, output, environment, report):
        self.cli = cli
        self.project = project
        self.output = output
        self.environment = environment
        self.report = report

    def command(self, label, arguments, expected=0):
        started = time.monotonic()
        entry = {"name": label, "command": [str(value) for value in arguments], "expected_exit": expected}
        self.report["commands"].append(entry)
        try:
            result = subprocess.run(arguments, cwd=self.project, env=self.environment, capture_output=True,
                                    text=True, encoding="utf-8", errors="replace", timeout=900)
        except (OSError, subprocess.TimeoutExpired) as error:
            entry.update({"passed": False, "error": safe_text(str(error)), "seconds": round(time.monotonic() - started, 3)})
            raise RuntimeError(f"{label}: could not complete the native process") from error
        entry.update({"exit": result.returncode, "seconds": round(time.monotonic() - started, 3), "passed": result.returncode == expected})
        entry["stdout_tail"] = safe_text(result.stdout)[-2048:]
        if result.stderr:
            entry["stderr"] = safe_text(result.stderr)
        if result.returncode != expected:
            diagnostic = {"command": entry["command"], "exit": result.returncode,
                          "stdout": safe_text(result.stdout), "stderr": safe_text(result.stderr)}
            write_json(self.output / f"{label}-diagnostic.json", diagnostic)
            raise RuntimeError(f"{label}: exit {result.returncode}, expected {expected}; see {label}-diagnostic.json")
        return result

    def cli_command(self, label, *arguments, json_output=True):
        command = [str(self.cli), "-C", str(self.project), "--lang", "en"]
        if json_output:
            command.append("--json")
        result = self.command(label, command + list(arguments))
        if not json_output:
            return result
        try:
            value = json.loads(result.stdout)
        except ValueError as error:
            write_json(self.output / f"{label}-diagnostic.json", {"stdout": safe_text(result.stdout), "stderr": safe_text(result.stderr)})
            raise RuntimeError(f"{label}: stdout is not one JSON document") from error
        require(value.get("protocol") == "pinset/3", f"{label}: unexpected CLI protocol")
        write_json(self.output / f"{label}.json", value)
        return value

    def check(self, filename, versions):
        result = self.cli_command(filename, "check", "--deep", "--probe", "--format", "json", json_output=False)
        value = json.loads(result.stdout)
        write_json(self.output / f"{filename}.json", value)
        require(not result.stderr.strip(), f"{filename}: structured check emitted unexpected stderr")
        require(value.get("protocol") == "pinset/3" and value.get("schema_version") == 1 and value.get("passed") is True,
                f"{filename}: overall structured check failed")
        checks = {item["tool"]: item for item in value["report"]["checks"]}
        require(set(checks) == set(versions), f"{filename}: unexpected checked tools")
        for name, version in versions.items():
            item = checks[name]
            require(item["expected_version"] == version and item["actual_version"] == version,
                    f"{filename}: {name} expected/actual version differs from the lock")
            require(all(item[field] is True for field in ("configured", "installed", "bound", "actually_verified", "passed")),
                    f"{filename}: {name} did not pass execution and routing checks")
            checksum = item["artifact_digest"]
            require(all(checksum[field] is True for field in ("receipt_matches_lock", "payload_verified", "passed")),
                    f"{filename}: {name} artifact/payload check did not pass")
            require(checksum["expected"] == "sha256:" + self.report["artifacts"][name]["sha256"],
                    f"{filename}: {name} check digest differs from lock")
        return checks

    def locked_artifact(self, name, version, target):
        lock = toml(self.project / ".pinset/lock.toml")
        selected = next(tool for tool in lock["tool"] if tool["name"] == name)
        require(selected["version"] == version, f"{name}: unexpected locked version")
        artifacts = [artifact for artifact in selected["artifact"] if artifact["target"] == target]
        require(len(artifacts) == 1, f"{name}: missing/duplicate native artifact")
        artifact = artifacts[0]
        require(re.fullmatch(r"[0-9a-f]{64}", artifact["sha256"]) is not None, f"{name}: lock has no SHA-256")
        cache = Path(self.environment["PINSET_HOME"]) / "v3/cache/downloads/sha256" / (artifact["sha256"] + ".archive")
        require(cache.is_file() and digest(cache) == artifact["sha256"], f"{name}: real download cache differs from locked SHA-256")
        self.report["artifacts"][name] = {"version": version, "provider": selected["provider"], "target": target,
                                            "canonical_url": artifact["canonical_url"], "sha256": artifact["sha256"],
                                            "download_digest_verified": True}
        return selected, artifact


def execute(args, output, project, report):
    require(os.environ.get("GITHUB_ACTIONS") == "true", "this network smoke is only run by an explicitly dispatched GitHub Actions job")
    actual_target = native_target()
    report["native_target"] = actual_target
    require(actual_target == args.expected_target, f"native Python target {actual_target} differs from {args.expected_target}")
    runner_os = {"Linux": "linux", "macOS": "macos", "Windows": "windows"}.get(report["runner_os"])
    runner_arch = {"X64": "x86_64", "ARM64": "aarch64"}.get(report["runner_arch"])
    require(f"{runner_os}-{runner_arch}" == actual_target, "GitHub runner OS/architecture does not match the native process")
    resolved = shutil.which(args.cli)
    require(resolved is not None, "provided Pinset CLI is not executable or is not on PATH")
    cli = Path(resolved).resolve()
    suffix = ".exe" if os.name == "nt" else ""
    shim = cli.with_name("pinset-shim" + suffix)
    require(shim.is_file(), "matching pinset-shim must be adjacent to the real CLI")
    report["pair"] = {"cli": str(cli), "shim": str(shim), "cli_sha256": digest(cli), "shim_sha256": digest(shim)}
    pinset_home = Path(os.environ.get("PINSET_HOME", str(output / "pinset-home"))).resolve()
    require(bool(os.environ.get("PINSET_HOME")), "the job must provide an isolated PINSET_HOME for the following Action step")
    with tempfile.TemporaryDirectory(prefix="pinset-ci-home-") as private:
        private_home = Path(private).resolve()
        allowed = ("PATH", "PATHEXT", "SystemRoot", "SYSTEMROOT", "WINDIR", "COMSPEC", "LANG", "LC_ALL",
                   "SSL_CERT_FILE", "SSL_CERT_DIR", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY",
                   "http_proxy", "https_proxy", "all_proxy", "no_proxy")
        environment = {key: os.environ[key] for key in allowed if key in os.environ}
        for key in ("HOME", "USERPROFILE"):
            environment[key] = str(private_home)
        for key, directory in (("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"), ("XDG_CACHE_HOME", "cache"),
                               ("XDG_STATE_HOME", "state"), ("APPDATA", "appdata"), ("LOCALAPPDATA", "localappdata"),
                               ("TMPDIR", "tmp"), ("TMP", "tmp"), ("TEMP", "tmp")):
            path = private_home / directory
            path.mkdir(exist_ok=True)
            environment[key] = str(path)
        terraform_config = private_home / "terraform.rc"
        terraform_config.write_text("", encoding="utf-8")
        environment.update({"PINSET_HOME": str(pinset_home), "PINSET_NO_ENV": "1", "CHECKPOINT_DISABLE": "1",
                            "TF_IN_AUTOMATION": "1", "TF_CLI_CONFIG_FILE": str(terraform_config), "NO_COLOR": "1"})
        smoke = Smoke(cli, project, output, environment, report)
        info = smoke.cli_command("self-info", "self", "info")
        require(info["platform"] == actual_target, "Pinset compiled target differs from the native runner")
        report["cli_version"] = info["version"]
        result = smoke.command("shim-entry", [str(shim)], expected=1)
        require("PINSET_COMMAND_INVALID" in result.stderr, "adjacent shim did not run its native entry guard")
        smoke.cli_command("init", "init")
        config_path = project / ".pinset/config.toml"
        lock_path = project / ".pinset/lock.toml"
        (project / ".nvmrc").write_text("22\n", encoding="utf-8")
        before = config_path.read_bytes()
        plan = smoke.cli_command("import-plan", "import", "--plan")
        require(plan["plan"] is True and plan["install"] is False and plan["requires_write"] is True, "import preview contract differs")
        require(not plan["conflicts"] and not plan["unresolved"] and len(plan["imports"]) == 1, "Node import preview did not resolve cleanly")
        planned = plan["imports"][0]
        require(planned["tool"] == "node" and planned["resolved"] is True and re.fullmatch(r"22\.\d+\.\d+", planned["version"]),
                "Node 22 preview did not resolve to an annotated exact version")
        require(config_path.read_bytes() == before and not lock_path.exists(), "import preview wrote project files")
        applied = smoke.cli_command("import-applied", "import", "--yes")
        require(applied["plan"] is False and applied["applied"] is True and applied["install"] is False, "import did not apply without installing")
        node_version = toml(config_path)["tools"]["node"]
        require(re.fullmatch(r"22\.\d+\.\d+", node_version) is not None, "imported Node selection is not exact Node 22")
        report["import"] = {"planned_version": planned["version"], "selected_version": node_version, "range_resolved": True}
        smoke.cli_command("install-node", "install", "node")
        _, node_artifact = smoke.locked_artifact("node", node_version, actual_target)
        require(node_artifact["canonical_url"].startswith("https://nodejs.org/"), "Node download did not use the official HTTPS origin")
        smoke.check("check-node", {"node": node_version})
        token = "PINSET_CI_NATIVE_NODE_" + actual_target
        script = "process.stdout.write(JSON.stringify({token:" + json.dumps(token) + ",version:process.versions.node,platform:process.platform,arch:process.arch}))"
        result = smoke.cli_command("node-exec", "exec", "--no-env", "--", "node", "-e", script, json_output=False)
        observed = json.loads(result.stdout)
        require(observed == {"token": token, "version": node_version,
                             "platform": {"linux": "linux", "macos": "darwin", "windows": "win32"}[actual_target.split("-")[0]],
                             "arch": "arm64" if actual_target.endswith("aarch64") else "x64"}, "Node native execution target/version/token mismatch")
        managed = pinset_home / "v3/bin"
        result = smoke.command("node-shim", [str(managed / ("node" + suffix)), "-e", script])
        require(json.loads(result.stdout) == observed, "managed Node shim or adjacent broker pair differs from exec")
        with config_path.open("a", encoding="utf-8", newline="\n") as stream:
            stream.write('\n[generic.terraform]\nurl = "https://releases.hashicorp.com/terraform/{version}/terraform_{version}_{os}_{arch}.zip"\n'
                         'format = "zip"\nexecutable = "terraform{exe}"\nos = { macos = "darwin" }\n'
                         'arch = { x86_64 = "amd64", aarch64 = "arm64" }\n')
        smoke.cli_command("use-terraform", "use", "terraform@1.9.0")
        selected, artifact = smoke.locked_artifact("terraform", "1.9.0", actual_target)
        expected_os = actual_target.split("-")[0].replace("macos", "darwin")
        expected_arch = "arm64" if actual_target.endswith("aarch64") else "amd64"
        require(selected["provider"] == "generic-binary" and artifact["canonical_url"] ==
                f"https://releases.hashicorp.com/terraform/1.9.0/terraform_1.9.0_{expected_os}_{expected_arch}.zip", "generic platform URL mapping differs")
        result = smoke.cli_command("terraform-exec", "exec", "--no-env", "--", "terraform", "--version", json_output=False)
        require(result.stdout.splitlines() and result.stdout.splitlines()[0].strip() == "Terraform v1.9.0", "Terraform exec version differs")
        result = smoke.command("terraform-shim", [str(managed / ("terraform" + suffix)), "--version"])
        require(result.stdout.splitlines() and result.stdout.splitlines()[0].strip() == "Terraform v1.9.0", "generic shim did not route Terraform 1.9.0")
        report["checks"] = smoke.check("check", {"node": node_version, "terraform": "1.9.0"})
        locked = lock_path.read_bytes()
        smoke.cli_command("install-offline", "install", "--offline")
        require(lock_path.read_bytes() == locked, "offline install changed the lock")
        report["offline_checks"] = smoke.check("check-after-offline", {"node": node_version, "terraform": "1.9.0"})
        config_before = config_path.read_bytes()
        (project / ".node-version").write_text("20.19.0\n", encoding="utf-8")
        conflict = smoke.cli_command("import-conflict-plan", "import", "--plan")
        conflicts = [item for item in conflict["conflicts"] if item["tool"] == "node"]
        require(len(conflicts) == 1 and conflicts[0]["reason"] == "existing_selection" and conflicts[0]["version"] == node_version,
                "import did not report the existing Node 22 conflict")
        require(any(item["tool"] == "node" and item["version"] == node_version for item in conflict["retained"]), "conflict plan lost the existing Node selection")
        require(config_path.read_bytes() == config_before and lock_path.read_bytes() == locked, "conflict preview overwrote existing selections")
        report["conflict_preserved_existing_selection"] = True
        report["actual_versions"] = {name: item["actual_version"] for name, item in report["checks"].items()}
        report["paired_managed_entries_verified"] = True


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--cli", required=True, help="Native CLI path or name on PATH; adjacent matching shim is required")
    parser.add_argument("--output", type=Path, default=Path("output/ci-platform"))
    parser.add_argument("--expected-target", required=True, choices=TARGETS)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    project = output / "project"
    report_path = output / "report.json"
    report = {"schema_version": 1, "status": "running", "expected_target": args.expected_target,
              "source_commit": os.environ.get("GITHUB_SHA"), "runner_os": os.environ.get("RUNNER_OS"),
              "runner_arch": os.environ.get("RUNNER_ARCH"), "python_version": platform.python_version(),
              "python_machine": platform.machine(), "project": str(project), "commands": [], "artifacts": {},
              "scope": "native GitHub Actions smoke; Node 22 and Terraform 1.9.0 only; no source builds, WSL, or architecture emulation"}
    started = time.monotonic()
    try:
        project.mkdir(exist_ok=False)
        github_output = os.environ.get("GITHUB_OUTPUT")
        if github_output:
            with open(github_output, "a", encoding="utf-8") as stream:
                stream.write(f"project={project}\nreport={report_path}\n")
        execute(args, output, project, report)
        report["status"] = "passed"
        report["passed"] = True
    except Exception as error:
        report.update({"status": "failed", "passed": False, "error": safe_text(f"{type(error).__name__}: {error}")})
        print(report["error"], file=sys.stderr)
    finally:
        for filename in ("config.toml", "lock.toml"):
            source = project / ".pinset" / filename
            if source.is_file():
                shutil.copyfile(source, output / filename)
        report["seconds"] = round(time.monotonic() - started, 3)
        write_json(report_path, report)
    print(json.dumps({"passed": report["passed"], "expected_target": args.expected_target,
                      "native_target": report.get("native_target"), "report": str(report_path)}, ensure_ascii=False))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
