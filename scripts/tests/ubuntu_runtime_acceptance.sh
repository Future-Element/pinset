#!/usr/bin/env bash
set -euo pipefail

: "${PINSET_BIN:?PINSET_BIN must point to the release pinset binary}"

GLOBAL_VERSION="${PINSET_GLOBAL_VERSION:-24.0.0}"
PROJECT_VERSION="${PINSET_PROJECT_VERSION:-22.0.0}"
BUN_VERSION="${PINSET_BUN_VERSION:-1.3.14}"
GLOBAL_PNPM_SELECTOR="${PINSET_GLOBAL_PNPM_SELECTOR:-latest}"
PROJECT_PNPM_SELECTOR="${PINSET_PROJECT_PNPM_SELECTOR:-10}"
PROJECT_BUN_SELECTOR="${PINSET_PROJECT_BUN_SELECTOR:-1.2}"
GLOBAL_GO_SELECTOR="${PINSET_GLOBAL_GO_SELECTOR:-latest}"
PROJECT_GO_SELECTOR="${PINSET_PROJECT_GO_SELECTOR:-1.24}"
GLOBAL_PYTHON_SELECTOR="${PINSET_GLOBAL_PYTHON_SELECTOR:-latest}"
PROJECT_PYTHON_SELECTOR="${PINSET_PROJECT_PYTHON_SELECTOR:-3.13}"
GLOBAL_JAVA_SELECTOR="${PINSET_GLOBAL_JAVA_SELECTOR:-lts}"
GLOBAL_RUST_SELECTOR="${PINSET_GLOBAL_RUST_SELECTOR:-latest}"
GLOBAL_FLUTTER_SELECTOR="${PINSET_GLOBAL_FLUTTER_SELECTOR:-latest}"
PROJECT_FLUTTER_SELECTOR="${PINSET_PROJECT_FLUTTER_SELECTOR:-3.44}"
SKIP_FLUTTER_RUNTIME="${PINSET_SKIP_FLUTTER_RUNTIME:-0}"
VERSION_PATTERN='^[0-9]+\.[0-9]+\.[0-9]+$'

if [[ ! "$GLOBAL_VERSION" =~ $VERSION_PATTERN ]] || [[ ! "$PROJECT_VERSION" =~ $VERSION_PATTERN ]] ||
   [[ ! "$BUN_VERSION" =~ $VERSION_PATTERN ]]; then
  echo "acceptance versions must use x.y.z" >&2
  exit 2
fi
if [[ ! -x "$PINSET_BIN" ]]; then
  echo "PINSET_BIN is not executable: $PINSET_BIN" >&2
  exit 2
fi
if [[ "$SKIP_FLUTTER_RUNTIME" != "0" && "$SKIP_FLUTTER_RUNTIME" != "1" ]]; then
  echo "PINSET_SKIP_FLUTTER_RUNTIME must be 0 or 1" >&2
  exit 2
fi

assert_pinset_pip_routes_to_python() {
  local label="$1"
  local expected actual pip_command
  expected="$("$PINSET_BIN" exec -- python -m pip --version)"
  printf '%s: %s\n' "$label python -m pip" "$expected"
  printf '%s\n' "$expected" | grep -E '^pip [0-9]+(\.[0-9]+)+'
  for pip_command in pip pip3; do
    actual="$("$PINSET_BIN" exec -- "$pip_command" --version)"
    test "$actual" = "$expected"
  done
}

assert_direct_pip_routes_to_python() {
  local label="$1"
  local expected actual pip_command
  expected="$(python -m pip --version)"
  printf '%s: %s\n' "$label python -m pip" "$expected"
  printf '%s\n' "$expected" | grep -E '^pip [0-9]+(\.[0-9]+)+'
  for pip_command in pip pip3; do
    actual="$("$pip_command" --version)"
    test "$actual" = "$expected"
  done
}

TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEST_ROOT"' EXIT

