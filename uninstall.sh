#!/usr/bin/env sh
set -eu
plan=0
confirm=0
base="${PINSET_HOME:-${HOME:?HOME is required}/.pinset}"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --yes) confirm=1 ;;
    --plan) plan=1 ;;
    --pinset-home) shift; base="${1:?missing home}" ;;
    --help|-h) printf '%s\n' 'uninstall.sh [--yes] [--plan] [--pinset-home BASE]'; exit 0 ;;
    *) printf 'Unknown option: %s\n' "$1" >&2; exit 2 ;;
  esac
  shift
done
case "$base" in /*) ;; *) echo 'Pinset home must be absolute' >&2; exit 1 ;; esac
case "$base" in /|"$HOME"|*/../*|*/..|*/./*) echo 'Unsafe Pinset home' >&2; exit 1 ;; esac
target="${base%/}/v3"
if [ -L "$target" ]; then echo 'Refusing a linked v3 data directory' >&2; exit 1; fi
if [ ! -d "$target" ]; then printf 'No Pinset 3 data: %s\n' "$target"; exit 0; fi
parent=$(CDPATH= cd -P "${base%/}" && pwd -P)
resolved=$(CDPATH= cd -P "$target" && pwd -P)
[ "$resolved" = "$parent/v3" ] || { echo 'Pinset 3 path escaped its parent' >&2; exit 1; }
[ -f "$resolved/.pinset-home.json" ] && grep -q '"protocol": "pinset/3"' "$resolved/.pinset-home.json" || { echo 'Directory is not owned by Pinset 3' >&2; exit 1; }
printf 'Remove Pinset 3 data and command entries: %s\n' "$resolved"
if [ "$plan" -eq 1 ]; then exit 0; fi
if [ "$confirm" -ne 1 ]; then printf 'Continue? [y/N] '; read -r answer; [ "$answer" = y ] || [ "$answer" = Y ] || exit 0; fi
rm -rf -- "$resolved"
