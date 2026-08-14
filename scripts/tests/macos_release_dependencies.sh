#!/bin/sh
set -eu

if [ "$#" -eq 0 ]; then
  echo "usage: $0 <macOS release binary>..." >&2
  exit 2
fi

if [ "$(uname -s)" != "Darwin" ]; then
  echo "macOS release dependency verification requires Darwin" >&2
  exit 2
fi

if ! command -v otool >/dev/null 2>&1; then
  echo "otool is required to verify macOS release dependencies" >&2
  exit 2
fi

for binary in "$@"; do
  if [ ! -x "$binary" ]; then
    echo "macOS release binary is not executable: $binary" >&2
    exit 1
  fi

  dependencies="$(otool -L "$binary")"
  printf '%s\n' "$dependencies"
  unexpected="$(
    printf '%s\n' "$dependencies" |
      tail -n +2 |
      awk '$1 !~ "^/usr/lib/" && $1 !~ "^/System/Library/" { print }'
  )"
  if [ -n "$unexpected" ]; then
    echo "macOS release binary has non-system dynamic dependencies: $binary" >&2
    printf '%s\n' "$unexpected" >&2
    exit 1
  fi
done

echo "macOS release binaries only depend on system libraries"
