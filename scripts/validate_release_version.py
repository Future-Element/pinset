#!/usr/bin/env python3
"""Validate immutable stable/RC tags against the workspace's stable base version."""
import re
import sys


def validate(workspace: str, tag: str) -> bool:
    number = r"(?:0|[1-9][0-9]*)"
    if not re.fullmatch(rf"{number}\.{number}\.{number}", workspace):
        return False
    return re.fullmatch(rf"v?{re.escape(workspace)}(?:-rc\.[1-9][0-9]*)?", tag) is not None


if __name__ == "__main__":
    if len(sys.argv) != 3 or not validate(sys.argv[1], sys.argv[2]):
        raise SystemExit("release tag must match the workspace version or its -rc.N candidate (N >= 1)")
