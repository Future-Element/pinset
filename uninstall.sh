#!/bin/sh

set -eu

INSTALL_DIR="${PINSET_INSTALL_DIR:-}"
PINSET_DATA_HOME="${PINSET_HOME:-}"
SHIM_DIR="${PINSET_SHIM_DIR:-}"
SHIM_BINARY="${PINSET_SHIM_BINARY:-}"
ASSUME_YES=0
DRY_RUN=0
ALLOW_NONSTANDARD_HOME=0
ROUTE_COMMANDS="node npm npx corepack pnpm bun bunx go gofmt flutter dart python python3 pip pip3 java javac jar javadoc javap keytool jshell rustc cargo rustdoc rustfmt cargo-fmt clippy-driver cargo-clippy"

usage() {
    cat <<'EOF'
Remove Pinset and all data owned by Pinset for the current user.

Usage:
  uninstall.sh [--yes | --dry-run] [OPTIONS]

Options:
  --yes                    Delete the displayed targets.
  --dry-run                Display targets without deleting anything.
  --install-dir DIRECTORY  Directory containing pinset and pinset-shim.
  --pinset-home DIRECTORY  Pinset data directory. This includes all runtimes.
  --shim-dir DIRECTORY     Additional command-routing directory to inspect.
  --shim-binary FILE       Router used to verify Pinset-owned command entries.
  --allow-nonstandard-home Allow an explicitly supplied custom PINSET_HOME.
  -h, --help               Show this help.

Environment equivalents:
  PINSET_INSTALL_DIR
  PINSET_HOME
  PINSET_SHIM_DIR
  PINSET_SHIM_BINARY

Project pinset.toml and pinset.lock files are never searched for or removed.
Shell profiles, system runtimes, and files owned by other managers are preserved.
EOF
}

fail() {
    printf 'pinset uninstaller: %s\n' "$*" >&2
    exit 1
}

strip_trailing_slashes() {
    path=$1
    while [ "$path" != "/" ] && [ "${path%/}" != "$path" ]; do
        path=${path%/}
    done
    printf '%s\n' "$path"
}

require_absolute_directory() {
    label=$1
    path=$2
    case "$path" in
        /*) ;;
        *) fail "$label must be an absolute path: $path" ;;
    esac
    [ "$path" != "/" ] || fail "$label cannot be the filesystem root"
}

paths_equal() {
    [ "$(strip_trailing_slashes "$1")" = "$(strip_trailing_slashes "$2")" ]
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --yes)
            ASSUME_YES=1
            shift
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        --install-dir)
            [ "$#" -ge 2 ] || fail "--install-dir requires a value"
            INSTALL_DIR=$2
            shift 2
            ;;
        --pinset-home)
            [ "$#" -ge 2 ] || fail "--pinset-home requires a value"
            PINSET_DATA_HOME=$2
            shift 2
            ;;
        --shim-dir)
            [ "$#" -ge 2 ] || fail "--shim-dir requires a value"
            SHIM_DIR=$2
            shift 2
            ;;
        --shim-binary)
            [ "$#" -ge 2 ] || fail "--shim-binary requires a value"
            SHIM_BINARY=$2
            shift 2
            ;;
        --allow-nonstandard-home)
            ALLOW_NONSTANDARD_HOME=1
            shift
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

if [ "$ASSUME_YES" -eq 1 ] && [ "$DRY_RUN" -eq 1 ]; then
    fail "--yes and --dry-run cannot be used together"
fi

if [ -z "$INSTALL_DIR" ]; then
    PINSET_COMMAND=$(command -v pinset 2>/dev/null || true)
    case "$PINSET_COMMAND" in
        /*) INSTALL_DIR=$(dirname -- "$PINSET_COMMAND") ;;
        *)
            [ -n "${HOME:-}" ] || fail "HOME is not set; pass --install-dir"
            INSTALL_DIR="$HOME/.local/bin"
            ;;
    esac
fi

if [ -n "${XDG_DATA_HOME:-}" ]; then
    DEFAULT_PINSET_HOME="$XDG_DATA_HOME/pinset"
else
    [ -n "${HOME:-}" ] || fail "HOME is not set; pass --pinset-home"
    DEFAULT_PINSET_HOME="$HOME/.local/share/pinset"
fi
if [ -z "$PINSET_DATA_HOME" ]; then
    PINSET_DATA_HOME=$DEFAULT_PINSET_HOME
fi

INSTALL_DIR=$(strip_trailing_slashes "$INSTALL_DIR")
PINSET_DATA_HOME=$(strip_trailing_slashes "$PINSET_DATA_HOME")
case "$PINSET_DATA_HOME" in
    */../*|*/..|*/./*|*/.) fail "PINSET_HOME cannot contain . or .. path components: $PINSET_DATA_HOME" ;;
