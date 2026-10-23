#!/usr/bin/env python3
"""Focused public JSON contract. Standard-library Python 3.10+; no test runner.

Run after applying the prepared JSON patch and building pinset + pinset-shim:
  python agent_json_contract.py --cli /absolute/path/to/pinset

The default suite creates only temporary project/home state and does not resolve
or download tools. Add --pipeline to explicitly download one official Node 22
runtime and exercise import -> install -> check using JSON alone. Its controlled
negative case removes the executable path returned by `which`, then restores it.
It never reads config/lock/receipt files to make pipeline decisions.
"""

import argparse
import copy
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def document(text):
    value = json.loads(text)  # Reject terminal text and multiple JSON documents.
    require(isinstance(value, dict), "CLI JSON must be an object")
    require(value.get("protocol") == "pinset/3", "unexpected CLI protocol")
    require(type(value.get("schema_version")) is int and value["schema_version"] == 1,
            "expected additive schema_version 1")
    return value


def test_identity():
    """Publicly reproducible test key, passed only in child memory; no keyring."""
    hrp = "age-secret-key-"
    alphabet = "qpzry9x8gf2tvdw0s3jn54khce6mua7l"
    encoded = []
    accumulator = bits = 0
    for byte in bytes(range(32)):
        accumulator = (accumulator << 8) | byte
        bits += 8
        while bits >= 5:
            bits -= 5
            encoded.append((accumulator >> bits) & 31)
    if bits:
        encoded.append((accumulator << (5 - bits)) & 31)
    checksum = 1
    values = [ord(char) >> 5 for char in hrp] + [0] + [ord(char) & 31 for char in hrp]
    for value in values + encoded + [0] * 6:
        top = checksum >> 25
        checksum = ((checksum & 0x1ffffff) << 5) ^ value
        for index, generator in enumerate((0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3)):
            if (top >> index) & 1:
                checksum ^= generator
    checksum ^= 1
    return (hrp + "1" + "".join(alphabet[value] for value in encoded + [
        (checksum >> (5 * (5 - index))) & 31 for index in range(6)
    ])).upper()


def acceptable_plan(value, expected):
    for name in ("unsupported", "conflicts", "unresolved"):
        require(isinstance(value.get(name), list), f"missing plan array: {name}")
        require(not value[name], f"migration needs review: {name}")
    selected = {}
    for name in ("imports", "retained"):
        require(isinstance(value.get(name), list), f"missing plan array: {name}")
        for item in value[name]:
            require(isinstance(item.get("tool"), str), "plan entry has no tool")
            require(isinstance(item.get("version"), str), "plan entry has no exact version")
            selected[item["tool"]] = item["version"]
    require(bool(expected), "the agent must declare its expected tool set")
    require(expected <= selected.keys(), "expected tools absent from migration plan")
    return {name: selected[name] for name in expected}


class Harness:
    def __init__(self, cli, scratch):
        self.cli = cli
        self.scratch = scratch
        self.records = []
        self.environment = os.environ.copy()
        for key in ("PINSET_IDENTITY", "PINSET_PROFILE", "PINSET_NO_ENV", "PINSET_ENV_RESOLVED"):
            self.environment.pop(key, None)
        self.environment.update(PINSET_HOME=str(scratch / "home"), HOME=str(scratch / "user"),
                                USERPROFILE=str(scratch / "user"),
                                PINSET_IDENTITY=test_identity(),
                                XDG_CONFIG_HOME=str(scratch / "user" / "config"),
                                XDG_DATA_HOME=str(scratch / "user" / "data"))
        (scratch / "user").mkdir()

    def project(self, name):
        path = self.scratch / name
        path.mkdir()
        return path

    def invoke(self, root, *arguments, expected=0, import_stderr=False, global_json=True):
        command = [str(self.cli), "-C", str(root), "--lang", "en"]
        if global_json:
            command.append("--json")
        command.extend(arguments)
        started = time.monotonic()
        result = subprocess.run(command, cwd=root, env=self.environment, input="", text=True,
                                encoding="utf-8", errors="strict", capture_output=True, timeout=900)
        record = {"arguments": list(arguments), "exit": result.returncode,
                  "expected_exit": expected, "seconds": round(time.monotonic() - started, 3)}
        self.records.append(record)
        require(result.returncode == expected,
                f"{arguments}: exit {result.returncode}, expected {expected}; stdout={result.stdout!r}")
        value = document(result.stdout)
        if import_stderr and result.returncode == 0:
            preview = document(result.stderr)
            require(preview.get("operation") == "import" and preview.get("plan") is True,
                    "import --yes stderr must contain its separate JSON plan")
        else:
            require(not result.stderr, f"{arguments}: unexpected stderr")
        if expected:
            if "error" in value:
                require(isinstance(value["error"].get("code"), str), "missing machine error code")
                require(isinstance(value["error"].get("message"), str), "missing error message")
            else:
                require(value.get("passed") is False, "failed check lacks passed:false")
        return value


