#!/usr/bin/env python3
"""Render only reviewed non-secret report fields in GitHub's job summary."""
import html
import json
import os
from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
if path.is_symlink() or not path.is_file() or path.stat().st_size > 1024 * 1024:
    raise SystemExit("Environment report must be a regular file of at most 1 MiB")
report = json.loads(path.read_text(encoding="utf-8"))
if report.get("schema") != 2:
    raise SystemExit("Unsupported environment report schema")
def escape(value):
    text = str(value)
    return html.escape(text) if re.fullmatch(r"[A-Za-z0-9._+-]{1,120}", text) else "unknown"
lines = ["### Pinset environment", "", f"Environment ready: **{bool(report['environment_ready'])}**. Execution observed: **{bool(report['execution_verified'])}**.", "", "| Runtime | Locked version | Target |", "| --- | --- | --- |"]
for runtime in report["runtimes"]:
    lines.append("| " + " | ".join(escape(runtime.get(key) or "unknown") for key in ("tool", "locked_version", "target")) + " |")
lines.extend(["", "This report does not compare secret values or verify project dependencies, application builds, or unobserved editor entries.", ""])
with Path(os.environ["GITHUB_STEP_SUMMARY"]).open("a", encoding="utf-8") as output:
    output.write("\n".join(lines))