export PINSET_HOME="$TEST_ROOT/pinset-home"
unset PINSET_LANG
unset GOTOOLCHAIN
unset VIRTUAL_ENV
unset PYTHONHOME
unset FLUTTER_ROOT
unset FLUTTER_SUPPRESS_ANALYTICS
unset JAVA_HOME
unset CLASSPATH
unset JAVA_TOOL_OPTIONS
unset JDK_JAVA_OPTIONS
unset _JAVA_OPTIONS

cd "$TEST_ROOT"

"$PINSET_BIN" --lang zh-CN | tee language.txt
grep -F '语言已切换为中文' language.txt
grep -F 'language = "zh-CN"' "$PINSET_HOME/settings.toml"

"$PINSET_BIN" use "node@$GLOBAL_VERSION" --global
"$PINSET_BIN" list pnpm --available | grep -E '^pnpm@10\.'
"$PINSET_BIN" list bun --available | grep -F "bun@$BUN_VERSION"
"$PINSET_BIN" list go --available | grep -E '^go@[0-9]+\.[0-9]+\.[0-9]+$'
"$PINSET_BIN" list python --available | grep -E '^python@[0-9]+\.[0-9]+\.[0-9]+\+[0-9]{8} '
"$PINSET_BIN" list java --available | grep -E '^java@[0-9]+\.[0-9]+\.[0-9]+(\.[0-9]+)?\+[0-9]+ temurin (lts|ga) '
"$PINSET_BIN" list rust --available | grep -E '^rust@[0-9]+\.[0-9]+\.[0-9]+ stable \([0-9]{4}-[0-9]{2}-[0-9]{2}\)$'
FLUTTER_RELEASES="$("$PINSET_BIN" list flutter --available)"
printf '%s\n' "$FLUTTER_RELEASES" | grep -E '^flutter@[0-9]+\.[0-9]+\.[0-9]+ dart@[0-9]+\.[0-9]+\.[0-9]+ stable$'
PROJECT_FLUTTER_VERSION="$(
  printf '%s\n' "$FLUTTER_RELEASES" |
    sed -n "s/^flutter@\(${PROJECT_FLUTTER_SELECTOR//./\.}\.[0-9][0-9]*\) .*/\1/p" |
    head -n 1
)"
test -n "$PROJECT_FLUTTER_VERSION"
"$PINSET_BIN" global "pnpm@$GLOBAL_PNPM_SELECTOR"
GLOBAL_PNPM_VERSION="$("$PINSET_BIN" exec -- pnpm --version)"
printf '%s\n' "$GLOBAL_PNPM_VERSION" | grep -E '^11\.'
"$PINSET_BIN" use "bun@$BUN_VERSION" --global
"$PINSET_BIN" global "go@$GLOBAL_GO_SELECTOR"
GLOBAL_GO_VERSION="$("$PINSET_BIN" exec -- go version | sed -E 's/^go version go([^ ]+).*/\1/')"
printf '%s\n' "$GLOBAL_GO_VERSION" | grep -E "$VERSION_PATTERN"
"$PINSET_BIN" global "python@$GLOBAL_PYTHON_SELECTOR"
GLOBAL_PYTHON_VERSION="$("$PINSET_BIN" exec -- python -c 'import sys; print(".".join(map(str, sys.version_info[:3])))')"
printf '%s\n' "$GLOBAL_PYTHON_VERSION" | grep -E "$VERSION_PATTERN"
"$PINSET_BIN" global "java@$GLOBAL_JAVA_SELECTOR"
GLOBAL_JAVA_VERSION="$("$PINSET_BIN" --lang en current java | sed -n 's/^java \([^ ]*\) installed.*/\1/p')"
printf '%s\n' "$GLOBAL_JAVA_VERSION" | grep -E '^[0-9]+\.[0-9]+\.[0-9]+(\.[0-9]+)?\+[0-9]+$'
"$PINSET_BIN" global "rust@$GLOBAL_RUST_SELECTOR"
GLOBAL_RUST_VERSION="$("$PINSET_BIN" exec -- rustc --version | sed -n 's/^rustc \([^ ]*\).*/\1/p')"
printf '%s\n' "$GLOBAL_RUST_VERSION" | grep -E "$VERSION_PATTERN"
cat > PinsetJavaProbe.java <<'JAVA'
public class PinsetJavaProbe {
    public static void main(String[] args) {
        System.out.println("pinset-java-ok");
        System.out.println("java.home=" + System.getProperty("java.home"));
        System.out.println("JAVA_HOME=" + System.getenv("JAVA_HOME"));
    }
}
JAVA
"$PINSET_BIN" exec -- javac PinsetJavaProbe.java
"$PINSET_BIN" exec -- java -cp "$TEST_ROOT" PinsetJavaProbe | tee global-java.txt
grep -Fx 'pinset-java-ok' global-java.txt
GLOBAL_JAVA_HOME="$(sed -n 's/^JAVA_HOME=//p' global-java.txt)"
GLOBAL_JAVA_RUNTIME_HOME="$(sed -n 's/^java\.home=//p' global-java.txt)"
case "$GLOBAL_JAVA_HOME" in
  "$PINSET_HOME/installs/java/$GLOBAL_JAVA_VERSION/"*) ;;
  *)
    echo "global Java reported an unmanaged JAVA_HOME: $GLOBAL_JAVA_HOME" >&2
    exit 1
    ;;
