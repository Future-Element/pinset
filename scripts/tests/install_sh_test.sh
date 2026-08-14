#!/bin/sh

set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
TEST_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/pinset-install-test.XXXXXX")

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

case "$(uname -s):$(uname -m)" in
    Linux:x86_64|Linux:amd64) ARCHIVE="pinset-linux-x86_64.tar.gz" ;;
    Darwin:arm64|Darwin:aarch64) ARCHIVE="pinset-macos-aarch64.tar.gz" ;;
    *)
        printf 'installer test skipped on unsupported host\n'
        exit 0
        ;;
esac

RELEASE_DIR="$TEST_ROOT/release"
PACKAGE_DIR="$TEST_ROOT/package"
INSTALL_DIR="$TEST_ROOT/install dir"
mkdir -p "$RELEASE_DIR" "$PACKAGE_DIR"

cat > "$PACKAGE_DIR/pinset" <<'EOF'
#!/bin/sh
printf 'pinset 9.8.7-test\n'
EOF
cat > "$PACKAGE_DIR/pinset-shim" <<'EOF'
#!/bin/sh
printf 'fake shim\n'
EOF
chmod 755 "$PACKAGE_DIR/pinset" "$PACKAGE_DIR/pinset-shim"
tar -czf "$RELEASE_DIR/$ARCHIVE" -C "$PACKAGE_DIR" pinset pinset-shim

if command -v sha256sum >/dev/null 2>&1; then
    HASH=$(sha256sum "$RELEASE_DIR/$ARCHIVE" | awk '{ print $1 }')
else
    HASH=$(shasum -a 256 "$RELEASE_DIR/$ARCHIVE" | awk '{ print $1 }')
fi
printf '%s  %s\n' "$HASH" "$ARCHIVE" > "$RELEASE_DIR/SHA256SUMS"

INSTALL_OUTPUT=$(
    PINSET_INSTALL_TEST_MODE=1 \
    PINSET_TEST_RELEASE_BASE_URL="file://$RELEASE_DIR" \
    sh "$ROOT/install.sh" --install-dir "$INSTALL_DIR"
)
printf '%s\n' "$INSTALL_OUTPUT" | grep -F 'Add Pinset to the current shell:'
printf '%s\n' "$INSTALL_OUTPUT" | grep -F "export PATH=\"$INSTALL_DIR:\$PATH\""

[ -x "$INSTALL_DIR/pinset" ]
[ -x "$INSTALL_DIR/pinset-shim" ]
[ "$("$INSTALL_DIR/pinset" --version)" = "pinset 9.8.7-test" ]
INSTALLED_ENTRIES=$(find "$INSTALL_DIR" -mindepth 1 -maxdepth 1 -print | wc -l | awk '{ print $1 }')
[ "$INSTALLED_ENTRIES" = "2" ]
for runtime_command in node npm npx corepack pnpm bun bunx go gofmt flutter dart \
  python python3 pip pip3 java javac jar javadoc javap keytool jshell \
  rustc cargo rustdoc rustfmt cargo-fmt clippy-driver cargo-clippy dotnet; do
    [ ! -e "$INSTALL_DIR/$runtime_command" ]
done

FRONT_OUTPUT=$(
    PATH="$INSTALL_DIR:/usr/bin:/bin" \
    PINSET_INSTALL_TEST_MODE=1 \
    PINSET_TEST_RELEASE_BASE_URL="file://$RELEASE_DIR" \
    sh "$ROOT/install.sh" --install-dir "$INSTALL_DIR"
)
if printf '%s\n' "$FRONT_OUTPUT" | grep -F 'for the current shell:' >/dev/null; then
    printf 'installer unexpectedly requested PATH activation when already first\n' >&2
    exit 1
fi

SHADOWED_OUTPUT=$(
    PATH="/usr/bin:$INSTALL_DIR:/bin" \
    PINSET_INSTALL_TEST_MODE=1 \
    PINSET_TEST_RELEASE_BASE_URL="file://$RELEASE_DIR" \
    sh "$ROOT/install.sh" --install-dir "$INSTALL_DIR"
)
printf '%s\n' "$SHADOWED_OUTPUT" | grep -F 'Pinset is on PATH but may be shadowed by earlier system commands.'
printf '%s\n' "$SHADOWED_OUTPUT" | grep -F 'Move Pinset to the front for the current shell:'

BAD_RELEASE_DIR="$TEST_ROOT/bad-release"
BAD_INSTALL_DIR="$TEST_ROOT/bad-install"
mkdir -p "$BAD_RELEASE_DIR"
cp "$RELEASE_DIR/$ARCHIVE" "$BAD_RELEASE_DIR/$ARCHIVE"
printf '%064d  %s\n' 0 "$ARCHIVE" > "$BAD_RELEASE_DIR/SHA256SUMS"

if PINSET_INSTALL_TEST_MODE=1 \
    PINSET_TEST_RELEASE_BASE_URL="file://$BAD_RELEASE_DIR" \
    sh "$ROOT/install.sh" --version 9.8.7-test --install-dir "$BAD_INSTALL_DIR"
then
    printf 'checksum mismatch unexpectedly succeeded\n' >&2
    exit 1
fi
[ ! -e "$BAD_INSTALL_DIR/pinset" ]
[ ! -e "$BAD_INSTALL_DIR/pinset-shim" ]

EXTRA_RELEASE_DIR="$TEST_ROOT/extra-release"
EXTRA_INSTALL_DIR="$TEST_ROOT/extra-install"
mkdir -p "$EXTRA_RELEASE_DIR"
printf 'unexpected\n' > "$PACKAGE_DIR/unexpected"
tar -czf "$EXTRA_RELEASE_DIR/$ARCHIVE" -C "$PACKAGE_DIR" pinset pinset-shim unexpected
if command -v sha256sum >/dev/null 2>&1; then
    EXTRA_HASH=$(sha256sum "$EXTRA_RELEASE_DIR/$ARCHIVE" | awk '{ print $1 }')
else
    EXTRA_HASH=$(shasum -a 256 "$EXTRA_RELEASE_DIR/$ARCHIVE" | awk '{ print $1 }')
fi
printf '%s  %s\n' "$EXTRA_HASH" "$ARCHIVE" > "$EXTRA_RELEASE_DIR/SHA256SUMS"

if PINSET_INSTALL_TEST_MODE=1 \
    PINSET_TEST_RELEASE_BASE_URL="file://$EXTRA_RELEASE_DIR" \
    sh "$ROOT/install.sh" --version 9.8.7-test --install-dir "$EXTRA_INSTALL_DIR"
then
    printf 'archive with an extra entry unexpectedly succeeded\n' >&2
    exit 1
fi
[ ! -e "$EXTRA_INSTALL_DIR/pinset" ]
[ ! -e "$EXTRA_INSTALL_DIR/pinset-shim" ]

if PINSET_INSTALL_TEST_MODE=1 \
    PINSET_TEST_UNAME_S=Linux \
    PINSET_TEST_UNAME_M=aarch64 \
    PINSET_TEST_RELEASE_BASE_URL="file://$RELEASE_DIR" \
    sh "$ROOT/install.sh" --version 9.8.7-test --install-dir "$TEST_ROOT/unsupported"
then
    printf 'unsupported platform unexpectedly succeeded\n' >&2
    exit 1
fi

printf 'install.sh offline tests passed\n'
