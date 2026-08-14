#!/bin/sh

set -eu

REPOSITORY="Future-Element/pinset"
DEFAULT_VERSION="0.4.2"
VERSION="${PINSET_VERSION:-$DEFAULT_VERSION}"
INSTALL_DIR="${PINSET_INSTALL_DIR:-}"
TEMP_ROOT=""
PINSET_TEMP=""
SHIM_TEMP=""

usage() {
    cat <<'EOF'
Install Pinset from a GitHub Release.

Usage:
  install.sh [--version VERSION] [--install-dir DIRECTORY]

Options:
  --version VERSION       Install an exact release, for example 0.4.2.
                          Default: the recommended release embedded in this script.
  --install-dir DIRECTORY Install binaries here. Default: $HOME/.local/bin.
  -h, --help              Show this help.

Environment equivalents:
  PINSET_VERSION
  PINSET_INSTALL_DIR
EOF
}

fail() {
    printf 'pinset installer: %s\n' "$*" >&2
    exit 1
}

cleanup() {
    if [ -n "$PINSET_TEMP" ]; then
        rm -f -- "$PINSET_TEMP"
    fi
    if [ -n "$SHIM_TEMP" ]; then
        rm -f -- "$SHIM_TEMP"
    fi
    if [ -n "$TEMP_ROOT" ]; then
        rm -rf -- "$TEMP_ROOT"
    fi
}

on_signal() {
    trap - EXIT HUP INT TERM
    cleanup
    exit 130
}

trap cleanup EXIT
trap on_signal HUP INT TERM

while [ "$#" -gt 0 ]; do
    case "$1" in
        --version)
            [ "$#" -ge 2 ] || fail "--version requires a value"
            VERSION=$2
            shift 2
            ;;
        --install-dir)
            [ "$#" -ge 2 ] || fail "--install-dir requires a value"
            INSTALL_DIR=$2
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            fail "unknown option: $1"
            ;;
    esac
done

if [ -z "$INSTALL_DIR" ]; then
    [ -n "${HOME:-}" ] || fail "HOME is not set; pass --install-dir"
    INSTALL_DIR="$HOME/.local/bin"
fi