esac
test "$(CDPATH= cd -- "$GLOBAL_JAVA_HOME" && pwd -P)" = "$(CDPATH= cd -- "$GLOBAL_JAVA_RUNTIME_HOME" && pwd -P)"
cat > PinsetRustProbe.rs <<'RUST'
fn main() {
    println!("pinset-rust-ok");
}
RUST
"$PINSET_BIN" exec -- rustc PinsetRustProbe.rs -o pinset-rust-probe
./pinset-rust-probe | grep -Fx 'pinset-rust-ok'
if [[ "$SKIP_FLUTTER_RUNTIME" == "0" ]]; then
  "$PINSET_BIN" global "flutter@$GLOBAL_FLUTTER_SELECTOR"
  GLOBAL_FLUTTER_JSON="$("$PINSET_BIN" exec -- flutter --version --machine)"
  GLOBAL_FLUTTER_VERSION="$(
    printf '%s' "$GLOBAL_FLUTTER_JSON" |
      python3 -c 'import json,sys; print(json.load(sys.stdin)["frameworkVersion"])'
  )"
  GLOBAL_DART_VERSION="$(
    printf '%s' "$GLOBAL_FLUTTER_JSON" |
      python3 -c 'import json,sys; print(json.load(sys.stdin)["dartSdkVersion"])'
  )"
  printf '%s\n' "$GLOBAL_FLUTTER_VERSION" "$GLOBAL_DART_VERSION" | grep -E "$VERSION_PATTERN"
  test "$PROJECT_FLUTTER_VERSION" != "$GLOBAL_FLUTTER_VERSION"