def no_download_contract(harness):
    root = harness.project("commands")
    harness.invoke(root, "init")
    harness.invoke(root, "init", expected=1)
    listing = harness.invoke(root, "list")
    require(isinstance(listing["installs"], list), "list installs must be an array")
    require(isinstance(listing["selected"], dict), "initialized list must include selected lock")
    require(harness.invoke(root, "which")["commands"] == [], "empty which collection changed")
    require(harness.invoke(root, "env", "list")["profiles"] == [], "fresh env list must be empty")
    missing = harness.invoke(root, "env", "list", "--profile", "absent", expected=1)
    require(missing["error"]["code"] == "PINSET_PROFILE_MISSING", "wrong missing profile code")
    harness.invoke(root, "env", "init", "dev")
    profiles = harness.invoke(root, "env", "list")
    require(profiles["profiles"] == ["dev"], "env list omitted a configured profile")
    require(profiles["shared_default"] is None and profiles["local_default"] is None,
            "profile creation must not invent defaults")
    variables = harness.invoke(root, "env", "list", "--profile", "dev")
    require(variables["profile"] == "dev" and variables["variables"] == [],
            "profile query must list names without returning decrypted values")
    require(harness.invoke(root, "install", "--plan")["tools"] == [], "empty install plan changed")
    empty = harness.invoke(root, "check")
    require(empty["passed"] is True and empty["report"]["checks"] == [],
            "empty check is a valid report, not evidence that expected tools exist")
    require(harness.invoke(root, "check", "--format", "json", global_json=False) == empty,
            "check --format json must match global --json")
    invalid = harness.invoke(root, "remove", "node", "--nonexistent", expected=2)
    require(invalid["error"]["code"] == "PINSET_ARGUMENT_INVALID", "wrong argument error")
    invalid_check = harness.invoke(root, "check", "--format", "json", "--nonexistent",
                                   expected=2, global_json=False)
    require(invalid_check["passed"] is False, "check argument error lacks passed:false")
    require(harness.invoke(root, "which", "node", expected=1)["error"]["code"] ==
            "PINSET_TOOL_NOT_SELECTED", "unselected which must fail")
    require(harness.invoke(root, "install", "node", expected=1)["error"]["code"] ==
            "PINSET_TOOL_NOT_SELECTED", "unselected install must fail")
    require(harness.invoke(root, "exec", "--", "echo", "unexecuted", expected=1)
            ["error"]["code"] == "PINSET_NATIVE_OUTPUT", "exec JSON exception changed")
    require(harness.invoke(root, "env", "access", "request", "--ci", expected=1)
            ["error"]["code"] == "PINSET_CI_IDENTITY_TTY", "CI identity JSON exception changed")
    for args in (("self", "info"), ("self", "shell", "bash"),
                 ("self", "completions", "bash"), ("clean", "cache", "--plan")):
        harness.invoke(root, *args)
    require(isinstance(harness.invoke(root, "self", "shell", "bash")["text"], str),
            "shell JSON must retain its text wrapper")

    no_sources = harness.project("no-sources")
    require(harness.invoke(no_sources, "import", "--plan", expected=1)["error"]["code"] ==
            "PINSET_IMPORT_NO_SOURCES", "source-free import must fail")
    skipped = harness.project("skipped")
    (skipped / "package.json").write_text('{"engines":{"ruby":"3.3"}}', encoding="utf-8")
    plan = harness.invoke(skipped, "import", "--plan")
    require(plan["unsupported"] and plan["requires_write"] is False,
            "unsupported sources must remain explicit even on exit 0")
    # The agent refuses to proceed even though this CLI request returned 0.
    try:
        acceptable_plan(plan, {"node"})
    except AssertionError:
        pass
    else:
        raise AssertionError("agent accepted unsupported tools or an empty selection")
    result = harness.invoke(skipped, "import", "--yes", import_stderr=True)
    require(result["applied"] is False, "unsupported-only import must not claim application")
    expanded = copy.deepcopy(plan)
    expanded["future_field"] = {"unknown": True}
    require(document(json.dumps(expanded))["unsupported"] == plan["unsupported"],
            "consumer rejected additive fields")