case "$INSTALL_DIR" in
    /*) ;;
    *) fail "install directory must be an absolute path: $INSTALL_DIR" ;;
esac

VERSION=${VERSION#v}
case "$VERSION" in
    ''|[!0-9A-Za-z]*|*[!0-9A-Za-z.-]*) fail "invalid version: $VERSION" ;;
esac
RELEASE_BASE_URL="https://github.com/$REPOSITORY/releases/download/v$VERSION"
RELEASE_LABEL="v$VERSION"

# Reserved for the repository's offline installer tests. It is deliberately
# unavailable unless test mode is explicitly enabled.
if [ "${PINSET_INSTALL_TEST_MODE:-}" = "1" ]; then
    [ -n "${PINSET_TEST_RELEASE_BASE_URL:-}" ] || fail "test release URL is missing"
    RELEASE_BASE_URL=$PINSET_TEST_RELEASE_BASE_URL
fi

OS=$(uname -s)
ARCH=$(uname -m)
if [ "${PINSET_INSTALL_TEST_MODE:-}" = "1" ]; then
    OS=${PINSET_TEST_UNAME_S:-$OS}
    ARCH=${PINSET_TEST_UNAME_M:-$ARCH}
fi
case "$OS:$ARCH" in
    Linux:x86_64|Linux:amd64)
        ARCHIVE="pinset-linux-x86_64.tar.gz"
        ;;
    Darwin:arm64|Darwin:aarch64)
        ARCHIVE="pinset-macos-aarch64.tar.gz"
        ;;
    Darwin:x86_64|Darwin:amd64)
        fail "macOS Intel is not published yet; use an Apple Silicon shell or build from source"
        ;;
    Linux:aarch64|Linux:arm64)
        fail "Linux arm64 is not published yet"
        ;;
    *)
        fail "unsupported platform: $OS $ARCH"
        ;;
esac

for command in curl tar awk mktemp chmod mv cp mkdir rm; do
    command -v "$command" >/dev/null 2>&1 || fail "required command not found: $command"
done

download() {
    url=$1
    destination=$2
    case "$url" in
        https://*)
            curl --proto '=https' --tlsv1.2 --fail --location --silent --show-error \
                --output "$destination" "$url"
            ;;
        file://*)
            [ "${PINSET_INSTALL_TEST_MODE:-}" = "1" ] || fail "non-HTTPS download URL rejected"
            curl --fail --location --silent --show-error --output "$destination" "$url"
            ;;
        *)
            fail "non-HTTPS download URL rejected"
            ;;
    esac
}

TEMP_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/pinset-install.XXXXXX")
ARCHIVE_PATH="$TEMP_ROOT/$ARCHIVE"
CHECKSUMS_PATH="$TEMP_ROOT/SHA256SUMS"
EXTRACT_DIR="$TEMP_ROOT/extract"
mkdir -p "$EXTRACT_DIR"

printf 'Downloading Pinset %s for %s %s...\n' "$RELEASE_LABEL" "$OS" "$ARCH"
download "$RELEASE_BASE_URL/$ARCHIVE" "$ARCHIVE_PATH"
download "$RELEASE_BASE_URL/SHA256SUMS" "$CHECKSUMS_PATH"

EXPECTED_HASH=$(awk -v archive="$ARCHIVE" '
    $2 == archive {
        if (found) exit 2
        print $1
        found = 1
    }
    END { if (!found) exit 1 }
' "$CHECKSUMS_PATH") || fail "SHA256SUMS does not contain exactly one entry for $ARCHIVE"

printf '%s\n' "$EXPECTED_HASH" | awk '
    length($0) == 64 && $0 !~ /[^0-9A-Fa-f]/ { valid = 1 }
    END { exit !valid }
' || fail "invalid SHA-256 value for $ARCHIVE"

if command -v sha256sum >/dev/null 2>&1; then
    ACTUAL_HASH=$(sha256sum "$ARCHIVE_PATH" | awk '{ print $1 }')
elif command -v shasum >/dev/null 2>&1; then
    ACTUAL_HASH=$(shasum -a 256 "$ARCHIVE_PATH" | awk '{ print $1 }')
elif command -v openssl >/dev/null 2>&1; then
    ACTUAL_HASH=$(openssl dgst -sha256 "$ARCHIVE_PATH" | awk '{ print $NF }')
else
    fail "sha256sum, shasum, or openssl is required to verify the release"
fi

EXPECTED_HASH=$(printf '%s' "$EXPECTED_HASH" | awk '{ print tolower($0) }')
ACTUAL_HASH=$(printf '%s' "$ACTUAL_HASH" | awk '{ print tolower($0) }')
[ "$ACTUAL_HASH" = "$EXPECTED_HASH" ] || fail "SHA-256 mismatch for $ARCHIVE"

ENTRY_LIST="$TEMP_ROOT/archive-entries"
tar -tzf "$ARCHIVE_PATH" > "$ENTRY_LIST" || fail "cannot list $ARCHIVE"
awk '
    $0 == "pinset" { pinset += 1; next }
    $0 == "pinset-shim" { shim += 1; next }
    { exit 2 }
    END { if (NR != 2 || pinset != 1 || shim != 1) exit 1 }
' "$ENTRY_LIST" || fail "release archive must contain exactly pinset and pinset-shim"

tar -xzf "$ARCHIVE_PATH" -C "$EXTRACT_DIR"
for binary in pinset pinset-shim; do
    [ -f "$EXTRACT_DIR/$binary" ] || fail "release archive is missing $binary"
    [ ! -L "$EXTRACT_DIR/$binary" ] || fail "release archive contains a symbolic-link binary: $binary"
done

mkdir -p "$INSTALL_DIR"
[ -d "$INSTALL_DIR" ] || fail "install destination is not a directory: $INSTALL_DIR"
[ -w "$INSTALL_DIR" ] || fail "install destination is not writable: $INSTALL_DIR"

PINSET_TEMP=$(mktemp "$INSTALL_DIR/.pinset.XXXXXX")
SHIM_TEMP=$(mktemp "$INSTALL_DIR/.pinset-shim.XXXXXX")
cp "$EXTRACT_DIR/pinset" "$PINSET_TEMP"
cp "$EXTRACT_DIR/pinset-shim" "$SHIM_TEMP"
chmod 755 "$PINSET_TEMP" "$SHIM_TEMP"

# Publish the companion first and the CLI last. Both files were fully copied
# and verified before either existing executable is replaced.
mv -f "$SHIM_TEMP" "$INSTALL_DIR/pinset-shim"
SHIM_TEMP=""
mv -f "$PINSET_TEMP" "$INSTALL_DIR/pinset"
PINSET_TEMP=""

printf 'Installed %s\n' "$INSTALL_DIR/pinset"
printf 'Installed %s\n' "$INSTALL_DIR/pinset-shim"
"$INSTALL_DIR/pinset" --version

case ":${PATH:-}:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        printf '\nAdd Pinset to the current shell:\n'
        printf '  export PATH="%s:$PATH"\n' "$INSTALL_DIR"
        ;;
esac

printf '\nInstalled only the Pinset CLI and its runtime-agnostic command router.\n'
printf 'Runtime providers register their own commands only after you select or install that runtime.\n'
printf 'Pinset does not modify shell profiles or install language runtimes automatically.\n'