fi
"$PINSET_BIN" current node | tee global-current.txt
grep -F "Node.js $GLOBAL_VERSION" global-current.txt
grep -F '来源=全局' global-current.txt
"$PINSET_BIN" exec -- node --version | grep -Fx "v$GLOBAL_VERSION"
"$PINSET_BIN" exec -- npm --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
"$PINSET_BIN" exec -- npx --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
"$PINSET_BIN" exec -- corepack --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
"$PINSET_BIN" exec -- pnpm --version | grep -Fx "$GLOBAL_PNPM_VERSION"
"$PINSET_BIN" exec -- bun --version | grep -Fx "$BUN_VERSION"
"$PINSET_BIN" exec -- bunx --version | grep -Fx "$BUN_VERSION"
"$PINSET_BIN" exec -- go version | grep -F "go version go$GLOBAL_GO_VERSION"
"$PINSET_BIN" exec -- go env GOTOOLCHAIN | grep -Fx 'local'
"$PINSET_BIN" exec -- python3 -c 'import sys; print(".".join(map(str, sys.version_info[:3])))' | grep -Fx "$GLOBAL_PYTHON_VERSION"
assert_pinset_pip_routes_to_python 'global pinset exec'
"$PINSET_BIN" exec -- javac -version 2>&1 | grep -F 'javac '
"$PINSET_BIN" exec -- java -version 2>&1 | grep -F 'Temurin'
"$PINSET_BIN" exec -- rustc --version | grep -F "rustc $GLOBAL_RUST_VERSION"
"$PINSET_BIN" exec -- cargo --version | grep -E '^cargo [0-9]+\.[0-9]+\.[0-9]+'
"$PINSET_BIN" exec -- rustfmt --version | grep -E '^rustfmt [0-9]+\.[0-9]+\.[0-9]+'
"$PINSET_BIN" exec -- go env GOROOT | grep -F "$PINSET_HOME/installs/go/$GLOBAL_GO_VERSION/"
printf 'package p\nfunc f( ){ }\n' | "$PINSET_BIN" exec -- gofmt | grep -F 'func f() {}'
if [[ "$SKIP_FLUTTER_RUNTIME" == "0" ]]; then
  GLOBAL_FLUTTER_PATH="$("$PINSET_BIN" which flutter)"
  GLOBAL_DART_PATH="$("$PINSET_BIN" which dart)"
  test "$(dirname "$GLOBAL_FLUTTER_PATH")" = "$(dirname "$GLOBAL_DART_PATH")"
  GLOBAL_FLUTTER_ROOT="$(CDPATH= cd -- "$(dirname "$GLOBAL_FLUTTER_PATH")/.." && pwd -P)"
  printf "import 'dart:io'; void main() => print(Platform.environment['FLUTTER_ROOT']);\n" > verify_flutter_env.dart
  "$PINSET_BIN" exec -- dart verify_flutter_env.dart | grep -Fx "$GLOBAL_FLUTTER_ROOT"
  "$PINSET_BIN" exec -- dart --version 2>&1 | grep -F "Dart SDK version: $GLOBAL_DART_VERSION"
fi

SHIM_DIR="$("$PINSET_BIN" shim path)"
export PATH="$SHIM_DIR:$PATH"
node --version | grep -Fx "v$GLOBAL_VERSION"
npm --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
npx --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
corepack --version | grep -E '^[0-9]+\.[0-9]+\.[0-9]+'
pnpm --version | grep -Fx "$GLOBAL_PNPM_VERSION"
bun --version | grep -Fx "$BUN_VERSION"
bunx --version | grep -Fx "$BUN_VERSION"
go version | grep -F "go version go$GLOBAL_GO_VERSION"
go env GOTOOLCHAIN | grep -Fx 'local'
python -c 'import sys; print(".".join(map(str, sys.version_info[:3])))' | grep -Fx "$GLOBAL_PYTHON_VERSION"
python3 -c 'import sys; print(".".join(map(str, sys.version_info[:3])))' | grep -Fx "$GLOBAL_PYTHON_VERSION"
assert_direct_pip_routes_to_python 'global direct'
javac -version 2>&1 | grep -F 'javac '
java -cp "$TEST_ROOT" PinsetJavaProbe | grep -Fx 'pinset-java-ok'
rustc --version | grep -F "rustc $GLOBAL_RUST_VERSION"
cargo --version | grep -E '^cargo [0-9]+\.[0-9]+\.[0-9]+'
if [[ "$SKIP_FLUTTER_RUNTIME" == "0" ]]; then
  flutter --version --machine | python3 -c 'import json,sys; print(json.load(sys.stdin)["frameworkVersion"])' | grep -Fx "$GLOBAL_FLUTTER_VERSION"
  dart --version 2>&1 | grep -F "Dart SDK version: $GLOBAL_DART_VERSION"
  for mutation in upgrade downgrade channel; do
    if flutter "$mutation" > flutter-mutation.txt 2>&1; then
      echo "managed flutter $mutation unexpectedly succeeded" >&2
      exit 1
    fi
    grep -F "refusing to run \`flutter $mutation\` against a Pinset-managed Flutter SDK" flutter-mutation.txt
  done
