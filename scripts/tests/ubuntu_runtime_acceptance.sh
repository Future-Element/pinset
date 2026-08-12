#!/usr/bin/env bash
set -euo pipefail

: "${PINSET_BIN:?PINSET_BIN must point to the release pinset binary}"

GLOBAL_VERSION="${PINSET_GLOBAL_VERSION:-24.0.0}"
PROJECT_VERSION="${PINSET_PROJECT_VERSION:-22.0.0}"
VERSION_PATTERN='^[0-9]+\.[0-9]+\.[0-9]+$'

if [[ ! "$GLOBAL_VERSION" =~ $VERSION_PATTERN ]] || [[ ! "$PROJECT_VERSION" =~ $VERSION_PATTERN ]]; then
  echo "acceptance versions must use x.y.z" >&2
  exit 2
fi
if [[ ! -x "$PINSET_BIN" ]]; then
  echo "PINSET_BIN is not executable: $PINSET_BIN" >&2
  exit 2
fi

TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEST_ROOT"' EXIT

export PINSET_HOME="$TEST_ROOT/pinset-home"
unset PINSET_LANG

cd "$TEST_ROOT"

"$PINSET_BIN" --lang zh-CN | tee language.txt
grep -F '语言已切换为中文' language.txt
grep -F 'language = "zh-CN"' "$PINSET_HOME/settings.toml"

"$PINSET_BIN" use "node@$GLOBAL_VERSION" --global
"$PINSET_BIN" current node | tee global-current.txt
grep -F "Node.js $GLOBAL_VERSION" global-current.txt
grep -F '来源=全局' global-current.txt
"$PINSET_BIN" exec -- node --version | grep -Fx "v$GLOBAL_VERSION"
"$PINSET_BIN" exec -- npm --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
"$PINSET_BIN" exec -- corepack --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'

mkdir project
cd project
"$PINSET_BIN" init
"$PINSET_BIN" use "node@$PROJECT_VERSION"
test -f pinset.toml
test -f pinset.lock
"$PINSET_BIN" current node | tee project-current.txt
grep -F "Node.js $PROJECT_VERSION" project-current.txt
grep -F '来源=项目' project-current.txt
"$PINSET_BIN" exec -- node --version | grep -Fx "v$PROJECT_VERSION"
"$PINSET_BIN" exec -- npm --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
"$PINSET_BIN" exec -- corepack --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'

cd "$TEST_ROOT"
"$PINSET_BIN" current node | tee restored-global-current.txt
grep -F "Node.js $GLOBAL_VERSION" restored-global-current.txt
grep -F '来源=全局' restored-global-current.txt
"$PINSET_BIN" exec -- node --version | grep -Fx "v$GLOBAL_VERSION"

echo "Ubuntu real runtime acceptance passed"