esac
if [ -n "$SHIM_DIR" ]; then
    SHIM_DIR=$(strip_trailing_slashes "$SHIM_DIR")
fi
if [ -z "$SHIM_BINARY" ]; then
    SHIM_BINARY="$INSTALL_DIR/pinset-shim"
fi

require_absolute_directory "install directory" "$INSTALL_DIR"
require_absolute_directory "PINSET_HOME" "$PINSET_DATA_HOME"
DEFAULT_PINSET_HOME=$(strip_trailing_slashes "$DEFAULT_PINSET_HOME")
if ! paths_equal "$PINSET_DATA_HOME" "$DEFAULT_PINSET_HOME" \
    && [ "$ALLOW_NONSTANDARD_HOME" -ne 1 ]; then
    fail "custom PINSET_HOME requires --allow-nonstandard-home: $PINSET_DATA_HOME"
fi
if [ -n "$SHIM_DIR" ]; then
    require_absolute_directory "shim directory" "$SHIM_DIR"
fi
case "$SHIM_BINARY" in
    /*) ;;
    *) fail "shim binary must be an absolute path: $SHIM_BINARY" ;;
esac

# PINSET_HOME is recursively deleted, so reject links, non-directories, and
# common broad roots even when explicitly supplied.
if [ -L "$PINSET_DATA_HOME" ]; then
    fail "PINSET_HOME is a symbolic link; pass its resolved Pinset-owned directory explicitly"
fi
if [ -e "$PINSET_DATA_HOME" ] && [ ! -d "$PINSET_DATA_HOME" ]; then
    fail "PINSET_HOME is not a directory: $PINSET_DATA_HOME"
fi
if [ -d "$PINSET_DATA_HOME" ]; then
    PINSET_DATA_HOME=$(CDPATH= cd -P "$PINSET_DATA_HOME" && pwd -P) \
        || fail "cannot resolve PINSET_HOME: $PINSET_DATA_HOME"
fi
if [ -n "${HOME:-}" ]; then
    for protected in "$HOME" "$HOME/.local" "$HOME/.local/share"; do
        if paths_equal "$PINSET_DATA_HOME" "$protected"; then
            fail "refusing to use a broad PINSET_HOME: $PINSET_DATA_HOME"
        fi
    done
fi
if [ -n "${XDG_DATA_HOME:-}" ] && paths_equal "$PINSET_DATA_HOME" "$XDG_DATA_HOME"; then
    fail "refusing to use XDG_DATA_HOME itself as PINSET_HOME: $PINSET_DATA_HOME"
fi
case "${PINSET_DATA_HOME#/}" in
    */*) ;;
    *) fail "PINSET_HOME is too broad to remove safely: $PINSET_DATA_HOME" ;;
esac
case "${PINSET_DATA_HOME##*/}" in
    [Pp][Ii][Nn][Ss][Ee][Tt]|[Pp][Ii][Nn][Ss][Ee][Tt][-._]*) ;;
    *)
        [ "$ALLOW_NONSTANDARD_HOME" -eq 1 ] \
            || fail "nonstandard PINSET_HOME name requires --allow-nonstandard-home: $PINSET_DATA_HOME"
        ;;
esac
for command_name in rm cmp dirname readlink; do
    command -v "$command_name" >/dev/null 2>&1 || fail "required command not found: $command_name"
done

is_managed_route() {
    route=$1
    [ -e "$route" ] || [ -L "$route" ] || return 1
    [ "$route" != "$INSTALL_DIR/pinset" ] || return 1
    [ "$route" != "$INSTALL_DIR/pinset-shim" ] || return 1

    if [ -L "$route" ]; then
        target=$(readlink "$route" 2>/dev/null || true)
        [ "$target" = "$SHIM_BINARY" ] && return 0
    fi
    [ -f "$route" ] && [ -f "$SHIM_BINARY" ] && cmp -s "$route" "$SHIM_BINARY"
}

