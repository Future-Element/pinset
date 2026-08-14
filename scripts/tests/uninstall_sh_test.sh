#!/bin/sh

set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
TEST_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/pinset-uninstall-test.XXXXXX")

cleanup() {
    rm -rf -- "$TEST_ROOT"
}
on_signal() {
    trap - EXIT HUP INT TERM
    cleanup
    exit 130
}
trap cleanup EXIT
trap on_signal HUP INT TERM

TEST_HOME="$TEST_ROOT/user home"
INSTALL_DIR="$TEST_ROOT/install dir"
PINSET_DATA_HOME="$TEST_ROOT/data/pinset-home"
SHIM_DIR="$TEST_ROOT/custom shims"
PATH_DIR="$TEST_ROOT/path shims"
PROJECT_DIR="$TEST_ROOT/project"
mkdir -p \
    "$TEST_HOME" \
    "$INSTALL_DIR" \
    "$PINSET_DATA_HOME/installs/node/24.0.0/linux-x86_64" \
    "$PINSET_DATA_HOME/installs/python/3.14.0/linux-x86_64" \
    "$SHIM_DIR" \
    "$PATH_DIR" \
    "$PROJECT_DIR"

cat > "$INSTALL_DIR/pinset" <<'EOF'
#!/bin/sh
printf 'pinset test\n'
EOF
cat > "$INSTALL_DIR/pinset-shim" <<'EOF'
#!/bin/sh
printf 'pinset shim test\n'
EOF
chmod 755 "$INSTALL_DIR/pinset" "$INSTALL_DIR/pinset-shim"

ln -s "$INSTALL_DIR/pinset-shim" "$INSTALL_DIR/node"
ln "$INSTALL_DIR/pinset-shim" "$INSTALL_DIR/npm"
cp "$INSTALL_DIR/pinset-shim" "$INSTALL_DIR/npx"
ln -s "$INSTALL_DIR/pinset-shim" "$SHIM_DIR/corepack"
ln -s "$INSTALL_DIR/pinset-shim" "$SHIM_DIR/go"
ln -s "$INSTALL_DIR/pinset-shim" "$SHIM_DIR/flutter"
ln -s "$INSTALL_DIR/pinset-shim" "$SHIM_DIR/dart"
ln -s "$INSTALL_DIR/pinset-shim" "$SHIM_DIR/rustc"
ln "$INSTALL_DIR/pinset-shim" "$INSTALL_DIR/cargo"
cp "$INSTALL_DIR/pinset-shim" "$PATH_DIR/npx"
printf 'foreign command\n' > "$PATH_DIR/python"
printf 'project config\n' > "$PROJECT_DIR/pinset.toml"
printf 'project lock\n' > "$PROJECT_DIR/pinset.lock"
printf 'runtime\n' > "$PINSET_DATA_HOME/installs/node/24.0.0/linux-x86_64/node"
printf 'runtime\n' > "$PINSET_DATA_HOME/installs/python/3.14.0/linux-x86_64/python"

if HOME="$TEST_HOME" PATH="$PATH_DIR:$PATH" sh "$ROOT/uninstall.sh" \
    --install-dir "$INSTALL_DIR" \
    --pinset-home "$PINSET_DATA_HOME" \
    --allow-nonstandard-home \
    --shim-dir "$SHIM_DIR" \
    --shim-binary "$INSTALL_DIR/pinset-shim"
then
    printf 'uninstall without --yes unexpectedly succeeded\n' >&2
    exit 1
fi
[ -e "$INSTALL_DIR/pinset" ]
[ -e "$INSTALL_DIR/node" ]
[ -e "$PINSET_DATA_HOME" ]

HOME="$TEST_HOME" PATH="$PATH_DIR:$PATH" sh "$ROOT/uninstall.sh" --dry-run \
    --install-dir "$INSTALL_DIR" \
    --pinset-home "$PINSET_DATA_HOME" \
    --allow-nonstandard-home \
    --shim-dir "$SHIM_DIR" \
    --shim-binary "$INSTALL_DIR/pinset-shim"
[ -e "$INSTALL_DIR/pinset" ]
[ -e "$INSTALL_DIR/node" ]
[ -e "$PINSET_DATA_HOME" ]

if HOME="$TEST_HOME" sh "$ROOT/uninstall.sh" --yes \
    --install-dir "$INSTALL_DIR" \
    --pinset-home "$TEST_HOME" \
    --shim-binary "$INSTALL_DIR/pinset-shim"
then
    printf 'broad PINSET_HOME unexpectedly succeeded\n' >&2
    exit 1
fi
[ -d "$TEST_HOME" ]

if HOME="$TEST_HOME" sh "$ROOT/uninstall.sh" --yes \
    --install-dir "$INSTALL_DIR" \
    --pinset-home "$PINSET_DATA_HOME/child/.." \
    --allow-nonstandard-home \
    --shim-binary "$INSTALL_DIR/pinset-shim"
then
    printf 'PINSET_HOME traversal unexpectedly succeeded\n' >&2
    exit 1
fi
[ -e "$PINSET_DATA_HOME" ]

HOME="$TEST_HOME" PATH="$PATH_DIR:$PATH" sh "$ROOT/uninstall.sh" --yes \
    --install-dir "$INSTALL_DIR" \
    --pinset-home "$PINSET_DATA_HOME" \
    --allow-nonstandard-home \
    --shim-dir "$SHIM_DIR" \
    --shim-binary "$INSTALL_DIR/pinset-shim"

[ ! -e "$INSTALL_DIR/pinset" ]
[ ! -e "$INSTALL_DIR/pinset-shim" ]
[ ! -e "$INSTALL_DIR/node" ]
[ ! -e "$INSTALL_DIR/npm" ]
[ ! -e "$INSTALL_DIR/npx" ]
[ ! -e "$SHIM_DIR/corepack" ]
[ ! -e "$SHIM_DIR/go" ]
[ ! -e "$SHIM_DIR/flutter" ]
[ ! -e "$SHIM_DIR/dart" ]
[ ! -e "$SHIM_DIR/rustc" ]
[ ! -e "$INSTALL_DIR/cargo" ]
[ ! -e "$PATH_DIR/npx" ]
[ ! -e "$PINSET_DATA_HOME" ]
[ -f "$PATH_DIR/python" ]
[ -f "$PROJECT_DIR/pinset.toml" ]
[ -f "$PROJECT_DIR/pinset.lock" ]
[ -d "$INSTALL_DIR" ]
[ -d "$SHIM_DIR" ]

printf 'uninstall.sh isolated tests passed\n'
