#!/bin/sh
set -eu
cd /workspace

case "${1:-rust}" in
  rust)
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
    cargo test --workspace --all-features --locked
    python3 scripts/tests/integrations_test.py
    python3 scripts/tests/team_delivery_test.py target/debug/pinset
    ;;
  extension)
    npm ci --prefix editors/vscode
    npm run package --prefix editors/vscode
    ;;
  website)
    cd website
    pnpm install --frozen-lockfile
    pnpm typecheck
    pnpm build
    pnpm validate:seo
    ;;
  native)
    # Docker Desktop's Linux kernel contains "Microsoft". Suppress only VS Code's
    # interactive WSL install prompt inside this disposable container.
    export DONT_PROMPT_WSL_INSTALL=1
    export PINSET_ACCEPTANCE_CACHE=/var/cache/pinset-acceptance
    cargo build --locked -p pinset-cli -p pinset-shim
    npm ci --prefix editors/vscode
    npm run compile --prefix editors/vscode
    npm install --prefix /tmp/pinset-editor-test --no-audit --no-fund @vscode/test-electron@3.1.0
    PINSET_EDITOR_TEST_MODULES=/tmp/pinset-editor-test/node_modules \
      xvfb-run -a python3 scripts/tests/environment_setup_test.py target/debug/pinset
    ;;
  *)
    echo 'Usage: verify.sh rust|extension|website|native' >&2
    exit 2
    ;;
esac