fi
"$PINSET_BIN" cache clean

mkdir project
cd project
printf '{"private":true}\n' > package.json
"$PINSET_BIN" init
"$PINSET_BIN" use "node@$PROJECT_VERSION"
"$PINSET_BIN" use "pnpm@$PROJECT_PNPM_SELECTOR"
PROJECT_PNPM_VERSION="$(pnpm --version)"
printf '%s\n' "$PROJECT_PNPM_VERSION" | grep -E '^10\.'
"$PINSET_BIN" uninstall "pnpm@$PROJECT_PNPM_VERSION" --force
"$PINSET_BIN" use "bun@$PROJECT_BUN_SELECTOR"
PROJECT_BUN_VERSION="$(bun --version)"
printf '%s\n' "$PROJECT_BUN_VERSION" | grep -E '^1\.2\.'
"$PINSET_BIN" use "go@$PROJECT_GO_SELECTOR"
PROJECT_GO_VERSION="$(go version | sed -E 's/^go version go([^ ]+).*/\1/')"
printf '%s\n' "$PROJECT_GO_VERSION" | grep -E "^${PROJECT_GO_SELECTOR//./\.}\."
"$PINSET_BIN" use "python@$PROJECT_PYTHON_SELECTOR"
PROJECT_PYTHON_VERSION="$(python -c 'import sys; print(".".join(map(str, sys.version_info[:3])))')"
printf '%s\n' "$PROJECT_PYTHON_VERSION" | grep -E "^${PROJECT_PYTHON_SELECTOR//./\.}\."
"$PINSET_BIN" use "java@$GLOBAL_JAVA_VERSION" --no-install
"$PINSET_BIN" use "rust@$GLOBAL_RUST_VERSION" --no-install
if [[ "$SKIP_FLUTTER_RUNTIME" == "0" ]]; then
  "$PINSET_BIN" use "flutter@$PROJECT_FLUTTER_VERSION" --no-install
fi
"$PINSET_BIN" install --locked
if [[ "$SKIP_FLUTTER_RUNTIME" == "0" ]]; then
  PROJECT_FLUTTER_JSON="$(flutter --version --machine)"
  PROJECT_FLUTTER_ACTUAL="$(
    printf '%s' "$PROJECT_FLUTTER_JSON" |
      python3 -c 'import json,sys; print(json.load(sys.stdin)["frameworkVersion"])'
  )"
  PROJECT_DART_VERSION="$(
    printf '%s' "$PROJECT_FLUTTER_JSON" |
      python3 -c 'import json,sys; print(json.load(sys.stdin)["dartSdkVersion"])'
  )"
  test "$PROJECT_FLUTTER_ACTUAL" = "$PROJECT_FLUTTER_VERSION"
