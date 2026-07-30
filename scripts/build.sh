#!/usr/bin/env bash
# Build script only (§10). Builds the frontend, then the Tauri bundle.
set -euo pipefail
cd "$(dirname "$0")/../app"
npm run build
npm run tauri -- build