list_managed_routes_in() {
    directory=$1
    [ -d "$directory" ] || return 0
    for command_name in $ROUTE_COMMANDS; do
        route="$directory/$command_name"
        if is_managed_route "$route"; then
            printf '  managed command route: %s\n' "$route"
        fi
    done
}

list_all_managed_routes() {
    list_managed_routes_in "$INSTALL_DIR"
    if [ -n "$SHIM_DIR" ] && ! paths_equal "$SHIM_DIR" "$INSTALL_DIR"; then
        list_managed_routes_in "$SHIM_DIR"
    fi
    remaining_path=${PATH:-}
    while [ -n "$remaining_path" ]; do
        case "$remaining_path" in
            *:*) directory=${remaining_path%%:*}; remaining_path=${remaining_path#*:} ;;
            *) directory=$remaining_path; remaining_path= ;;
        esac
        [ -n "$directory" ] || continue
        if ! paths_equal "$directory" "$INSTALL_DIR" \
            && { [ -z "$SHIM_DIR" ] || ! paths_equal "$directory" "$SHIM_DIR"; }; then
            list_managed_routes_in "$directory"
        fi
    done
}

remove_managed_routes_in() {
    directory=$1
    [ -d "$directory" ] || return 0
    for command_name in $ROUTE_COMMANDS; do
        route="$directory/$command_name"
        if is_managed_route "$route"; then
            rm -f -- "$route"
            printf 'Removed managed command route %s\n' "$route"
        fi
    done
}

remove_all_managed_routes() {
    remove_managed_routes_in "$INSTALL_DIR"
    if [ -n "$SHIM_DIR" ] && ! paths_equal "$SHIM_DIR" "$INSTALL_DIR"; then
        remove_managed_routes_in "$SHIM_DIR"
    fi
    remaining_path=${PATH:-}
    while [ -n "$remaining_path" ]; do
        case "$remaining_path" in
            *:*) directory=${remaining_path%%:*}; remaining_path=${remaining_path#*:} ;;
            *) directory=$remaining_path; remaining_path= ;;
        esac
        [ -n "$directory" ] || continue
        if ! paths_equal "$directory" "$INSTALL_DIR" \
            && { [ -z "$SHIM_DIR" ] || ! paths_equal "$directory" "$SHIM_DIR"; }; then
            remove_managed_routes_in "$directory"
        fi
    done
}

printf 'Pinset uninstall plan:\n'
if [ -e "$INSTALL_DIR/pinset" ] || [ -L "$INSTALL_DIR/pinset" ]; then
    printf '  CLI: %s\n' "$INSTALL_DIR/pinset"
fi
if [ -e "$INSTALL_DIR/pinset-shim" ] || [ -L "$INSTALL_DIR/pinset-shim" ]; then
    printf '  command router: %s\n' "$INSTALL_DIR/pinset-shim"
fi
list_all_managed_routes
if [ -e "$PINSET_DATA_HOME" ]; then
    printf '  PINSET_HOME and all managed runtimes: %s\n' "$PINSET_DATA_HOME"
fi
printf '  preserved: project pinset.toml/pinset.lock files, shell profiles, and foreign runtimes\n'

if [ "$DRY_RUN" -eq 1 ]; then
    printf 'Dry run complete. Nothing was removed.\n'
    exit 0
fi
if [ "$ASSUME_YES" -ne 1 ]; then
    printf 'Nothing was removed. Re-run with --yes after reviewing this plan.\n' >&2
    exit 2
fi

# Routes must be checked while pinset-shim still exists, especially for hard-link
# and copy fallbacks. The router and CLI are removed only after that ownership check.
remove_all_managed_routes
for binary in "$INSTALL_DIR/pinset" "$INSTALL_DIR/pinset-shim"; do
    if [ -e "$binary" ] || [ -L "$binary" ]; then
        rm -f -- "$binary"
        printf 'Removed %s\n' "$binary"
    fi
done
if [ -e "$PINSET_DATA_HOME" ]; then
    rm -rf -- "$PINSET_DATA_HOME"
    printf 'Removed PINSET_HOME and all managed runtimes %s\n' "$PINSET_DATA_HOME"
fi

printf 'Pinset uninstall complete. Manually remove any PATH line or PINSET_* variable you added to a shell profile.\n'