def pipeline_contract(harness, node_selector):
    root = harness.project("pipeline")
    (root / ".nvmrc").write_text(node_selector + "\n", encoding="utf-8")
    expected = {"node"}
    plan = harness.invoke(root, "import", "--plan")
    versions = acceptable_plan(plan, expected)
    extended = copy.deepcopy(plan)
    extended["future_field"] = True
    require(acceptable_plan(extended, expected) == versions, "agent rejected additive plan fields")
    applied = harness.invoke(root, "import", "--yes", import_stderr=True)
    require(acceptable_plan(applied, expected) == versions, "resolved version changed; review new plan")
    require(applied["plan"] is False and applied["applied"] is True, "import was not applied")
    missing = harness.invoke(root, "check", expected=1)
    require(missing["passed"] is False, "uninstalled selected Node must fail check")
    offline = harness.invoke(root, "install", "--offline", expected=1)
    require(isinstance(offline.get("error"), dict), "uncached offline install must report an error")
    installed = harness.invoke(root, "install")
    require(installed["installed"] is True, "install did not succeed")
    require(expected <= set(installed["tools"]), "install omitted requested tool names")
    details = {item["tool"]: item for item in installed["installed_tools"]}
    for name, version in versions.items():
        require(details[name]["version"] == version, "installed version differs from plan")
        for key in ("provider", "target", "install_identity"):
            require(isinstance(details[name].get(key), str), "missing installation detail: " + key)
    first = harness.invoke(root, "list")
    second = harness.invoke(root, "list")
    require(first == second, "unchanged list JSON must be deterministic")
    entries = first["installs"]
    sort_key = lambda item: tuple(item[key] for key in ("tool", "version", "identity", "platform", "path"))
    require(entries == sorted(entries, key=sort_key), "install inventory order is unstable")
    route = harness.invoke(root, "which", "node")
    require(route["tool"] == "node" and route["version"] == versions["node"], "wrong Node route")
    require("node" in {item["tool"] for item in harness.invoke(root, "which")["commands"]},
            "collection which omitted Node")
    checked = harness.invoke(root, "check", "--deep", "--probe")
    checks = {item["tool"]: item for item in checked["report"]["checks"]}
    require(expected <= checks.keys() and checked["passed"] is True, "required tools did not pass")
    require(checks["node"]["actual_version"] == versions["node"], "probe observed wrong version")
    # This path comes from JSON; no config, lock or receipt file is inspected.
    executable = Path(route["executable"])
    owned = (harness.scratch / "home").resolve()
    require(owned in executable.resolve().parents, "negative fixture path escaped temporary home")
    saved = executable.with_name(executable.name + ".agent-contract-backup")
    require(not saved.exists(), "negative fixture backup already exists")
    executable.rename(saved)
    try:
        require(harness.invoke(root, "check", "--deep", expected=1)["passed"] is False,
                "damaged installation must fail")
        harness.invoke(root, "which", "node", expected=1)
    finally:
        saved.rename(executable)
    require(harness.invoke(root, "check", "--deep", "--probe")["passed"] is True,
            "negative fixture restoration failed")


def verify_agent_json(cli, pipeline=False, node_selector="22"):
    found = shutil.which(str(cli))
    binary = Path(found or cli).resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="pinset-agent-json-") as temporary:
        harness = Harness(binary, Path(temporary))
        no_download_contract(harness)
        if pipeline:
            pipeline_contract(harness, node_selector)
        return {"passed": True, "schema_version": 1,
                "scope": "public-json-contract" + ("+official-node-pipeline" if pipeline else ""),
                "commands": harness.records}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", required=True, help="built pinset executable with adjacent matching pinset-shim")
    parser.add_argument("--pipeline", action="store_true", help="explicitly resolve/download one Node runtime")
    parser.add_argument("--node-selector", default="22", help="Node declaration used only with --pipeline")
    args = parser.parse_args()
    print(json.dumps(verify_agent_json(args.cli, args.pipeline, args.node_selector), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
