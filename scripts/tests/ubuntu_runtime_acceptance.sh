#!/usr/bin/env bash
set -euo pipefail

: "${PINSET_BIN:?PINSET_BIN must point to the release pinset binary}"

GLOBAL_VERSION="${PINSET_GLOBAL_VERSION:-24.0.0}"
PROJECT_VERSION="${PINSET_PROJECT_VERSION:-22.0.0}"
PNPM_VERSION="${PINSET_PNPM_VERSION:-11.21.0}"
BUN_VERSION="${PINSET_BUN_VERSION:-1.3.14}"
VERSION_PATTERN='^[0-9]+\.[0-9]+\.[0-9]+$'

if [[ ! "$GLOBAL_VERSION" =~ $VERSION_PATTERN ]] || [[ ! "$PROJECT_VERSION" =~ $VERSION_PATTERN ]] ||
   [[ ! "$PNPM_VERSION" =~ $VERSION_PATTERN ]] || [[ ! "$BUN_VERSION" =~ $VERSION_PATTERN ]]; then
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
"$PINSET_BIN" list pnpm --available | grep -F "pnpm@$PNPM_VERSION"
"$PINSET_BIN" list bun --available | grep -F "bun@$BUN_VERSION"
"$PINSET_BIN" use "pnpm@$PNPM_VERSION" --global
"$PINSET_BIN" use "bun@$BUN_VERSION" --global
"$PINSET_BIN" current node | tee global-current.txt
grep -F "Node.js $GLOBAL_VERSION" global-current.txt
grep -F '来源=全局' global-current.txt
"$PINSET_BIN" exec -- node --version | grep -Fx "v$GLOBAL_VERSION"
"$PINSET_BIN" exec -- npm --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
"$PINSET_BIN" exec -- npx --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
"$PINSET_BIN" exec -- corepack --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
"$PINSET_BIN" exec -- pnpm --version | grep -Fx "$PNPM_VERSION"
"$PINSET_BIN" exec -- bun --version | grep -Fx "$BUN_VERSION"
"$PINSET_BIN" exec -- bunx --version | grep -Fx "$BUN_VERSION"

SHIM_DIR="$("$PINSET_BIN" shim path)"
export PATH="$SHIM_DIR:$PATH"
node --version | grep -Fx "v$GLOBAL_VERSION"
npm --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
npx --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
corepack --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
pnpm --version | grep -Fx "$PNPM_VERSION"
bun --version | grep -Fx "$BUN_VERSION"
bunx --version | grep -Fx "$BUN_VERSION"

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
"$PINSET_BIN" exec -- npx --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
"$PINSET_BIN" exec -- corepack --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
node --version | grep -Fx "v$PROJECT_VERSION"
npm --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
npx --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
corepack --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
pnpm --version | grep -Fx "$PNPM_VERSION"
bun --version | grep -Fx "$BUN_VERSION"
bunx --version | grep -Fx "$BUN_VERSION"
set +e
PNPM_CHILD_NODE_OUTPUT="$(pnpm exec node --version 2>&1)"
PNPM_CHILD_NODE_STATUS=$?
set -e
printf 'pnpm exec node --version => status=%s output=%s\n' \
  "$PNPM_CHILD_NODE_STATUS" "$PNPM_CHILD_NODE_OUTPUT"
test "$PNPM_CHILD_NODE_STATUS" -eq 0
printf '%s\n' "$PNPM_CHILD_NODE_OUTPUT" | grep -Fx "v$PROJECT_VERSION"

cd "$TEST_ROOT"
"$PINSET_BIN" current node | tee restored-global-current.txt
grep -F "Node.js $GLOBAL_VERSION" restored-global-current.txt
grep -F '来源=全局' restored-global-current.txt
"$PINSET_BIN" exec -- node --version | grep -Fx "v$GLOBAL_VERSION"
node --version | grep -Fx "v$GLOBAL_VERSION"
pnpm --version | grep -Fx "$PNPM_VERSION"
bun --version | grep -Fx "$BUN_VERSION"

echo "Unix real Node, pnpm and Bun acceptance passed"
