#!/usr/bin/env bash
# Check script only (§10). Mirrors the CI workflow (.github/workflows/ci.yml)
# for local use: frontend lint/typecheck, backend fmt/clippy/test.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "== Frontend =="
cd app
npm run format:check
npm run lint
npm run build
cd ..

echo "== Backend =="
cd app/src-tauri
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