fi
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
pnpm --version | grep -Fx "$PROJECT_PNPM_VERSION"
bun --version | grep -Fx "$PROJECT_BUN_VERSION"
bunx --version | grep -Fx "$PROJECT_BUN_VERSION"
go version | grep -F "go version go$PROJECT_GO_VERSION"
go env GOTOOLCHAIN | grep -Fx 'local'
go env GOROOT | grep -F "$PINSET_HOME/installs/go/$PROJECT_GO_VERSION/"
PROJECT_VENV="$(CDPATH= cd -- .venv && pwd -P)"
python -c 'import os,sys; print(os.path.realpath(sys.prefix))' | grep -Fx "$PROJECT_VENV"
"$PINSET_BIN" exec -- python3 -c 'import os; print(os.environ["VIRTUAL_ENV"])' | grep -F '/.venv'
assert_pinset_pip_routes_to_python 'project pinset exec'
assert_direct_pip_routes_to_python 'project direct'
test -f .venv/.pinset-venv.toml
javac "$TEST_ROOT/PinsetJavaProbe.java"
java -cp "$TEST_ROOT" PinsetJavaProbe | tee project-java.txt
grep -Fx 'pinset-java-ok' project-java.txt
grep -F "JAVA_HOME=$PINSET_HOME/installs/java/$GLOBAL_JAVA_VERSION/" project-java.txt
"$PINSET_BIN" --lang en current java | grep -F "java $GLOBAL_JAVA_VERSION installed"
"$PINSET_BIN" --lang en current rustc | grep -F "rust $GLOBAL_RUST_VERSION installed"
rustc "$TEST_ROOT/PinsetRustProbe.rs" -o project-rust-probe
./project-rust-probe | grep -Fx 'pinset-rust-ok'
if [[ "$SKIP_FLUTTER_RUNTIME" == "0" ]]; then
  PROJECT_FLUTTER_PATH="$("$PINSET_BIN" which flutter)"
  PROJECT_DART_PATH="$("$PINSET_BIN" which dart)"
  test "$(dirname "$PROJECT_FLUTTER_PATH")" = "$(dirname "$PROJECT_DART_PATH")"
  PROJECT_FLUTTER_ROOT="$(CDPATH= cd -- "$(dirname "$PROJECT_FLUTTER_PATH")/.." && pwd -P)"
  dart "$TEST_ROOT/verify_flutter_env.dart" | grep -Fx "$PROJECT_FLUTTER_ROOT"
  dart --version 2>&1 | grep -F "Dart SDK version: $PROJECT_DART_VERSION"
fi
"$PINSET_BIN" cache clean
"$PINSET_BIN" install --locked | tee locked-reuse.txt
grep -F "java@$GLOBAL_JAVA_VERSION is already installed" locked-reuse.txt
grep -F "rust@$GLOBAL_RUST_VERSION is already installed" locked-reuse.txt
if [[ "$SKIP_FLUTTER_RUNTIME" == "0" ]]; then
  grep -F "flutter@$PROJECT_FLUTTER_VERSION is already installed" locked-reuse.txt
fi
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
pnpm --version | grep -Fx "$GLOBAL_PNPM_VERSION"
bun --version | grep -Fx "$BUN_VERSION"
go version | grep -F "go version go$GLOBAL_GO_VERSION"
go env GOTOOLCHAIN | grep -Fx 'local'
python -c 'import sys; print(".".join(map(str, sys.version_info[:3])))' | grep -Fx "$GLOBAL_PYTHON_VERSION"
assert_direct_pip_routes_to_python 'restored global direct'
java -cp "$TEST_ROOT" PinsetJavaProbe | grep -Fx 'pinset-java-ok'
rustc --version | grep -F "rustc $GLOBAL_RUST_VERSION"
cargo --version | grep -E '^cargo [0-9]+\.[0-9]+\.[0-9]+'
if [[ "$SKIP_FLUTTER_RUNTIME" == "0" ]]; then
  flutter --version --machine | python3 -c 'import json,sys; print(json.load(sys.stdin)["frameworkVersion"])' | grep -Fx "$GLOBAL_FLUTTER_VERSION"
  dart --version 2>&1 | grep -F "Dart SDK version: $GLOBAL_DART_VERSION"
fi

if [[ "$SKIP_FLUTTER_RUNTIME" == "0" ]]; then
  echo "Unix real Node, pnpm, Bun, Go, Python, Java, Rust and Flutter acceptance passed"
else
  echo "Unix real Node, pnpm, Bun, Go, Python, Java and Rust acceptance passed; Flutter runtime download skipped"
fi
